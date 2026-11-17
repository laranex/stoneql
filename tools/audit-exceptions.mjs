// Advisory exceptions for the Linux release graph only (`npm run audit:release:linux`).
// Approved by the user on 2026-11-17 as a documented exception; re-check at every Tauri upgrade.
// The macOS release audit has no exceptions. Each entry must still be reported by cargo audit:
// an exception whose advisory no longer appears fails the audit so it is removed, not kept.
export const linuxExceptions = [
  {
    id: 'RUSTSEC-2024-0429',
    kind: 'unsound',
    package: 'glib',
    version: '0.18.5',
    path: 'stoneql-desktop → tauri 2.12.5 → muda 0.20.0 / tao 0.37.2 / wry 0.57.1 → gtk 0.18.2 → glib 0.18.5',
    reason: 'Unsound Iterator impls of glib::VariantStrIter. No use of VariantStrIter in tauri, wry, tao, muda, gtk, webkit2gtk, gio, soup3, atk, gdk or StoneQL; only glib defines it. Tauri 2 on Linux is tied to gtk-rs 0.18.',
  },
  {
    id: 'RUSTSEC-2024-0370',
    kind: 'unmaintained',
    package: 'proc-macro-error',
    version: '1.0.4',
    path: 'stoneql-desktop → tauri 2.12.5 → gtk 0.18.2 → glib 0.18.5 → glib-macros 0.18.5 (proc-macro) → proc-macro-error 1.0.4',
    reason: 'Unmaintained compile-time helper of the glib-macros proc macro; absent from the runtime graph (cargo tree -e normal,no-proc-macro), so not in the shipped binary.',
  },
];

/**
 * Problems in a `cargo audit --json` report given the allowed exceptions: any vulnerability or
 * warning not matched exactly (ID, kind, package and version), and any exception not reported.
 */
export function auditProblems(report, exceptions) {
  if (!report?.vulnerabilities || !report.warnings) return ['cargo audit report is missing or malformed'];
  const findings = [
    ...report.vulnerabilities.list.map((entry) => ({ ...entry, kind: 'vulnerability' })),
    ...Object.values(report.warnings).flat(),
  ].map((entry) => ({ id: entry.advisory?.id ?? null, kind: entry.kind, package: entry.package?.name, version: entry.package?.version }));
  const allowed = (finding) => exceptions.some((exception) => exception.id === finding.id && exception.kind === finding.kind && exception.package === finding.package && exception.version === finding.version);
  const problems = findings.filter((finding) => !allowed(finding))
    .map((finding) => `not allowed: ${finding.id ?? '(no advisory)'} ${finding.kind} ${finding.package} ${finding.version}`);
  for (const exception of exceptions) {
    if (!findings.some((finding) => finding.id === exception.id && finding.package === exception.package && finding.version === exception.version)) {
      problems.push(`stale exception: ${exception.id} (${exception.package} ${exception.version}) is no longer reported; remove it`);
    }
  }
  return problems;
}
