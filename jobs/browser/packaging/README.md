# Native Jobs Browser packaging — quarantined

This private package inventories the optional native release SDK independently
from the Web/cloud Jobs workspace. It is deliberately **not** a Jobs workspace.
The Web/cloud dependency audit does not qualify this separate dependency graph.

`node ../scripts/verify-packaging-toolchain.mjs` runs a metadata-only audit in a
temporary directory. It copies only this manifest and lock, disables lifecycle
scripts, includes development dependencies, and audits at the moderate threshold
against the public npm registry. The command does not install or execute the SDK.
It requires npm paired with the executing Node distribution; an ambient PATH npm
or parent workspace override is not accepted.

The current pinned SDK still contains legacy dependencies. Its metadata inventory
is not a clean security audit. Do not use a blind minimatch major override to hide
that finding: the legacy packages use incompatible callable and callback APIs.

Even a clean metadata audit cannot authorize packaging. The normal packaging
entry point refuses before source staging, subprocesses, signing, or cleanup.
The release workflow also refuses explicitly after the separate audit, so no
candidate, authorization, or promotion job can succeed through this quarantine.
There is no CLI or environment override to enable native publication.

Removing this quarantine is a separate reviewed migration. It requires a clean
independent audit, an exact installed-SDK binding, exercised native packaging,
platform install/protocol/upgrade/rollback checks, and the existing signing and
release approvals. Web/cloud Jobs work does not supply any of that authority.
