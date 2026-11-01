# FIX-845: Enforce the private VM forwarding boundary before boot

Codex preflight: `bluey-ops`; parent owns all runtime and cleanup operations.

## Issue

Phase639 compatibility attempt16562 was deliberately interrupted during startup
when private Lima hostagent logs showed a wildcard DNS-port forwarding attempt.
Generated Colima configuration already had `portForwarder: none`; that setting
alone did not establish the required lower-level forwarding boundary.

## Root Cause

Pinned Colima0.10.3 adds an ignore rule with guestIP zero and proto:any, but
omits `guestIPMustBeZero`. Pinned Lima2.2.1 defaults the omitted field totrue
when guestIP is zero, limiting that rule to the exact wildcard guest binding.
Other guest-interface binds can reach later/fallback forwarding rules.

Primary source:
[Colima rule](https://raw.githubusercontent.com/abiosoft/colima/v0.10.3/environment/vm/lima/yaml.go),
[Lima defaults](https://raw.githubusercontent.com/lima-vm/lima/v2.2.1/pkg/limayaml/defaults.go),
[first-match and override contract](https://raw.githubusercontent.com/lima-vm/lima/v2.2.1/templates/default.yaml).

## Fix Summary

Before the first CLI command, create an exact private
`$LIMA_HOME/_config/override.yaml` rule: guestIP0.0.0.0,
`guestIPMustBeZero:false`, guest ports1-65535, proto:any, ignore:true.
Override slices precede instance rules and apply on provisioning restarts too.
Lima's explicit guest22/loopback-SSH management exception remains unchanged.

Bind the0700 directory and0600 single-link file to exact device/inode, owner,
size and bytes. Revalidate before every command through stop/delete. Record
policy SHA in the closed success report. Independently reject automatic-forward
attempts in the bounded generated hostagent log and retain the same-UID TCP/UDP
delta check; a configuration label alone cannot qualify the guest.

## Files Modified

- `jobs/scripts/run-nonprod-managed-runner-vm-tests.mjs`
- `jobs/scripts/nonprod-managed-runner-vm.test.mjs`

## Verification

Parent syntax check and14 pure tests pass, including policy identity/bytes,
critical YAML mutations, hostagent-log failure/encoding/size and ordering cases.
Fresh independent review and actual boot/stop/delete qualification remain
pending for the new bytes. Attempt16562 is failed/interrupted evidence only;
its cleanup was proven and observed VM/helper PIDs were absent. No owner/global
profile, Docker context, production resource or employer application changed.

## Known Limitations

The rule does not disable VM outbound networking used for official dependency
provisioning; it forbids guest-service host-port exposure. Runtime synthetic
provider egress isolation, native x86 performance, actual image execution and
normal Jobs runner startup remain separate unproven gates.
