//! SSH tunnel transport. One SSH session per logical connection handle, a verified enrolled
//! host key, agent/password/key-file authentication and a private Unix-socket endpoint.
//!
//! The database adapter dials the endpoint but keeps its own protocol, TLS mode and the
//! original database hostname for certificate verification. The SSH server resolves and
//! reaches that hostname; StoneQL never rewrites it to the local endpoint. Agent forwarding,
//! sessions, shells and remote forwards are never requested.
//!
//! Unix-socket endpoints exist only on Unix hosts; elsewhere tunnels are explicitly unsupported.
#![cfg_attr(not(unix), allow(dead_code))]
use super::LifecycleError;
use russh::{
    client,
    keys::{PrivateKeyWithHashAlg, PublicKey, PublicKeyOrCertificate, agent::client::AgentClient},
};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

/// Bounded agent identity attempts; servers commonly disconnect after six failures.
const MAX_AGENT_IDENTITIES: usize = 5;
/// Private keys are small text files; anything larger is refused before parsing.
pub(crate) const MAX_KEY_FILE_BYTES: u64 = 16_384;
/// Concurrent forwarded streams per tunnel: the pool, a job or lease connection and a cancel
/// connection use at most three; the fourth absorbs reconnects. Streams beyond it are closed at
/// once, so a cancel connection fails fast instead of waiting (Contract 5.2).
const MAX_FORWARDS: usize = 4;

/// Trusted SSH tunnel configuration. Never derives Debug/Serialize: it may carry secrets.
pub struct SshConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    /// Enrolled OpenSSH public key (`<algorithm> <base64>`). `None` is an unknown host.
    pub host_key: Option<String>,
    pub auth: SshAuth,
}

/// Authentication material resolved by the trusted host. The renderer supplies at most a
/// session-only secret or an opaque reference; the host resolves references before this point.
pub enum SshAuth {
    /// Agent socket (Unix) or named pipe (Windows) chosen by the trusted host; `None` = unavailable.
    Agent(Option<PathBuf>),
    Password(String),
    /// Owner-scoped key-file grant issued after the native file picker.
    KeyFile {
        grant: String,
        passphrase: Option<String>,
    },
}

/// Public host key presented by a server, for explicit user verification and enrollment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostKey {
    pub algorithm: String,
    pub public_key: String,
    pub fingerprint: String,
}

fn text_field(value: &str, max: usize) -> bool {
    !value.is_empty()
        && value.len() <= max
        && !value
            .chars()
            .any(|character| character.is_whitespace() || character.is_control())
}

pub(crate) fn validate_endpoint(host: &str, port: u16) -> Result<(), LifecycleError> {
    if !text_field(host, 253) || port == 0 {
        return Err(LifecycleError::InvalidConfiguration);
    }
    Ok(())
}

impl SshConfig {
    pub(crate) fn validate(&self) -> Result<(), LifecycleError> {
        validate_endpoint(&self.host, self.port)?;
        let secret_ok = |secret: &str| secret.len() <= 16_384 && !secret.contains('\0');
        if !text_field(&self.username, 256)
            || self.host_key.as_ref().is_some_and(|key| key.len() > 8192)
            || match &self.auth {
                SshAuth::Agent(_) => false,
                SshAuth::Password(password) => password.is_empty() || !secret_ok(password),
                SshAuth::KeyFile { passphrase, .. } => {
                    passphrase.as_deref().is_some_and(|value| !secret_ok(value))
                }
            }
        {
            return Err(LifecycleError::InvalidConfiguration);
        }
        Ok(())
    }
}

/// Parses an enrolled key. Certificates and malformed keys are configuration errors.
pub(crate) fn parse_host_key(key: &str) -> Result<PublicKey, LifecycleError> {
    PublicKey::from_openssh(key.trim()).map_err(|_| LifecycleError::InvalidConfiguration)
}

