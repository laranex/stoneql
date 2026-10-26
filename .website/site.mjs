export function siteBase(value = process.env.DOCS_BASE || '/') {
  if (!/^\/(?:[A-Za-z0-9._-]+\/)*$/.test(value) || value.split('/').some(part => part === '.' || part === '..')) throw new Error('DOCS_BASE must be a slash-delimited absolute path');
  return value;
}
export function repositoryUrl() {
  const value = process.env.DOCS_REPOSITORY_URL || (process.env.GITHUB_REPOSITORY ? `https://github.com/${process.env.GITHUB_REPOSITORY}` : '');
  if (value && !/^https:\/\/github\.com\/[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(value)) throw new Error('Expected a public GitHub repository URL');
  return value;
}
export const sidebar = [
  { text: 'Getting Started', items: [{ text: 'Setup', link: '/getting-started/' }] },
  { text: 'User Guide', items: [
    { text: 'Workspace', link: '/user-guide/' },
    { text: 'Connections', link: '/user-guide/connections' },
    { text: 'SQL Editor', link: '/user-guide/sql-editor' },
    { text: 'Import / Export', link: '/user-guide/import-export' },
    { text: 'Database Objects', link: '/database-objects' }
  ] },
  { text: 'Architecture — Proposed', items: ['overview', 'database-adapters', 'connection-management', 'api-contract', 'data-integrity', 'import-export', 'security', 'deployment', 'roadmap'].map(name => ({ text: name.split('-').map(word => word[0].toUpperCase() + word.slice(1)).join(' '), link: `/architecture/${name}` })) },
  { text: 'Development', items: [
    { text: 'Contributing', link: '/development/' },
    { text: 'Verification', link: '/development/verification' },
    { text: 'Rust Foundation', link: '/development/rust-foundation' },
    { text: 'Documentation', link: '/development/documentation' },
    { text: 'Branding', link: '/development/branding' },
    { text: 'Read-only SQL Safety', link: '/development/read-only-safety' },
    { text: 'PostgreSQL & SQLite', link: '/development/postgres-sqlite' },
    { text: 'SSH & Credentials', link: '/development/ssh-credentials' },
    { text: 'Resource Profiles', link: '/development/resource-profiles' },
    { text: 'UX Review', link: '/ux-review-remediation' }
  ] }
];
