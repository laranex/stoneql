import { execFileSync } from 'node:child_process';
const metadata = JSON.parse(execFileSync('cargo', ['metadata', '--locked', '--format-version', '1'], {encoding: 'utf8', maxBuffer: 16 * 1024 * 1024}));
const packages = new Map(metadata.packages.map(pkg => [pkg.id, pkg]));
const nodes = new Map(metadata.resolve.nodes.map(node => [node.id, node]));
for (const name of ['stoneql-core', 'stoneql-contracts']) {
  const root = metadata.packages.find(pkg => pkg.name === name);
  if (!root) throw new Error(`Missing shared crate ${name}`);
  const visited = new Set();
  function visit(id) {
    if (visited.has(id)) return;
    visited.add(id);
    const dependency = packages.get(id).name;
    // Host frameworks, drivers, async runtime, SSH transport and OS secret providers stay out.
    if (/^(tauri|axum|sqlx|tokio|russh|ssh-key|security-framework|keyring)(-|$)/.test(dependency)) throw new Error(`${name} imports host/driver/runtime/transport/secret crate ${dependency}`);
    for (const dep of nodes.get(id).dependencies) visit(dep);
  }
  visit(root.id);
  console.log(`${name}: ${visited.size} packages checked; host/driver/runtime boundary intact.`);
}