/// Parses an OpenSSH public key and derives its algorithm and SHA256 fingerprint. Used by hosts
/// to validate stored trust entries; the fingerprint is never taken from storage.
pub fn describe_host_key(openssh: &str) -> Result<HostKey, LifecycleError> {
    describe(&parse_host_key(openssh)?)
}
pub(crate) fn describe(key: &PublicKey) -> Result<HostKey, LifecycleError> {
    let mut public_key = key.clone();
    public_key.set_comment("");
    Ok(HostKey {
        algorithm: key.algorithm().as_str().to_owned(),
        public_key: public_key
            .to_openssh()
            .map_err(|_| LifecycleError::Unavailable)?
            .trim()
            .to_owned(),
        fingerprint: key.fingerprint(russh::keys::HashAlg::Sha256).to_string(),
    })
}

enum KeyCheck {
    Matched,
    Unknown(PublicKey),
    Changed,
    Certificate,
}

/// Accepts exactly the enrolled key. Unknown, changed and certificate host keys abort the
/// key exchange before any authentication material is sent.
struct Verifier {
    expected: Option<PublicKey>,
    outcome: Arc<Mutex<Option<KeyCheck>>>,
    /// Dropped with the handler when russh's session task ends on any path (remote disconnect,
    /// keepalive timeout, I/O error, our own close). Receivers then observe the tunnel as dead
    /// (F-03); `Handler::disconnected` alone can be skipped when the final shutdown fails.
    _alive: Option<tokio::sync::watch::Sender<()>>,
}
impl client::Handler for Verifier {
    type Error = russh::Error;
    async fn check_server_key(
        &mut self,
        presented: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let check = match presented {
            PublicKeyOrCertificate::Certificate(_) => KeyCheck::Certificate,
            PublicKeyOrCertificate::PublicKey { key, .. } => match &self.expected {
                Some(expected) if expected.key_data() == key.key_data() => KeyCheck::Matched,
                Some(_) => KeyCheck::Changed,
                None => KeyCheck::Unknown(key.clone()),
            },
        };
        let accepted = matches!(check, KeyCheck::Matched);
        *self.outcome.lock().expect("host key outcome lock") = Some(check);
        Ok(accepted)
    }
}

fn client_config() -> Arc<client::Config> {
    Arc::new(client::Config {
        // Detects a dead server or path; the tunnel then reports itself closed.
        keepalive_interval: Some(Duration::from_secs(30)),
        keepalive_max: 3,
        nodelay: true,
        ..Default::default()
    })
}

type Session = client::Handle<Verifier>;

async fn handshake(
    host: &str,
    port: u16,
    expected: Option<PublicKey>,
) -> Result<(Session, tokio::sync::watch::Receiver<()>), LifecycleError> {
    let socket = tokio::net::TcpStream::connect((host, port))
        .await
        .map_err(|_| LifecycleError::Unavailable)?;
    let _ = socket.set_nodelay(true);
    let outcome = Arc::new(Mutex::new(None));
    let (alive, alive_receiver) = tokio::sync::watch::channel(());
    let verifier = Verifier {
        expected,
        outcome: outcome.clone(),
        _alive: Some(alive),
    };
    match client::connect_stream(client_config(), socket, verifier).await {
        Ok(session) => Ok((session, alive_receiver)),
        Err(_) => Err(
            match outcome.lock().expect("host key outcome lock").take() {
                Some(KeyCheck::Unknown(_) | KeyCheck::Certificate) => {
                    LifecycleError::HostKeyUnknown
                }
                Some(KeyCheck::Changed) => LifecycleError::HostKeyChanged,
                _ => LifecycleError::Unavailable,
            },
        ),
    }
}

/// Reads the server's host key and disconnects before authentication. Used only to show the
/// fingerprint for explicit enrollment; it never marks a key as trusted.
pub(crate) async fn scan(
    host: &str,
    port: u16,
    deadline: Duration,
) -> Result<HostKey, LifecycleError> {
    validate_endpoint(host, port)?;
    let socket = tokio::time::timeout(deadline, tokio::net::TcpStream::connect((host, port)))
        .await
        .map_err(|_| LifecycleError::Timeout)?
        .map_err(|_| LifecycleError::Unavailable)?;
    let outcome = Arc::new(Mutex::new(None));
    let verifier = Verifier {
        expected: None,
        outcome: outcome.clone(),
        _alive: None,
    };
    let _ = tokio::time::timeout(
        deadline,
        client::connect_stream(client_config(), socket, verifier),
    )
    .await
    .map_err(|_| LifecycleError::Timeout)?;
    let check = outcome.lock().expect("host key outcome lock").take();
    match check {
        Some(KeyCheck::Unknown(key)) => describe(&key),
        // Host certificates need a separate CA trust policy; not supported.
        Some(KeyCheck::Certificate) => Err(LifecycleError::Unsupported),
        _ => Err(LifecycleError::Unavailable),
    }
}

