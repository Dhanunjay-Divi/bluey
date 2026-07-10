# Bluey Jobs Source Provenance

No source file in this Jobs slice was copied from the reviewed job-application
repositories. The implementation is original Bluey code. The projects below
informed product or interface patterns only.

| Source | License observed during review | Use in this slice |
| --- | --- | --- |
| `11844/Auto_Jobs_Applier_AIHawk` | User states commercial rights are cleared; repository license still requires a separate release audit | Profile breadth, search exclusions, apply-once behavior, unique packet idea |
| `feder-cr/Jobs_Applier_AI_Agent_AIHawk` | AGPL | Product research only; no code copied |
| `santifer/career-ops` | MIT | Public ATS discovery-provider pattern; no code copied in this slice |
| `srbhr/Resume-Matcher` | Apache-2.0 | Resume import, diff, and export product patterns; no code copied |
| `browserbase/stagehand` | MIT | Semantic fallback boundary; not bundled by this slice |

Direct runtime dependencies are recorded in `package-lock.json` and Rust lock
files. Before a public release, generate a dependency notice from those lock
files and repeat the commercial-use review for any source code selected for a
future port.
