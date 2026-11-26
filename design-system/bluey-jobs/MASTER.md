# Bluey Jobs — product interface direction

Current repository/user scope outranks generated design suggestions. Reference:
UI/UX Pro Max verified candidate-career-portal search (minimal/Swiss direction),
React semantic navigation guidance and the current Bluey brand assets.

## Product and visual language

An everyday job-search workspace for people, not an infrastructure console.
Professional, calm and legible; show what needs attention and what happened.
Retain Bluey's cyan/blue identity with neutral surfaces and restrained borders.
Use existing Inter/system sans fonts; do not add font CDNs or new UI dependencies.
Use Lucide/SVG consistently; decorative icons are hidden from accessibility APIs.
Avoid dense all-caps technical prose, giant empty status cards, decorative charts,
heavy gradients, excessive animation and unsupported marketing claims.

## Layout

Desktop: labelled persistent sidebar, compact utility header, generous but bounded
reading width and a clear primary page action. Mobile: existing five labelled
destinations, reserved bottom-nav space, account/settings still reachable.
Prioritize user work above operational detail; disclose deeper source/runner
diagnostics without removing them. Preserve deep links and navigation state.

## Tokens and interaction

Use semantic surface/text/border/accent/status/on-accent tokens for both themes.
Separate accent text and filled-button contrast where needed; darkening a link
color must not leave a dark button label on a newly dark fill. Verify normal
text4.5:1, meaningful UI boundaries3:1, visible keyboard focus and non-color labels.
Keep essential copy readable; mobile form/body text16px where appropriate.
Use a4/8px spacing rhythm, comfortable targets, no layout-jumping press states.
Motion is brief and purposeful; honor reduced motion. No animation blocks input.

## Functional truth

Every action uses existing product behavior. Counts retain loaded/global scope,
setup readiness does not imply eligibility, submitted state is not invented
employer receipt, and cloud availability follows server evidence. Empty, loading,
failed, offline, expired-authority and permission-required states are first-class.
Resume changes are reviewable; immutable facts and authority guards remain intact.
No mock success, exaggerated source totals, fabricated testimonials or silently
enabled effects. No capture of resume/email content in analytics or screenshots.

## Delivery

Ship reviewed feature slices, not an all-at-once rewrite. Source tests plus real
browser route/viewport/theme/keyboard checks are required. App entry/onboarding,
matches, resume/application review, tracker, messaging/automation and Jobs-specific
public entry are successive cohesive parts of the same product direction.
