// Real-process validation of the durable unknown-outcome journal (M006, D025-13): repeatedly
// SIGKILLs `examples/journal_crash.rs work` at seeded random moments and checks, on every
// restart, that no unknown outcome is lost, no acknowledged one comes back, every transaction
// left open by the kill is listed, and the journal reopens without a problem. Then checks that a
// second process cannot open a journal another process holds. Headless; uses a disposable SQLite
// file and a private temporary journal directory, never the app data directory.
import { spawn, spawnSync } from 'node:child_process';
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

const root = resolve(import.meta.dirname, '..');
const cycles = Number(process.argv[2] ?? 40);
if (!Number.isInteger(cycles) || cycles < 2 || cycles > 500) throw Error('cycles must be 2..500');
const build = spawnSync('cargo', ['build', '--locked', '-q', '-p', 'stoneql-runtime', '--example', 'journal_crash'], { cwd: root, stdio: 'inherit' });
if (build.status !== 0) throw Error('journal_crash build failed');
const binary = resolve(root, 'target/debug/examples/journal_crash');
// Canonical path: the runtime refuses SQLite grants through symlinks (macOS /var → /private/var).
const temp = realpathSync(mkdtempSync(join(tmpdir(), 'stoneql-journal-')));
const dir = join(temp, 'journal'), db = join(temp, 't.sqlite'), log = join(temp, 'events.ndjson');
let seed = 0x5eed;
const random = () => ((seed = (seed * 1103515245 + 12345) & 0x7fffffff) / 0x7fffffff);
const exited = (child) => new Promise((done) => (child.exitCode !== null || child.signalCode ? done() : child.once('exit', done)));

const report = { cycles, kills: 0, restoredChecks: 0, unknownSeen: 0, acknowledged: 0, openLeasesAtKill: 0, lost: [], resurrected: [], openLeasesMissing: [], problems: [] };
try {
  for (let cycle = 0; cycle < cycles; cycle++) {
    const child = spawn(binary, ['work', dir, db, log, String(1 + cycle)], { stdio: 'ignore' });
    await new Promise((done) => setTimeout(done, 250 + Math.floor(random() * 1250)));
    child.kill('SIGKILL');
    await exited(child);
    report.kills++;
  }
  // One clean probe after the last kill restores the final state.
  const runProbe = () => {
    const result = spawnSync(binary, ['probe', dir, db], { encoding: 'utf8', timeout: 30_000 });
    if (result.status !== 0) throw Error(`probe failed: ${result.stderr.slice(-2000)}`);
    return JSON.parse(result.stdout.trim());
  };
  const probe = runProbe();

  // Replay the event log segment by segment (each process starts with `open-start`).
  const events = readFileSync(log, 'utf8').split('\n').filter(Boolean).map((line) => JSON.parse(line));
  const segments = [];
  for (const event of events) {
    if (event.ev === 'open-start') segments.push([]);
    segments.at(-1)?.push(event);
  }
  const expected = new Set(), acked = new Set();
  const check = (restored, at) => {
    report.restoredChecks++;
    for (const id of expected) if (!restored.has(id)) report.lost.push({ at, id });
    for (const id of acked) if (restored.has(id)) report.resurrected.push({ at, id });
  };
  segments.forEach((segment, index) => {
    const opened = segment.find((event) => event.ev === 'open');
    if (!opened) return; // killed before the journal opened: nothing to check
    if (opened.problem) report.problems.push({ at: index, problem: opened.problem });
    const restored = new Set(opened.listing.outcomes.map((outcome) => outcome.outcomeId));
    if (index > 0) check(restored, index);
    // Everything restored stays expected until acknowledged.
    for (const id of restored) if (!acked.has(id)) expected.add(id);
    // Transactions open at the kill. One whose end had started is ambiguous: the runtime may have
    // concluded it (confirmed rollback/commit, journal record removed) just before the kill,
    // before this process logged `ended`; either listed-as-unknown or concluded is correct.
    const leases = new Set(), ending = new Set();
    let pendingAck;
    for (const event of segment) {
      if (event.ev === 'unknown' && event.outcome) { expected.add(event.id); report.unknownSeen++; }
      if (event.ev === 'begun') leases.add(event.id);
      if (event.ev === 'end-start') { leases.delete(event.id); ending.add(event.id); }
      if (event.ev === 'ended') ending.delete(event.id);
      if (event.ev === 'ack-start') pendingAck = event.id;
      if (event.ev === 'ack-done') { acked.add(event.id); expected.delete(event.id); pendingAck = undefined; report.acknowledged++; }
      if (event.ev === 'ack-err') pendingAck = undefined;
    }
    // Killed mid-acknowledgement: either state is correct.
    if (pendingAck) expected.delete(pendingAck);
    report.endingAtKill = (report.endingAtKill ?? 0) + ending.size;
    // A transaction left open by the kill must be listed as unknown after the restart.
    report.openLeasesAtKill += leases.size;
    const next = segments[index + 1]?.find((event) => event.ev === 'open');
    if (next) {
      const listed = new Set(next.listing.outcomes.map((outcome) => outcome.outcomeId));
      for (const id of leases) if (!listed.has(id)) report.openLeasesMissing.push({ at: index, id });
      for (const id of leases) expected.add(id);
    }
  });
  const final = new Set(probe.listing.outcomes.map((outcome) => outcome.outcomeId));
  if (probe.problem) report.problems.push({ at: 'probe', problem: probe.problem });
  check(final, 'probe');
  report.finalListed = final.size;

  // Single-writer lock: a second process must not open a journal another process holds.
  const holder = spawn(binary, ['hold', dir], { stdio: ['ignore', 'pipe', 'ignore'] });
  const held = await new Promise((done) => {
    let text = '';
    holder.stdout.on('data', (chunk) => { text += chunk; if (text.includes('\n')) done(JSON.parse(text.split('\n')[0])); });
  });
  const secondRun = spawnSync(binary, ['probe', dir, db], { encoding: 'utf8', timeout: 30_000 });
  const second = secondRun.status === 0 ? JSON.parse(secondRun.stdout.trim()) : { problem: 'process failed', begin: 'Err', submit: 'Err', stderr: secondRun.stderr.slice(-500) };
  holder.kill('SIGKILL');
  await exited(holder);
  report.lock = { holderProblem: held.problem, secondProblem: second.problem, secondBegin: second.begin, secondSubmit: second.submit };
  const refused = (text) => /Err/.test(text);
  const passed = !report.lost.length && !report.resurrected.length && !report.openLeasesMissing.length && !report.problems.length
    && held.problem === null && second.problem !== null && refused(second.begin) && refused(second.submit);
  console.log(JSON.stringify(report));
  if (!passed) {
    // Keep the synthetic event log (ids, codes, timestamps only) for diagnosis.
    mkdirSync(resolve(root, 'artifacts/m006'), { recursive: true });
    copyFileSync(log, resolve(root, 'artifacts/m006/journal-crash-events.ndjson'));
    throw Error('journal crash validation failed; events kept in artifacts/m006/journal-crash-events.ndjson');
  }
  console.log(`journal: ${report.kills} SIGKILL restarts, ${report.unknownSeen} unknown outcomes, ${report.acknowledged} acknowledged, ${report.openLeasesAtKill} open transactions at kill — none lost or resurrected; second process refused`);
} finally {
  rmSync(temp, { recursive: true, force: true });
  if (existsSync(temp)) throw Error('temporary journal directory remains');
}
