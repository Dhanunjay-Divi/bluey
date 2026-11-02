# FIX-877: Short-viewport sidebar reachability

Preflight `bluey-ops`; Round644. The new sticky sidebar had a fixed viewport-height
box with no overflow handling, allowing lower navigation to fall out of reach
on short desktop windows. Bounded vertical scrolling and focused-link scroll
clearance preserve all six destinations without growing the mobile tab bar.

Only the owned stylesheet changes. Actual final-artifact Chrome1440x400 keyboard
traversal reached lower Settings inside the scrolled sidebar (top265.945,
bottom309.945, scrollTop118). All six routes passed the final responsive matrix;
IMPL644 records current build/test evidence. Source inspection alone is not used
as reachable-focus proof. No route/API or execution authority change.