async fn authenticate(
    session: &mut Session,
    username: &str,
    auth: SshAuth,
    key_text: Option<String>,
) -> Result<(), LifecycleError> {
    let accepted = |result: Result<client::AuthResult, russh::Error>| {
        result
            .map_err(|_| LifecycleError::Unavailable)
            .map(|result| result.success())
    };
    let success = match auth {
        SshAuth::Password(password) => {
            accepted(session.authenticate_password(username, password).await)?
        }
        SshAuth::KeyFile { passphrase, .. } => {
            let text = key_text.ok_or(LifecycleError::Forbidden)?;
            // Encrypted keys without (or with a wrong) passphrase fail like any other
            // credential; RSA keys are unsupported in this build (see D016).
            let key = russh::keys::decode_secret_key(&text, passphrase.as_deref())
                .map_err(|_| LifecycleError::SshAuthentication)?;
            if key.algorithm().is_rsa() {
                return Err(LifecycleError::Unsupported);
            }
            accepted(
                session
                    .authenticate_publickey(
                        username,
                        PrivateKeyWithHashAlg::new(Arc::new(key), None),
                    )
                    .await,
            )?
        }
        SshAuth::Agent(socket) => {
            let socket = socket.ok_or(LifecycleError::AgentUnavailable)?;
            // Unix: an OpenSSH agent socket. Windows: the OpenSSH agent's named pipe.
            #[cfg(unix)]
            let agent = AgentClient::connect_uds(&socket).await;
            #[cfg(windows)]
            let agent = AgentClient::connect_named_pipe(&socket).await;
            let mut agent = agent.map_err(|_| LifecycleError::AgentUnavailable)?;
            let identities = agent
                .request_identities()
                .await
                .map_err(|_| LifecycleError::AgentUnavailable)?;
            let mut success = false;
            for identity in identities
                .iter()
                .filter_map(|identity| match identity {
                    russh::keys::agent::AgentIdentity::PublicKey { key, .. } => Some(key),
                    _ => None,
                })
                .filter(|key| !key.algorithm().is_rsa())
                .take(MAX_AGENT_IDENTITIES)
            {
                match session
                    .authenticate_publickey_with(username, identity.clone(), None, &mut agent)
                    .await
                {
                    Ok(result) if result.success() => {
                        success = true;
                        break;
                    }
                    Ok(_) => {}
                    Err(_) => return Err(LifecycleError::AgentUnavailable),
                }
            }
            // The agent connection is dropped here: it is used only to sign authentication
            // challenges and is never exposed to the server.
            success
        }
    };
    if success {
        Ok(())
    } else {
        Err(LifecycleError::SshAuthentication)
    }
}

/// A live tunnel: SSH session, private endpoint directory and the accept/forward task.
/// Dropping it (setup cancellation, failure, close) aborts forwarding, ends the SSH session
/// and removes the endpoint directory.
pub(crate) struct Tunnel {
    session: Arc<Session>,
    endpoint: Endpoint,
    accept: Option<tokio::task::JoinHandle<()>>,
    alive: tokio::sync::watch::Receiver<()>,
}

/// Resolves once the SSH session task has ended (its handler, the only sender, was dropped).
async fn session_ended(mut alive: tokio::sync::watch::Receiver<()>) {
    while alive.changed().await.is_ok() {}
}

/// Private per-tunnel directory (mode 0700) holding exactly one listening socket.
pub(crate) struct Endpoint {
    pub(crate) directory: PathBuf,
    pub(crate) socket: PathBuf,
}
impl Drop for Endpoint {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.socket);
        let _ = std::fs::remove_dir(&self.directory);
    }
}

