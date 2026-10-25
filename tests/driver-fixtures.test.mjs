import test from 'node:test';
import assert from 'node:assert/strict';
import { fixtureEnvironment, assertLocalFixtureDocker } from '../tools/fixture-environment.mjs';

test('driver fixtures do not inherit database credentials, targets or TLS key paths', () => {
  const filtered = fixtureEnvironment({
    PGHOST: 'production.invalid', PGPASSWORD: 'synthetic', PGSSLCERT: '/never/read',
    MYSQL_PWD: 'synthetic', MARIADB_HOST: 'production.invalid', DATABASE_URL: 'synthetic',
    DBADMIN_SPIKE_URL: 'legacy-synthetic', DBADMIN_FIXTURE_PASSWORD: 'legacy-secret', STONEQL_SPIKE_URL: 'synthetic', STONEQL_LIFECYCLE_CA: '/never/read', STONEQL_FIXTURE_PASSWORD: 'synthetic', PATH: '/bin', CARGO_HOME: '/temporary',
    RUSTUP_HOME: '/temporary', DOCKER_CONTEXT: 'desktop-linux',
  });
  assert.deepEqual(filtered, {PATH: '/bin', CARGO_HOME: '/temporary', RUSTUP_HOME: '/temporary', DOCKER_CONTEXT: 'desktop-linux'});
});

test('Docker endpoint validation follows context precedence and rejects remote hosts without inspecting defaults', () => {
  assert.doesNotThrow(() => assertLocalFixtureDocker({DOCKER_CONTEXT:'desktop-linux',DOCKER_HOST:'tcp://unapproved.invalid'}, context => {
    assert.equal(context,'desktop-linux');
    return 'unix:///synthetic-local';
  }));
  assert.throws(() => assertLocalFixtureDocker({DOCKER_HOST:'tcp://unapproved.invalid'}, () => {
    assert.fail('an explicit host must not resolve a default context');
  }), /local Docker/);
});
