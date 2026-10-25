// Preserve build/Docker configuration, but never inherit a user's database connection settings.
export function fixtureEnvironment(source) {
  return Object.fromEntries(Object.entries(source).filter(([name]) =>
    !/^(PG|MYSQL|MARIADB|DATABASE_URL$|DBADMIN_(SPIKE|LIFECYCLE|FIXTURE)_)/.test(name)));
}
