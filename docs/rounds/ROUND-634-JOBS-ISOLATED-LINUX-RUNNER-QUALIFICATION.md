# Round 634 — Jobs isolated Linux runner qualification

Date: 2026-10-27. Preflight: `bluey-ops`, current local handoff and
`performance-analysis` reproducible-baseline discipline. Base: `57635b29`.
Status: source/pure gates and static review passed; actual Linux/container run pending.

## Acceptance and boundaries

1. Use the canonical isolated launcher and shared `mac-build` lock. Keep the
   overall Jobs goal active, not marked complete by this batch.
2. Provision a new Jobs-only local VZ/Colima profile with separate
   HOME/COLIMA_HOME/LIMA_HOME/DOCKER_CONFIG/cache, no inherited
   owner credentials, no host mounts, no SSH-agent/config forwarding, no active
   Docker-context mutation. Do not start or alter existing Pinky/HQ profiles.
3. Bound VM resources to 4 CPUs, 6 GiB RAM, 8 GiB root plus16 GiB data disks;
   require at least32 GiB host free before startup. No paid/provider resources.
4. Build the unchanged current managed-runner Dockerfile, native addon and JS
   sources with exact source-commit metadata and CI-only zero config/migration/
   protocol hashes. This is local construction/smoke proof, never signed
   production artifact, release qualification or activation authority.
   The installed Docker29.8.1 client has no Buildx plugin. Use only an explicit
   `DOCKER_BUILDKIT=0` construction mode, reported as `legacy-explicit`, not silent
   fallback or CI/BuildKit byte equivalence. Do not install global tools or new
   third-party binaries. If the engine rejects this documented mode, fail closed.
5. Run the existing native-addon smoke in that exact image, no network, read-only
   rootfs, no capabilities/privilege escalation and only bounded private tmpfs.
   Preserve its unchanged probe/write/read/inventory/delete semantics.
6. Observe actual platform, image identity, outcomes and resource deltas. No
   low-latency, capacity, browser/employer, live API/PG lease or receipt claim.
7. Install stop/delete cleanup before startup; delete only the newly created
   private profile on success/failure/signals. Keep VM state in a separately
   generated, short, owned private temporary root outside the canonical test
   root: the canonical launcher must never unlink a detached VM's open disks
   after ambiguous shutdown. Recheck state identity and empty private Colima/
   Lima inventories before removing that exact state root; retain it on any
   uncertainty. Verify no owned VM or exact-root residue. Fail closed on cleanup
   ambiguity, capacity or unsupported platform.
8. Independently review procedure and evidence before marking this bounded batch
   complete. No production/desktop/overlay/Pinky/push/deploy/publication change.

No extra benchmark framework or unreviewed CLI is installed. Existing CLI help
and resource observations determine executable options. If Linux prerequisites
fail, retain that failure honestly; never use the stopped unrelated profiles as
an implicit fallback or weaken Docker/runtime measurement gates.

CLI isolation references: [Colima profiles](https://colima.run/docs/profiles/)
and [official FAQ environment variables](https://github.com/abiosoft/colima/blob/main/docs/FAQ.md).
Installed CLI help was checked before choosing the actual flags.
Builder-mode reference: [Docker legacy-builder deprecation and explicit opt-out](https://docs.docker.com/engine/deprecated/#legacy-builder-for-linux-images).
