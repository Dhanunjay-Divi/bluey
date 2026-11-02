# FIX-876: Account disclosure semantics

Preflight `bluey-ops`; Round644. The account popup declared ARIA menu/menuitem
roles without the corresponding focus-first and Arrow/Home/End keyboard pattern.
The narrow fix uses ordinary disclosure content and normal Tab navigation,
retaining expanded state, existing links/actions, Escape and focus return.

AppShell and its direct static markup regression changed. No account API,
authentication or sign-out semantics changed. Actual disclosure open, Tab to Jobs
settings, Escape close and trigger-focus return passed in Chrome on the final
artifact. The external account/sign-out actions were not executed during fixture
QA. Full current-source gate2520 passed; see IMPL644 for evidence and limits.
