import test from 'node:test';
import assert from 'node:assert/strict';
import { fixtureEnvironment } from '../tools/fixture-environment.mjs';

test('driver fixtures do not inherit database credentials, targets or TLS key paths', () => {
  const filtered = fixtureEnvironment({
    PGHOST: 'production.invalid', PGPASSWORD: 'synthetic', PGSSLCERT: '/never/read',
    MYSQL_PWD: 'synthetic', MARIADB_HOST: 'production.invalid', DATABASE_URL: 'synthetic',
    DBADMIN_SPIKE_URL: 'synthetic', PATH: '/bin', CARGO_HOME: '/temporary',
    RUSTUP_HOME: '/temporary', DOCKER_CONTEXT: 'desktop-linux',
  });
  assert.deepEqual(filtered, {PATH: '/bin', CARGO_HOME: '/temporary', RUSTUP_HOME: '/temporary', DOCKER_CONTEXT: 'desktop-linux'});
});
