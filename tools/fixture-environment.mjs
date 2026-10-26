// Preserve build/Docker configuration, but never inherit a user's database connection settings
// or SSH agent/askpass: SSH fixtures start their own private agent.
export function fixtureEnvironment(source) {
  return Object.fromEntries(Object.entries(source).filter(([name]) =>
    !/^(PG|MYSQL|MARIADB|SSH_|DATABASE_URL$|(?:STONEQL|DBADMIN)_(SPIKE|LIFECYCLE|FIXTURE|SSH)_)/.test(name)));
}

// Match Docker CLI precedence: an explicit context overrides DOCKER_HOST.
export function assertLocalFixtureDocker(environment, inspectContext) {
  const endpoint = environment.DOCKER_CONTEXT
    ? inspectContext(environment.DOCKER_CONTEXT)
    : environment.DOCKER_HOST || inspectContext();
  if (!endpoint.startsWith('unix://') && !endpoint.startsWith('npipe://')) {
    throw new Error('Fixture tests require a local Docker daemon; remote contexts are refused');
  }
}
