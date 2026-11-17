//! Linux provider: the desktop's Secret Service (`org.freedesktop.secrets`: GNOME Keyring,
//! KWallet, KeePassXC) over the session D-Bus, through `libdbus` (already linked by the window
//! toolkit). Items carry only `service` and `account` attributes and a fixed label; the value
//! (scope + secret) travels in the item's secret.
//!
//! The session uses the `plain` algorithm: the value crosses the per-user session bus
//! unencrypted. Any process of the same user can already read the item through the same API,
//! so DH session encryption would add new crypto code without changing who can read it.
//!
//! `Unsupported` means no Secret Service is on the session bus (no bus, or nothing owns or can
//! activate the name); it is the only result that selects the encrypted file vault. A present
//! Secret Service that cannot be used (no default collection, a dismissed or timed-out unlock
//! prompt, any other error) is `Failed`: saving and reading fail closed. A locked collection is
//! unlocked only through the service's own prompt.
use super::SecretError;
use dbus::{
    Path,
    arg::{PropMap, RefArg, Variant},
    blocking::{LocalConnection, Proxy},
    channel::Channel,
    message::MatchRule,
};
use std::{
    cell::Cell,
    collections::HashMap,
    rc::Rc,
    time::{Duration, Instant},
};

const BUS_NAME: &str = "org.freedesktop.secrets";
const SERVICE_PATH: &str = "/org/freedesktop/secrets";
const SERVICE: &str = "org.freedesktop.Secret.Service";
const COLLECTION: &str = "org.freedesktop.Secret.Collection";
const ITEM: &str = "org.freedesktop.Secret.Item";
const PROMPT: &str = "org.freedesktop.Secret.Prompt";
pub(super) const LABEL: &str = "StoneQL saved connection credential";
const CALL_TIMEOUT: Duration = Duration::from_secs(10);
/// Below the renderer's 60 s secret deadline, so the host answers first.
const PROMPT_TIMEOUT: Duration = Duration::from_secs(50);

