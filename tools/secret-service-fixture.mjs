// Disposable Secret Service for Linux fixtures: a private session bus (no service directories,
// so nothing is D-Bus-activated) and a gnome-keyring-daemon whose login keyring lives in a
// temporary XDG data directory and is unlocked with a password generated here. The password is
// piped on stdin and never printed or stored; the user's keyrings and session bus are not used.
import { spawn, spawnSync } from 'node:child_process';
import { randomBytes } from 'node:crypto';
import { mkdirSync, writeFileSync, existsSync, rmSync } from 'node:fs';
import { join } from 'node:path';

const waitFor = async (condition, what, ms = 10_000) => {
  const deadline = Date.now() + ms;
  while (!condition()) {
    if (Date.now() > deadline) throw Error(`${what} did not start`);
    await new Promise(resolve => setTimeout(resolve, 50));
  }
};
const exited = child => child.exitCode !== null || child.signalCode !== null;
const stop = async child => {
  if (!child || exited(child)) return;
  child.kill('SIGTERM');
  await new Promise(resolve => exited(child) ? resolve() : child.once('exit', resolve));
};

/**
 * Starts the bus and keyring under `directory` (a private, empty directory). `unlock: false`
 * starts the daemon without a login keyring: a Secret Service that is present but unusable.
 */
export async function startSecretService(directory, baseEnv, { unlock = true } = {}) {
  const run = join(directory, 'run'), data = join(directory, 'data'), home = join(directory, 'home');
  for (const path of [run, data, home]) mkdirSync(path, { mode: 0o700 });
  const socket = join(directory, 'bus');
  writeFileSync(join(directory, 'bus.conf'), `<busconfig><type>session</type><listen>unix:path=${socket}</listen><auth>EXTERNAL</auth>
<policy context="default"><allow send_destination="*"/><allow receive_sender="*"/><allow own="*"/></policy></busconfig>\n`);
  const bus = spawn('dbus-daemon', [`--config-file=${join(directory, 'bus.conf')}`, '--nofork'], { stdio: 'ignore' });
  let keyring;
  try {
    await waitFor(() => existsSync(socket), 'private session bus');
    const env = {
      ...baseEnv,
      DBUS_SESSION_BUS_ADDRESS: `unix:path=${socket}`,
      XDG_RUNTIME_DIR: run, XDG_DATA_HOME: data, HOME: home,
    };
    keyring = spawn('gnome-keyring-daemon', ['--foreground', ...(unlock ? ['--unlock'] : []), '--components=secrets'], { env, stdio: ['pipe', 'ignore', 'ignore'] });
    keyring.stdin.end(unlock ? randomBytes(24).toString('hex') : '');
    await waitFor(() => exited(keyring) || spawnSync('busctl', [`--address=unix:path=${socket}`, 'status', 'org.freedesktop.secrets'], { stdio: 'ignore' }).status === 0, 'gnome-keyring-daemon');
    if (exited(keyring)) throw Error('gnome-keyring-daemon exited');
    return {
      env,
      /** Items under `service` (count only: secret-tool's listing would include values). */
      itemsRemaining(service) {
        const result = spawnSync('secret-tool', ['search', '--all', 'service', service], { env, encoding: 'utf8', timeout: 10_000 });
        if (result.error || result.status > 1) throw Error('Secret Service lookup failed');
        return (result.stdout.match(/^\[/gm) || []).length;
      },
      /** Removes items left under `service` by a failed run; returns how many there were. */
      purge(service) {
        const count = this.itemsRemaining(service);
        if (count) spawnSync('secret-tool', ['clear', 'service', service], { env, stdio: 'ignore', timeout: 10_000 });
        return count;
      },
      async stop() {
        await stop(keyring);
        await stop(bus);
        rmSync(directory, { recursive: true, force: true });
      },
    };
  } catch (error) {
    await stop(keyring);
    await stop(bus);
    throw error;
  }
}
