//! Overlapped-speech region extraction (pure logic, std only).
//!
//! Used by the bank live diarizer to detect talk-over: a window cluster whose
//! speech is heavily overlapped is contaminated by another voice, so it labels
//! but does NOT update its speaker centroid (poisoning the profile is how
//! identity rots). Time is f64 seconds, absolute; intervals are half-open
//! `[start, end)`; a span ending exactly where another starts is NOT overlap.

/// Merged time regions where >= 2 of the given spans are simultaneously active.
/// Input spans are `(start, end)` in absolute secs (speaker identity irrelevant).
/// Zero-length / non-finite spans are skipped. Output regions are sorted,
/// disjoint, non-touching, each with `end > start`.
pub(crate) fn overlap_regions(spans: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut events: Vec<(f64, i32)> = Vec::with_capacity(spans.len() * 2);
    for &(s, e) in spans {
        if e > s && s.is_finite() && e.is_finite() {
            events.push((s, 1));
            events.push((e, -1));
        }
    }
    if events.is_empty() {
        return Vec::new();
    }
    events.sort_by(|a, b| a.0.partial_cmp(&b.0).expect("finite times"));

    let mut regions: Vec<(f64, f64)> = Vec::new();
    let mut active: i32 = 0;
    let mut region_start: Option<f64> = None;
    let mut i = 0;
    while i < events.len() {
        let t = events[i].0;
        // Apply ALL deltas at this instant before inspecting the count, so an
        // exact-touch end/start cancels (never overlap) and two overlapped
        // regions meeting at t stay one region (touching regions merge).
        while i < events.len() && events[i].0 == t {
            active += events[i].1;
            i += 1;
        }
        match (region_start, active >= 2) {
            (None, true) => region_start = Some(t),
            (Some(start), false) => {
                regions.push((start, t));
                region_start = None;
            }
            _ => {}
        }
    }
    debug_assert!(
        region_start.is_none(),
        "every span start has a matching end"
    );
    regions
}

/// Seconds of `[s, e)` covered by the regions (expected sorted + disjoint).
pub(crate) fn overlapped_secs(regions: &[(f64, f64)], s: f64, e: f64) -> f64 {
    // Empty/degenerate/NaN span → no coverage. The negation is deliberate: a
    // NaN endpoint must return 0.0, and NaN fails this strict `>` (so `!` is
    // true), whereas `e <= s` would be false for NaN and fall through.
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    if !(e > s) {
        return 0.0;
    }
    let mut total = 0.0;
    for &(rs, re) in regions {
        let lo = if rs > s { rs } else { s };
        let hi = if re < e { re } else { e };
        if hi > lo {
            total += hi - lo;
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f64 = 1e-9;

    #[test]
    fn disjoint_spans_no_regions() {
        assert!(overlap_regions(&[(0.0, 1.0), (2.0, 3.0)]).is_empty());
        assert!(overlap_regions(&[]).is_empty());
        assert!(overlap_regions(&[(1.0, 4.0)]).is_empty());
        assert!(overlap_regions(&[(0.0, 4.0), (2.0, 2.0)]).is_empty());
    }

    #[test]
    fn two_spans_overlap_is_intersection() {
        assert_eq!(overlap_regions(&[(0.0, 5.0), (3.0, 8.0)]), vec![(3.0, 5.0)]);
    }

    #[test]
    fn three_stacked_union_of_pairwise() {
        assert_eq!(
            overlap_regions(&[(0.0, 10.0), (2.0, 6.0), (4.0, 8.0)]),
            vec![(2.0, 8.0)]
        );
    }

    #[test]
    fn exact_touch_is_not_overlap() {
        assert!(overlap_regions(&[(0.0, 5.0), (5.0, 10.0)]).is_empty());
        assert_eq!(
            overlap_regions(&[(0.0, 5.0), (0.0, 5.0), (5.0, 10.0), (5.0, 10.0)]),
            vec![(0.0, 10.0)]
        );
    }

    #[test]
    fn half_covered_span_secs() {
        let r = [(0.0, 5.0)];
        assert!((overlapped_secs(&r, 0.0, 10.0) - 5.0).abs() < EPS);
        assert!((overlapped_secs(&r, 2.5, 7.5) - 2.5).abs() < EPS);
        assert!((overlapped_secs(&r, 3.0, 3.0)).abs() < EPS);
    }
}