/// `(session, parameters, value, content type)`, the Secret Service `Secret` struct.
type Secret = (Path<'static>, Vec<u8>, Vec<u8>, String);

pub(super) fn set(
    bus: Option<&str>,
    service: &str,
    account: &str,
    value: &[u8],
) -> Result<(), SecretError> {
    let session = Session::open(bus)?;
    let collection = session.default_collection()?;
    session.unlock(vec![collection.clone()])?;
    let mut properties = PropMap::new();
    properties.insert(
        format!("{ITEM}.Label"),
        Variant(Box::new(LABEL.to_owned()) as Box<dyn RefArg>),
    );
    properties.insert(
        format!("{ITEM}.Attributes"),
        Variant(Box::new(attributes(service, account)) as Box<dyn RefArg>),
    );
    let secret: Secret = (
        session.path.clone(),
        Vec::new(),
        value.to_vec(),
        "application/octet-stream".to_owned(),
    );
    let (_, prompt): (Path<'static>, Path<'static>) = session
        .proxy(collection)
        .method_call(COLLECTION, "CreateItem", (properties, secret, true))
        .map_err(failed)?;
    session.prompt(prompt)
}

pub(super) fn get(bus: Option<&str>, service: &str, account: &str) -> Result<Vec<u8>, SecretError> {
    let session = Session::open(bus)?;
    let item = match session.search(service, account)?.as_slice() {
        [] => return Err(SecretError::NotFound),
        [item] => item.clone(),
        // Reference IDs are random; duplicates were planted or copied. Fail closed.
        _ => return Err(SecretError::Failed),
    };
    let ((_, _, value, _),): (Secret,) = session
        .proxy(item)
        .method_call(ITEM, "GetSecret", (session.path.clone(),))
        .map_err(failed)?;
    Ok(value)
}

pub(super) fn delete(bus: Option<&str>, service: &str, account: &str) -> Result<(), SecretError> {
    let session = Session::open(bus)?;
    let items = session.search(service, account)?;
    if items.is_empty() {
        return Err(SecretError::NotFound);
    }
    for item in items {
        let (prompt,): (Path<'static>,) = session
            .proxy(item)
            .method_call(ITEM, "Delete", ())
            .map_err(failed)?;
        session.prompt(prompt)?;
    }
    Ok(())
}

/// Whether a Secret Service answers on the session bus (`Unsupported` when none does).
pub(super) fn probe(bus: Option<&str>) -> Result<(), SecretError> {
    Session::open(bus).map(drop)
}

fn attributes(service: &str, account: &str) -> HashMap<String, String> {
    HashMap::from([
        ("service".to_owned(), service.to_owned()),
        ("account".to_owned(), account.to_owned()),
    ])
}
fn failed(_: dbus::Error) -> SecretError {
    SecretError::Failed
}
/// Errors meaning nothing provides the Secret Service; anything else is a provider failure.
fn absent(error: dbus::Error) -> SecretError {
    match error.name() {
        Some(
            "org.freedesktop.DBus.Error.ServiceUnknown"
            | "org.freedesktop.DBus.Error.NameHasNoOwner"
            | "org.freedesktop.DBus.Error.NoServer"
            | "org.freedesktop.DBus.Error.FileNotFound"
            | "org.freedesktop.DBus.Error.NoSuchFile"
            | "org.freedesktop.DBus.Error.BadAddress"
            | "org.freedesktop.DBus.Error.Disconnected",
        ) => SecretError::Unsupported,
        _ => SecretError::Failed,
    }
}

/// The session bus address without libdbus's X11 autolaunch (which would start a new bus).
fn session_bus() -> Option<String> {
    if let Some(address) = std::env::var_os("DBUS_SESSION_BUS_ADDRESS") {
        return address.into_string().ok();
    }
    let socket = std::path::Path::new(&std::env::var_os("XDG_RUNTIME_DIR")?).join("bus");
    let socket = socket.to_str()?;
    std::path::Path::new(socket)
        .exists()
        .then(|| format!("unix:path={socket}"))
}

struct Session {
    connection: LocalConnection,
    path: Path<'static>,
}
impl Session {
    fn open(bus: Option<&str>) -> Result<Self, SecretError> {
        let address = match bus {
            Some(address) => address.to_owned(),
            None => session_bus().ok_or(SecretError::Unsupported)?,
        };
        // A private connection does not exit the process when the bus goes away.
        let mut channel = Channel::open_private(&address).map_err(|_| SecretError::Unsupported)?;
        channel.register().map_err(|_| SecretError::Unsupported)?;
        let connection = LocalConnection::from(channel);
        let (_, path): (Variant<Box<dyn RefArg>>, Path<'static>) = connection
            .with_proxy(BUS_NAME, SERVICE_PATH, CALL_TIMEOUT)
            .method_call(SERVICE, "OpenSession", ("plain", Variant(String::new())))
            .map_err(absent)?;
        Ok(Self { connection, path })
    }
    fn proxy<'a>(&'a self, path: Path<'a>) -> Proxy<'a, &'a LocalConnection> {
        self.connection.with_proxy(BUS_NAME, path, CALL_TIMEOUT)
    }
    fn service(&self) -> Proxy<'_, &LocalConnection> {
        self.proxy(Path::from(SERVICE_PATH))
    }
    /// Never creates a collection: without a default one, there is no usable store.
    fn default_collection(&self) -> Result<Path<'static>, SecretError> {
        let (collection,): (Path<'static>,) = self
            .service()
            .method_call(SERVICE, "ReadAlias", ("default",))
            .map_err(failed)?;
        if &*collection == "/" {
            return Err(SecretError::Failed);
        }
        Ok(collection)
    }
    /// Every matching item in any collection, unlocking locked ones through the service.
    fn search(&self, service: &str, account: &str) -> Result<Vec<Path<'static>>, SecretError> {
        let (mut unlocked, locked): (Vec<Path<'static>>, Vec<Path<'static>>) = self
            .service()
            .method_call(SERVICE, "SearchItems", (attributes(service, account),))
            .map_err(failed)?;
        if !locked.is_empty() {
            self.unlock(locked.clone())?;
            unlocked.extend(locked);
        }
        Ok(unlocked)
    }
    fn unlock(&self, objects: Vec<Path<'static>>) -> Result<(), SecretError> {
        let (_, prompt): (Vec<Path<'static>>, Path<'static>) = self
            .service()
            .method_call(SERVICE, "Unlock", (objects,))
            .map_err(failed)?;
        self.prompt(prompt)
    }
    /// Runs the service's own prompt (for example its unlock dialog) and waits for the result.
    /// StoneQL never sees or handles a keyring password.
    fn prompt(&self, prompt: Path<'static>) -> Result<(), SecretError> {
        if &*prompt == "/" {
            return Ok(());
        }
        let dismissed = Rc::new(Cell::new(None));
        let result = dismissed.clone();
        let rule = MatchRule::new_signal(PROMPT, "Completed").with_path(prompt.clone());
        self.connection
            .add_match(
                rule,
                move |(value, _): (bool, Variant<Box<dyn RefArg>>), _, _| {
                    result.set(Some(value));
                    false
                },
            )
            .map_err(failed)?;
        self.proxy(prompt.clone())
            .method_call::<(), _, _, _>(PROMPT, "Prompt", ("",))
            .map_err(failed)?;
        let deadline = Instant::now() + PROMPT_TIMEOUT;
        while dismissed.get().is_none() {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                let _ = self
                    .proxy(prompt)
                    .method_call::<(), _, _, _>(PROMPT, "Dismiss", ());
                return Err(SecretError::Failed);
            }
            self.connection
                .process(left.min(Duration::from_secs(1)))
                .map_err(failed)?;
        }
        match dismissed.get() {
            Some(false) => Ok(()),
            _ => Err(SecretError::Failed),
        }
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.proxy(self.path.clone()).method_call::<(), _, _, _>(
            "org.freedesktop.Secret.Session",
            "Close",
            (),
        );
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use std::process::{Child, Command, Stdio};

    /// A private session bus with no service directories, so nothing can be D-Bus-activated.
    pub(crate) struct EmptyBus {
        daemon: Child,
        pub(crate) dir: std::path::PathBuf,
        pub(crate) address: String,
    }
    impl EmptyBus {
        pub(crate) fn start() -> Self {
            let dir =
                std::env::temp_dir().join(format!("stoneql-empty-bus-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir(&dir).unwrap();
            let socket = dir.join("bus");
            let config = dir.join("bus.conf");
            std::fs::write(
                &config,
                format!(
                    "<busconfig><type>session</type><listen>unix:path={}</listen>\
                     <auth>EXTERNAL</auth><policy context=\"default\">\
                     <allow send_destination=\"*\"/><allow receive_sender=\"*\"/>\
                     <allow own=\"*\"/></policy></busconfig>",
                    socket.display()
                ),
            )
            .unwrap();
            let daemon = Command::new("dbus-daemon")
                .arg(format!("--config-file={}", config.display()))
                .arg("--nofork")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .spawn()
                .expect("dbus-daemon is required for the Secret Service tests");
            let deadline = Instant::now() + Duration::from_secs(10);
            while !socket.exists() {
                assert!(Instant::now() < deadline, "private bus did not start");
                std::thread::sleep(Duration::from_millis(20));
            }
            let address = format!("unix:path={}", socket.display());
            Self {
                daemon,
                dir,
                address,
            }
        }
    }
    impl Drop for EmptyBus {
        fn drop(&mut self) {
            let _ = self.daemon.kill();
            let _ = self.daemon.wait();
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn assert_unsupported(bus: &str) {
        let account = uuid::Uuid::new_v4().to_string();
        assert_eq!(
            set(Some(bus), "me.thixpin.stoneql.test", &account, b"v"),
            Err(SecretError::Unsupported)
        );
        assert_eq!(
            get(Some(bus), "me.thixpin.stoneql.test", &account),
            Err(SecretError::Unsupported)
        );
        assert_eq!(
            delete(Some(bus), "me.thixpin.stoneql.test", &account),
            Err(SecretError::Unsupported)
        );
        assert_eq!(probe(Some(bus)), Err(SecretError::Unsupported));
    }

    #[test]
    fn without_a_session_bus_saving_and_reading_fail_closed() {
        let missing = std::env::temp_dir().join(format!("stoneql-no-bus-{}", uuid::Uuid::new_v4()));
        assert_unsupported(&format!("unix:path={}", missing.display()));
    }

    #[test]
    fn a_bus_without_a_secret_service_fails_closed() {
        let bus = EmptyBus::start();
        assert_unsupported(&bus.address);
    }

    /// Runs against the session bus's Secret Service under a per-run service name; driven by
    /// `npm run test:secret-service` (a disposable unlocked keyring on a private bus).
    #[test]
    #[ignore = "needs a disposable Secret Service: npm run test:secret-service"]
    fn items_round_trip_in_the_session_secret_service() {
        let service = std::env::var("STONEQL_KEYCHAIN_SERVICE").unwrap();
        assert!(service.starts_with("me.thixpin.stoneql.test-"));
        let account = uuid::Uuid::new_v4().to_string();
        assert_eq!(get(None, &service, &account), Err(SecretError::NotFound));
        set(None, &service, &account, b"synthetic-value").unwrap();
        assert_eq!(get(None, &service, &account).unwrap(), b"synthetic-value");
        set(None, &service, &account, b"replaced").unwrap();
        assert_eq!(get(None, &service, &account).unwrap(), b"replaced");

        // The item is labelled, and its attributes hold the two names only (no secret).
        let session = Session::open(None).unwrap();
        let items = session.search(&service, &account).unwrap();
        assert_eq!(items.len(), 1);
        let item = items[0].clone();
        use dbus::blocking::stdintf::org_freedesktop_dbus::Properties;
        let proxy = session.proxy(item.clone());
        let label: String = proxy.get(ITEM, "Label").unwrap();
        assert_eq!(label, LABEL);
        let mut stored: HashMap<String, String> = proxy.get(ITEM, "Attributes").unwrap();
        // GNOME Keyring adds its generic schema name to items created without one.
        stored.remove("xdg:schema");
        assert_eq!(stored, attributes(&service, &account));
        drop(session);

        // Another reference or service never sees it.
        assert_eq!(
            get(None, &service, &uuid::Uuid::new_v4().to_string()),
            Err(SecretError::NotFound)
        );
        assert_eq!(
            get(None, &format!("{service}-other"), &account),
            Err(SecretError::NotFound)
        );
        delete(None, &service, &account).unwrap();
        assert_eq!(get(None, &service, &account), Err(SecretError::NotFound));
        assert_eq!(delete(None, &service, &account), Err(SecretError::NotFound));
    }
}
