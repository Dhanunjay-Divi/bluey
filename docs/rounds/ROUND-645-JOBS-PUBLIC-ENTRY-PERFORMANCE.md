# Round645 — Jobs public entry first content

Base0731c172ebc625d4fcf19790c6459222f6a04c47; branch
`feat/phase-645-jobs-public-entry-performance`. Owner clarified that the slow
surface is bluey.sh/jobs itself, not the authenticated workspace. Jobs only.

## Evidence and limits

Anonymous production samples from this Mac returned HTML200 with0.142–0.188s
TTFB and no redirect; main gzip JavaScript127,293 wire bytes in0.245s and CSS
21,576 wire bytes in0.175s were cache hits. They do not reproduce the owner's
long wait, identify a deployed commit, or establish browser LCP on a slow device.
The observed HTML has an empty root. Current local main eagerly imports App,
which imports onboarding/preview/API; AuthGate imports loginUrl through API and
the taxonomy module. These are concrete dependency weaknesses, not a proven
complete explanation of the production symptom. Do not disable Cloudflare or
infer private competitor infrastructure. Keep this distinction in reporting.

## Implementation

1. Split dependency-light existing token read/storage order and login navigation
   from API; keep public API exports compatible and exact authentication
   behavior. Token presence selects UI code, never server authority.
2. Lazy-load App only for existing token or explicit preview. Preserve legacy
   route normalization, preview scenarios, query/deep links and all beta guards.
   Anonymous initialization must not import App/API/taxonomy or request account
   endpoints. Storage-read denial must still permit public content.
3. Prerender the existing AuthGate as the single public-content source during
   the existing Vite build, using installed Vite/react-dom only. No new package,
   framework, serving process or hand-maintained duplicate marketing HTML. Keep
   metadata, assets, login/signup anchors and truthful cloud caveats exact.
   Anonymous static content works without JavaScript; authenticated/preview
   startup replaces it through the existing application. Preserve a visible,
   recoverable loading/error state if the lazy application fails.
4. Fail builds on missing/ambiguous root markers or unresolved local assets;
   retain deterministic generated-output freshness checks. No private/account
   data may enter static HTML. Import one bounded fully bundled in-memory
   renderer with Node-builtin externals, not temporary renderer files or a
   POSIX-only build boundary. Existing outer isolated build cleanup stays.

## Ownership and acceptance

Existing Terra builder owns main.tsx, AuthGate.tsx, narrowly extracted auth
modules/api re-exports and their tests. Core builder owns public rendering/build
entry/script/config/package changes and prerender tests. Parent owns docs, controllers, baseline/comparison,
generated synchronization and browser QA. Independent Sol review after freeze.
No concurrent changes to owned source; preserve other agents' edits. Do not
change locks unless package metadata truly requires it; no dependency install
or shared build by the worker.

Test storage precedence/API compatibility, no-token/preview/token selection,
legacy routes and scenario preservation, deterministic prerender/no-JS hero,
plans/caveats/anchors, no account data, resolved assets, no renderer temp files and
build failure paths. Full relevant Jobs regression/types/builds/audit/privacy
plus real browser anonymous and preview paths before source checkpoint. Compare
same-environment build graphs and byte sizes; do not claim an LCP improvement
without actual matching browser measurements. Record unmeasured gaps honestly.

No merge, deployment, publication, real application/email, paid resource,
provider scope, production flag, overlay/Pinky/shared root marketing change.
This bounded performance slice is not whole-product or cloud auto-apply proof.