impl Tunnel {
    pub(crate) fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }
    /// Resolves when the SSH session has died, so the registry can release the handle (F-03).
    pub(crate) fn closed(&self) -> impl std::future::Future<Output = ()> + Send + 'static {
        session_ended(self.alive.clone())
    }
    pub(crate) fn is_closed(&self) -> bool {
        self.session.is_closed() || self.accept.as_ref().is_none_or(|task| task.is_finished())
    }
    /// Graceful close: stop accepting/forwarding, then send an SSH disconnect.
    pub(crate) async fn close(mut self) {
        if let Some(task) = self.accept.take() {
            task.abort();
            let _ = task.await;
        }
        let _ = self
            .session
            .disconnect(russh::Disconnect::ByApplication, "", "")
            .await;
    }
}
impl Drop for Tunnel {
    fn drop(&mut self) {
        if let Some(task) = self.accept.take() {
            task.abort();
        }
    }
}

/// Root for private endpoint directories. Kept short: Unix socket paths are limited to about
/// 104 bytes on macOS.
pub fn endpoint_root() -> PathBuf {
    std::env::temp_dir()
}
pub const ENDPOINT_PREFIX: &str = "stoneql-ssh-";

#[cfg(unix)]
fn bind(socket_name: &str) -> Result<(Endpoint, tokio::net::UnixListener), LifecycleError> {
    use std::os::unix::fs::DirBuilderExt;
    let name = uuid::Uuid::new_v4().simple().to_string();
    let directory = endpoint_root().join(format!("{ENDPOINT_PREFIX}{}", &name[..16]));
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&directory)
        .map_err(|_| LifecycleError::Unavailable)?;
    let socket = directory.join(socket_name);
    let endpoint = Endpoint {
        directory,
        socket: socket.clone(),
    };
    if socket.as_os_str().len() > 100 {
        return Err(LifecycleError::Unsupported);
    }
    let listener =
        tokio::net::UnixListener::bind(&socket).map_err(|_| LifecycleError::Unavailable)?;
    Ok((endpoint, listener))
}

#[cfg(unix)]
async fn forward(
    session: Arc<Session>,
    mut local: tokio::net::UnixStream,
    host: String,
    port: u16,
) {
    let Ok(channel) = session
        .channel_open_direct_tcpip(host, u32::from(port), "127.0.0.1", 0)
        .await
    else {
        return;
    };
    use tokio::io::AsyncWriteExt;
    let remote = channel.into_stream();
    let (mut local_read, mut local_write) = local.split();
    let (mut remote_read, mut remote_write) = tokio::io::split(remote);
    // As `copy_bidirectional`, except that the forward (its slot and SSH channel) ends as soon
    // as the local side is done. Database clients never half-close, so that means StoneQL
    // closed or discarded the connection; waiting for the server as well would let an abandoned
    // statement hold a forward until it ends server-side, exhausting the tunnel for later
    // cancels (Contract 5.2). A server EOF only half-closes the local side, so the client can
    // still finish its own close.
    let upstream = async {
        let _ = tokio::io::copy(&mut local_read, &mut remote_write).await;
        // Delivers anything written before the close (for example COM_QUIT) and an EOF.
        let _ = remote_write.shutdown().await;
    };
    let downstream = async {
        let _ = tokio::io::copy(&mut remote_read, &mut local_write).await;
        let _ = local_write.shutdown().await;
        std::future::pending::<()>().await
    };
    tokio::select! {
        () = upstream => {}
        () = downstream => {}
    }
}

#[cfg(unix)]
fn serve(
    session: Arc<Session>,
    listener: tokio::net::UnixListener,
    host: String,
    port: u16,
    alive: tokio::sync::watch::Receiver<()>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        // Owned by this task: aborting it drops every forward and its SSH channel.
        let mut forwards = tokio::task::JoinSet::new();
        let ended = session_ended(alive);
        tokio::pin!(ended);
        loop {
            tokio::select! {
                // A dead session accepts nothing more; stop serving (F-03).
                () = &mut ended => break,
                accepted = listener.accept() => {
                    let Ok((stream, _)) = accepted else { break };
                    // F-4: finished forwards still count in `len()` until reaped; reap them first
                    // so a free slot is never refused (select! picks branches at random).
                    while forwards.try_join_next().is_some() {}
                    if forwards.len() >= MAX_FORWARDS || session.is_closed() {
                        continue;
                    }
                    forwards.spawn(forward(session.clone(), stream, host.clone(), port));
                }
                Some(_) = forwards.join_next(), if !forwards.is_empty() => {}
            }
        }
    })
}

