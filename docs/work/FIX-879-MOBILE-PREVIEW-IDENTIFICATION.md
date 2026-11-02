# FIX-879: Mobile sample-data identification

Preflight `bluey-ops`; Round644. Actual375/768 browser checks found that existing
mobile header CSS hid the Preview label while showing synthetic account and
application data. This could obscure the distinction between preview and a live
account. Remove only that label from the mobile-hidden selector; retain compact
utility controls and all product behavior. Add a direct markup/selector guard.

Final gate71049 passed2520 unique cases plus40 repeated focused shell/Overview
cases, five workspace typechecks/builds and deterministic bundle export. Actual
Chrome48-case matrix on this final artifact asserted375/768/1024/1440 widths
across six routes and both themes: Preview visible throughout, no horizontal
overflow. The final IAB matrix is excluded because its requested viewport sizes
did not apply. Source/evidence hashes and remaining QA limits are in IMPL644.
