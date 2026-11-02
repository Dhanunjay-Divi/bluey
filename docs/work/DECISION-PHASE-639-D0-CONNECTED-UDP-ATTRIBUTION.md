# Decision639 — Connected UDP clients are not host service listeners

Date: 2026-11-01. Status: bounded arena decision; implementation/qualification
pending. Current fifth attempt remains failed; no retroactive acceptance.

## Question, scope and budget

D0 permits outbound VM provisioning but forbids new guest-service host listeners
except exact loopback SSH management. It never claims no host networking,
destination/purpose approval, guest egress isolation or production authority.
The former all-new-UDP rule conflated connected clients with service listeners.

Use two existing Sol specialists, one concise proposal/challenge each, roughly
five minutes; no VM, source write, merge or deployment by reviewers. Parent owns
the decision and actual tests. Arena review is not runtime acceptance.

## Current evidence

Attempt92676 actually bound private usernet PID, UID, fresh kernel birth,
canonical Lima executable identity, PID file and three Unix sockets. Its one
new TCP listener belonged to that helper at the observed SSH port. Eight UDP
changes comprised three connected usernet sockets, zero unbound usernet sockets
and five baseline-PID-only records, zero unknown PIDs. Baseline-PID-only does
not establish identity continuity by itself. The run correctly failed its
then-current rule, stopped all recorded processes and proved cleanup.

Journal SHA-256:
`2e7ccdfc53f23b3fc85d3f58da8423450ddec4be76c39755150e395512c773dd`.

Pinned Lima2.2.1 uses gvisor-tap-vsock0.8.9. Its UDP forwarder creates connected
host UDP sockets per guest flow and keeps connection tracking. This supports
client-relay classification, not a claim that an endpoint is DNS or an official
dependency. Sources:
[dependency](https://raw.githubusercontent.com/lima-vm/lima/v2.2.1/go.mod),
[forwarder](https://raw.githubusercontent.com/containers/gvisor-tap-vsock/v0.8.9/pkg/services/forwarder/udp.go),
[tracking](https://raw.githubusercontent.com/containers/gvisor-tap-vsock/v0.8.9/pkg/services/forwarder/udp_proxy.go).

## Alternatives and outcome

A retains all-new-UDP denial and leaves D0 unqualified. It is a valid hold if
no outbound activity is allowed, but not a practical route for the stated
listener-only test. Host-wide quiescence/firewall changes or replacing the
pinned network helper would add unrelated scope and operational risk.

B distinguishes native-attributed connected clients from listeners. Both
reviewers recommended B with a narrow, explicit claim. Parent selects B, subject
to source review, negatives and a fresh exact-source VM run:

- TCP still requires exactly one private usernet-owned loopback SSH listener.
- Every admitted UDP delta must be connected, non-unbound and have parsed valid
  numeric, non-wildcard/non-unspecified local and peer endpoints.
- Ownership is either the fully bound private usernet PID, or a non-VM/nonhelper
  PID that appeared before VM start and has identical PID, parent, UID pair,
  kernel birth, executable identity and class across both native brackets.
- New, unknown, reused, changed, unconnected, unbound, other-VM and helper-sibling
  UDP denies. No TCP exemption. No fixed count or guessed DNS-purpose rule.
- Publish bounded counts/hashes only; preserve
  `literalNoHostNetworkClaimed:false`, `literalCodeSigningClaimed:false` and
  `literalOfficialDependencyPurposeClaimed:false`.

The claim is only host-service-listener qualification under the declared
trusted, nonadversarial local host model. It is not a firewall or an application
browser egress policy. Later D1/D2/D3 and release gates remain unchanged.

## Required acceptance

Variable connected-client counts; local/peer wildcard and IPv4/IPv6 unspecified
negatives; unconnected/unbound cases; missing/new/reused/changed native identity;
other VM/helper/controller PIDs; strict TCP; overflow/duplicate ambiguity;
metadata-only reports. Default Jobs tests must include these pure Node/Python
checks without booting a VM. Fresh runtime acceptance must follow source
clearance and observed stop/PID/socket/state absence. A vote cannot replace it.