/// Opens a verified, authenticated tunnel to `target_host:target_port` as resolved by the SSH
/// server. Every await happens inside the caller's deadline/cancellation scope; partial state
/// is released by drop.
pub(crate) async fn open(
    config: SshConfig,
    key_text: Option<String>,
    target_host: &str,
    target_port: u16,
    socket_name: &str,
) -> Result<Tunnel, LifecycleError> {
    config.validate()?;
    let expected = parse_host_key(
        config
            .host_key
            .as_deref()
            .ok_or(LifecycleError::HostKeyUnknown)?,
    )?;
    #[cfg(not(unix))]
    {
        let _ = (expected, key_text, target_host, target_port, socket_name);
        Err(LifecycleError::Unsupported)
    }
    #[cfg(unix)]
    {
        let (mut session, alive) = handshake(&config.host, config.port, Some(expected)).await?;
        authenticate(&mut session, &config.username, config.auth, key_text).await?;
        let session = Arc::new(session);
        let (endpoint, listener) = bind(socket_name)?;
        let accept = serve(
            session.clone(),
            listener,
            target_host.to_owned(),
            target_port,
            alive.clone(),
        );
        Ok(Tunnel {
            session,
            endpoint,
            accept: Some(accept),
            alive,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const ED25519: &str =
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl";
    fn config(auth: SshAuth) -> SshConfig {
        SshConfig {
            host: "bastion.example".into(),
            port: 22,
            username: "deploy".into(),
            host_key: Some(ED25519.into()),
            auth,
        }
    }
    #[test]
    fn host_keys_parse_and_describe_without_comments() {
        let key = parse_host_key(&format!("{ED25519} user@laptop")).unwrap();
        let described = describe(&key).unwrap();
        assert_eq!(described.algorithm, "ssh-ed25519");
        assert_eq!(described.public_key, ED25519);
        assert!(described.fingerprint.starts_with("SHA256:"));
        assert!(parse_host_key("ssh-ed25519 not-base64").is_err());
        assert!(parse_host_key("").is_err());
    }
    #[test]
    fn configuration_rejects_ambiguous_or_oversized_values() {
        assert!(config(SshAuth::Agent(None)).validate().is_ok());
        let mut bad = config(SshAuth::Password(String::new()));
        assert!(bad.validate().is_err());
        bad.auth = SshAuth::Password("secret\0".into());
        assert!(bad.validate().is_err());
        for host in ["", "two words", "line\nbreak"] {
            let mut input = config(SshAuth::Agent(None));
            input.host = host.into();
            assert!(input.validate().is_err());
        }
        let mut input = config(SshAuth::Agent(None));
        input.port = 0;
        assert!(input.validate().is_err());
        input = config(SshAuth::Agent(None));
        input.username = "a b".into();
        assert!(input.validate().is_err());
    }
    #[tokio::test]
    async fn unknown_host_key_is_refused_before_any_network_access() {
        let mut input = config(SshAuth::Password("never-sent".into()));
        input.host_key = None;
        // Port 9 on an unroutable documentation address would hang if dialled.
        input.host = "192.0.2.1".into();
        input.port = 9;
        assert!(matches!(
            open(input, None, "db", 3306, "s").await,
            Err(LifecycleError::HostKeyUnknown)
        ));
    }
}

#[cfg(test)]
mod session_end_tests {
    /// F-03 plumbing: the end signal resolves once the handler (sole sender) is dropped.
    #[tokio::test]
    async fn session_end_resolves_when_the_handler_is_dropped() {
        let (alive, receiver) = tokio::sync::watch::channel(());
        let ended = super::session_ended(receiver);
        tokio::pin!(ended);
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(50), &mut ended)
                .await
                .is_err(),
            "pending while the session lives"
        );
        drop(alive);
        tokio::time::timeout(std::time::Duration::from_secs(1), ended)
            .await
            .expect("resolves after the handler is dropped");
    }
}
