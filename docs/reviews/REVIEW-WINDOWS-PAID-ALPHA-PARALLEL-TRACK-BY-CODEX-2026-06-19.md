# Review: Windows Paid Alpha Parallel Track — 2026-06-19

## Verdict

🟡 ACCEPT WITH WINDOWS LIVE GATES

Windows can run in parallel, but it is not ready for paid users until the new
P0 readiness gate passes on a clean Windows 10/11 machine.

## Findings

- The native UI direction is correct. Win32 plus Direct2D/DirectWrite is the
  best fit for a sharp, low-latency, capture-excluded overlay.
- The existing Windows helper code is a useful foundation, not a finished
  customer path.
- The biggest current risks are packaging/install/update, clean-machine smoke,
  capture-exclusion proof, and real paid managed STT/Answer proof.
- No separate Windows backend is needed. Windows should use the same Bluey
  cloud, account, billing, provider, and support systems as macOS.

## Residual Risk

- Without a Windows smoke machine, this review cannot prove helper behavior,
  DPI, click-through, capture exclusion, or audio reliability.
- Unsigned Windows artifacts may trigger SmartScreen/Defender warnings. That is
  acceptable only for a tightly controlled invite alpha with clear instructions.

## Recommendation

Keep Windows marked as coming soon publicly. Start the Windows P0 track in
parallel. Do not invite paid Windows users until the readiness doc passes.
