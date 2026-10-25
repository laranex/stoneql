import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, readdirSync, rmSync, copyFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';

const cases = [
  {script: 'test-connection-lifecycle.mjs', stage: 'certificate'},
  {script: 'test-connection-lifecycle.mjs', stage: 'shutdown'},
  {script: 'run-driver-spikes.mjs', stage: 'shutdown'},
  {script: 'test-connection-lifecycle.mjs', stage: 'remote'},
  {script: 'run-driver-spikes.mjs', stage: 'remote'},
];
for (const {script,stage} of cases) test(`${script}: ${stage} failure removes private material and scopes Docker cleanup`, () => {
  const temp = mkdtempSync(join(tmpdir(), 'dbadmin-cleanup-regression-'));
  const scratch = join(temp, 'scratch');
  const workspace = join(temp, 'workspace');
  mkdirSync(scratch);
  mkdirSync(join(workspace,'tools'),{recursive:true});
  const log = join(temp, 'commands.jsonl');
  try {
    copyFileSync(resolve(import.meta.dirname,'../Cargo.lock'),join(workspace,'Cargo.lock'));
    // Run exact source copies so synthetic failure tests cannot overwrite real fixture evidence.
    for (const name of [script, 'fixture-environment.mjs']) copyFileSync(resolve(import.meta.dirname,'../tools',name),join(workspace,'tools',name));
    for (const command of ['docker', 'cargo', 'rustc', 'openssl']) {
      writeFileSync(join(temp, command), `#!${process.execPath}
import {appendFileSync,writeFileSync} from 'node:fs';
appendFileSync(process.env.DBADMIN_TEST_COMMAND_LOG, JSON.stringify({command:${JSON.stringify(command)},args:process.argv.slice(2)})+'\\n');
${command === 'docker' ? "if(process.argv[2]==='context') console.log(process.env.DBADMIN_TEST_CONTEXT_ENDPOINT); else if(process.argv[2]==='version') console.log('synthetic'); else process.exit(42);" : command === 'openssl' ? (stage === 'certificate' ? 'process.exit(41);' : "for(const flag of ['-out','-keyout']){const index=process.argv.indexOf(flag);if(index>=0)writeFileSync(process.argv[index+1],'synthetic fixture material');}") : command==='rustc'?"console.log('synthetic rustc');":''}
`, {mode: 0o755});
    }
    const result = spawnSync(process.execPath, [join(workspace,'tools',script)], {
      cwd: workspace, encoding: 'utf8', timeout: 15_000,
      env: {...process.env, PATH: `${temp}:${dirname(process.execPath)}:/usr/bin:/bin`, TMPDIR: scratch, DOCKER_HOST: 'unix:///synthetic-local', DOCKER_CONTEXT: stage === 'remote' ? 'synthetic-remote-context' : '', DBADMIN_TEST_COMMAND_LOG: log, DBADMIN_TEST_CONTEXT_ENDPOINT: stage === 'remote' ? 'tcp://unapproved.invalid:2376' : 'unix:///synthetic-fixture-daemon'},
    });
    assert.equal(result.status, 1);
    assert.deepEqual(readdirSync(scratch), [], 'private fixture directory must be removed on failure');
    assert.match(result.stderr, stage === 'remote' ? /local Docker|Local disposable Docker/ : stage === 'certificate' ? /openssl fixture operation failed/ : /docker.*failed/);
    const commands = readFileSync(log, 'utf8').trim().split('\n').map(line => JSON.parse(line));
    assert.equal(commands.some(call => call.command === 'docker' && call.args.includes('down')), stage === 'shutdown');
    if(stage === 'remote') assert.ok(commands.some(call => call.command === 'docker' && call.args.includes('synthetic-remote-context')));
  } finally { rmSync(temp, {recursive: true, force: true}); }
});
