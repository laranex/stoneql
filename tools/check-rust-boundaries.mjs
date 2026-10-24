import { execFileSync } from 'node:child_process';
const metadata = JSON.parse(execFileSync('cargo', ['metadata', '--locked', '--format-version', '1'], {encoding: 'utf8'}));
const packages = new Map(metadata.packages.map(pkg => [pkg.id, pkg]));
const nodes = new Map(metadata.resolve.nodes.map(node => [node.id, node]));
for (const name of ['dbadmin-core', 'dbadmin-contracts']) {
  const root = metadata.packages.find(pkg => pkg.name === name);
  if (!root) throw new Error(`Missing shared crate ${name}`);
  const visited = new Set();
  function visit(id) {
    if (visited.has(id)) return;
    visited.add(id);
    const dependency = packages.get(id).name;
    if (/^(tauri|axum|sqlx|tokio)(-|$)/.test(dependency)) throw new Error(`${name} imports host/driver/runtime crate ${dependency}`);
    for (const dep of nodes.get(id).dependencies) visit(dep);
  }
  visit(root.id);
  console.log(`${name}: ${visited.size} packages checked; host/driver/runtime boundary intact.`);
}
