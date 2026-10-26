# M003 resource characterization

Executed2026-10-26, macOS ARM / Apple M5 /10 logical CPUs /24GiB RAM, Node26.11.1, Rust1.99.1, debug runtime and disposable Docker. Engine/version, full hardware/OS context and all100 latency samples are in the safe JSON reports. No production credentials or user SQL/data retained.

| Engine | p95 runtime+JSON ms | Connected idle RSS MiB | Process max RSS MiB |
| --- | ---: | ---: | ---: |
| mysql | 14.583 | 8.000 | 23.375 |
| mariadb | 14.252 | 7.719 | 22.875 |

Both fixtures verified100,000 rows of1,024-byte payloads, PRIMARY range planning and100200-row bounded SELECTs. Metadata widths16/128/512/2048 serialized to892/6744/27096/109552bytes;128-byte rejection and fresh-query recovery passed. Eight barrier-started queries retained8,386,392serialized bytes total, each response individually within1MiB and explicitly truncated at a row boundary. Observer and Docker reports verify owned session/resource cleanup and private-directory removal.

Timing starts at the shared runtime call and ends after result serialization. It excludes native IPC/UI and server/parent-process memory. No warmups removed. Mac peak RSS is bytes; Linux KiB normalization is unit-tested but Linux execution has not occurred. Baseline is an independent ps snapshot; comparing it with a process high-water mark is characterization, not a portable allocator guarantee. The child has a180-second workload deadline below its240-second outer monitor timeout.

The2MiB approved packet bound is not an aggregate descriptor bound. Source review of prepare_statement/recv_result_metadata confirms descriptors accumulate before the application's serialized-output check. No driver patch/provenance changed; adversarial aggregate budgets require a new concrete evaluated mechanism before adoption. Numerical release budgets are still unapproved. Full native GUI/OS/resource acceptance and hostile UDF/external effects remain open; M003 is not complete.

Verification:157 JS tests (138frontend+19repository),33 default Rust tests, workspace build, rustfmt, all-target Clippy, contracts/boundaries/vendor, frontend production and docs production/source/navigation/output passed. Two new parser tests cover units, absent/duplicate/overflow values and unsupported platforms. Shared synthetic cleanup/readiness harnesses now copy the imported resource-report helper; all original failure regressions passed. No Vue or production runtime code changed, and no new browser/native smoke run is claimed. Prior22 browser scenarios remain prior evidence. Existing chunk/linker warnings and Linux GTK findings are not waived. No commit/push/deployment implied by these measurements.
