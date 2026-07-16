//! Deterministic, download-free benchmark for the transcript agreement layer.
//!
//! Run with:
//! `cargo run -p cue-core --release --example stt-agreement-bench -- --iterations 5000`

use cue_core::stt::agreement::{
    AgreementOutcome, LocalAgreementConfig, LocalAgreementTracker, TranscriptAgreementUpdate,
};
use serde::{Deserialize, Serialize};
use std::hint::black_box;
use std::time::Instant;

const FIXTURES_JSON: &str = include_str!("../tests/fixtures/stt_agreement.json");
const DEFAULT_ITERATIONS: usize = 5_000;
const MAX_ITERATIONS: usize = 20_000;

#[derive(Debug, Clone, Deserialize)]
struct Fixture {
    name: String,
    reference: String,
    updates: Vec<FixtureUpdate>,
}

#[derive(Debug, Clone, Deserialize)]
struct FixtureUpdate {
    at_ms: u64,
    kind: FixtureUpdateKind,
    text: String,
    #[serde(default)]
    silence_ms: u64,
    endpoint_expected: Option<bool>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum FixtureUpdateKind {
    Partial,
    Final,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct Percentiles {
    count: usize,
    p50: u64,
    p95: u64,
    p99: u64,
    max: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
struct WerReport {
    errors: usize,
    reference_words: usize,
    rate: f64,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
struct EndpointReport {
    true_positive: usize,
    true_negative: usize,
    false_positive: usize,
    false_negative: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
struct SemanticReport {
    fixtures: usize,
    fixture_names: Vec<String>,
    updates: usize,
    wer: WerReport,
    stable_prefix_delay_ms: Percentiles,
    endpoint: EndpointReport,
    uncommitted_reference_words: usize,
}

#[derive(Debug, Clone, Copy)]
struct ResourceSnapshot {
    cpu_time_us: Option<u64>,
    peak_rss_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
struct ResourceReport {
    cpu_time_us: Option<u64>,
    peak_rss_before_bytes: Option<u64>,
    peak_rss_after_bytes: Option<u64>,
    peak_rss_growth_bytes: Option<u64>,
}

#[derive(Debug, Serialize)]
struct BenchmarkReport {
    benchmark: &'static str,
    iterations: usize,
    tracker_config: TrackerConfigReport,
    semantic: SemanticReport,
    processing_latency_ns: Percentiles,
    resources: ResourceReport,
    limits: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
struct TrackerConfigReport {
    max_words: usize,
    max_text_bytes: usize,
    timestamp_tolerance_ms: u64,
    fast_endpoint_silence_ms: u64,
    slow_endpoint_silence_ms: u64,
    fast_endpoint_stable_ticks: u32,
}

fn main() {
    let iterations = parse_iterations(std::env::args().skip(1));
    let fixtures = load_fixtures();
    let config = LocalAgreementConfig::default();
    let semantic = evaluate_semantics(&fixtures, config);

    let before = resource_snapshot();
    let processing_latency_ns = measure_processing_latency(&fixtures, config, iterations);
    let after = resource_snapshot();
    let resources = ResourceReport {
        cpu_time_us: option_delta(after.cpu_time_us, before.cpu_time_us),
        peak_rss_before_bytes: before.peak_rss_bytes,
        peak_rss_after_bytes: after.peak_rss_bytes,
        peak_rss_growth_bytes: option_delta(after.peak_rss_bytes, before.peak_rss_bytes),
    };

    let report = BenchmarkReport {
        benchmark: "bluey_local_agreement_fixed_v1",
        iterations,
        tracker_config: TrackerConfigReport {
            max_words: config.max_words,
            max_text_bytes: config.max_text_bytes,
            timestamp_tolerance_ms: config.timestamp_tolerance_ms,
            fast_endpoint_silence_ms: config.fast_endpoint_silence_ms,
            slow_endpoint_silence_ms: config.slow_endpoint_silence_ms,
            fast_endpoint_stable_ticks: config.fast_endpoint_stable_ticks,
        },
        semantic,
        processing_latency_ns,
        resources,
        limits: vec![
            "WER is computed from fixed text hypotheses; it measures agreement/evaluation plumbing, not acoustic-model quality.",
            "Stable-prefix delay and endpoint FP/FN use labeled fixture timestamps and silence values, not microphone, VAD, or end-to-end decoder timing.",
            "The current local helper emits text without word timestamps, so production agreement is exact-token based; timestamp tolerance is covered by tracker tests.",
            "Processing latency and CPU are host/build dependent; run the release command for comparable measurements.",
            "RSS is process-wide peak RSS from getrusage on Unix and is unavailable on unsupported platforms; peak growth may be zero after an earlier process peak.",
        ],
    };

    println!(
        "{}",
        serde_json::to_string_pretty(&report).expect("serialize benchmark report")
    );
}

fn parse_iterations(args: impl Iterator<Item = String>) -> usize {
    let mut args = args.peekable();
    let mut iterations = DEFAULT_ITERATIONS;
    while let Some(argument) = args.next() {
        if argument == "--iterations" {
            if let Some(value) = args.next().and_then(|value| value.parse::<usize>().ok()) {
                iterations = value.clamp(1, MAX_ITERATIONS);
            }
        }
    }
    iterations
}

fn load_fixtures() -> Vec<Fixture> {
    serde_json::from_str(FIXTURES_JSON).expect("valid embedded STT agreement fixtures")
}

fn evaluate_semantics(fixtures: &[Fixture], config: LocalAgreementConfig) -> SemanticReport {
    let mut wer_errors = 0usize;
    let mut reference_words = 0usize;
    let mut stable_delays = Vec::new();
    let mut endpoint = EndpointReport::default();
    let mut updates = 0usize;
    let mut uncommitted_reference_words = 0usize;

    for (fixture_index, fixture) in fixtures.iter().enumerate() {
        let reference = eval_words(&fixture.reference);
        reference_words += reference.len();
        let mut first_seen_ms = vec![None; reference.len()];
        let mut committed_ms = vec![None; reference.len()];
        let mut tracker = LocalAgreementTracker::with_generation(config, fixture_index as u64 + 1);
        let mut final_text = None;

        for input in &fixture.updates {
            updates += 1;
            let hypothesis = eval_words(&input.text);
            let visible_reference_prefix = common_prefix_len(&reference, &hypothesis);
            for slot in first_seen_ms.iter_mut().take(visible_reference_prefix) {
                slot.get_or_insert(input.at_ms);
            }

            let cursor = tracker.cursor();
            let agreement = match input.kind {
                FixtureUpdateKind::Partial => {
                    applied(tracker.observe_partial_text(cursor, &input.text))
                }
                FixtureUpdateKind::Final => {
                    final_text = Some(input.text.as_str());
                    applied(tracker.observe_final_text(cursor, &input.text))
                }
            };
            record_committed_prefix(&reference, &agreement, input.at_ms, &mut committed_ms);

            if let (FixtureUpdateKind::Partial, Some(expected)) =
                (input.kind, input.endpoint_expected)
            {
                let predicted = tracker.endpoint_decision(input.silence_ms).should_finalize;
                match (predicted, expected) {
                    (true, true) => endpoint.true_positive += 1,
                    (false, false) => endpoint.true_negative += 1,
                    (true, false) => endpoint.false_positive += 1,
                    (false, true) => endpoint.false_negative += 1,
                }
            }
        }

        let final_text = final_text.unwrap_or_default();
        wer_errors += word_error_count(&reference, &eval_words(final_text));
        for (first_seen, committed) in first_seen_ms.into_iter().zip(committed_ms) {
            match (first_seen, committed) {
                (Some(first_seen), Some(committed)) => {
                    stable_delays.push(committed.saturating_sub(first_seen));
                }
                (_, None) => uncommitted_reference_words += 1,
                (None, Some(_)) => {}
            }
        }
    }

    SemanticReport {
        fixtures: fixtures.len(),
        fixture_names: fixtures
            .iter()
            .map(|fixture| fixture.name.clone())
            .collect(),
        updates,
        wer: WerReport {
            errors: wer_errors,
            reference_words,
            rate: if reference_words == 0 {
                0.0
            } else {
                wer_errors as f64 / reference_words as f64
            },
        },
        stable_prefix_delay_ms: percentiles(stable_delays),
        endpoint,
        uncommitted_reference_words,
    }
}

fn record_committed_prefix(
    reference: &[String],
    agreement: &TranscriptAgreementUpdate,
    at_ms: u64,
    committed_ms: &mut [Option<u64>],
) {
    let committed = eval_words(&agreement.committed_text);
    let committed_reference_prefix = common_prefix_len(reference, &committed);
    for slot in committed_ms.iter_mut().take(committed_reference_prefix) {
        slot.get_or_insert(at_ms);
    }
}

fn measure_processing_latency(
    fixtures: &[Fixture],
    config: LocalAgreementConfig,
    iterations: usize,
) -> Percentiles {
    let samples_per_iteration = fixtures.iter().map(|fixture| fixture.updates.len()).sum();
    let mut samples = Vec::with_capacity(iterations.saturating_mul(samples_per_iteration));
    for iteration in 0..iterations {
        for (fixture_index, fixture) in fixtures.iter().enumerate() {
            let generation = iteration
                .saturating_mul(fixtures.len())
                .saturating_add(fixture_index)
                .saturating_add(1) as u64;
            let mut tracker = LocalAgreementTracker::with_generation(config, generation);
            for input in &fixture.updates {
                let cursor = tracker.cursor();
                let started = Instant::now();
                let outcome = match input.kind {
                    FixtureUpdateKind::Partial => tracker.observe_partial_text(cursor, &input.text),
                    FixtureUpdateKind::Final => tracker.observe_final_text(cursor, &input.text),
                };
                samples.push(started.elapsed().as_nanos().min(u64::MAX as u128) as u64);
                black_box(outcome);
            }
        }
    }
    percentiles(samples)
}

fn applied(outcome: AgreementOutcome) -> TranscriptAgreementUpdate {
    match outcome {
        AgreementOutcome::Applied(update) => update,
        AgreementOutcome::RejectedStale { active, received } => {
            panic!("fixed fixture produced stale cursor: active={active:?} received={received:?}")
        }
    }
}

fn eval_words(text: &str) -> Vec<String> {
    text.split_whitespace()
        .filter_map(|token| {
            let token = token
                .trim_matches(|character: char| !character.is_alphanumeric())
                .to_lowercase();
            (!token.is_empty()).then_some(token)
        })
        .collect()
}

fn common_prefix_len(left: &[String], right: &[String]) -> usize {
    left.iter()
        .zip(right)
        .take_while(|(left, right)| left == right)
        .count()
}

fn word_error_count(reference: &[String], hypothesis: &[String]) -> usize {
    let mut previous: Vec<usize> = (0..=hypothesis.len()).collect();
    let mut current = vec![0usize; hypothesis.len() + 1];
    for (reference_index, reference_word) in reference.iter().enumerate() {
        current[0] = reference_index + 1;
        for (hypothesis_index, hypothesis_word) in hypothesis.iter().enumerate() {
            let substitution =
                previous[hypothesis_index] + usize::from(reference_word != hypothesis_word);
            let deletion = previous[hypothesis_index + 1] + 1;
            let insertion = current[hypothesis_index] + 1;
            current[hypothesis_index + 1] = substitution.min(deletion).min(insertion);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[hypothesis.len()]
}

fn percentiles(mut values: Vec<u64>) -> Percentiles {
    if values.is_empty() {
        return Percentiles {
            count: 0,
            p50: 0,
            p95: 0,
            p99: 0,
            max: 0,
        };
    }
    values.sort_unstable();
    Percentiles {
        count: values.len(),
        p50: nearest_rank(&values, 50),
        p95: nearest_rank(&values, 95),
        p99: nearest_rank(&values, 99),
        max: *values.last().expect("non-empty percentile input"),
    }
}

fn nearest_rank(values: &[u64], percentile: usize) -> u64 {
    let rank = percentile.saturating_mul(values.len()).saturating_add(99) / 100;
    values[rank.saturating_sub(1).min(values.len() - 1)]
}

fn option_delta(after: Option<u64>, before: Option<u64>) -> Option<u64> {
    Some(after?.saturating_sub(before?))
}

#[cfg(unix)]
fn resource_snapshot() -> ResourceSnapshot {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::zeroed();
    let result = unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) };
    if result != 0 {
        return ResourceSnapshot {
            cpu_time_us: None,
            peak_rss_bytes: None,
        };
    }
    let usage = unsafe { usage.assume_init() };
    let user_us = timeval_us(usage.ru_utime);
    let system_us = timeval_us(usage.ru_stime);
    #[cfg(target_os = "macos")]
    let peak_rss_bytes = (usage.ru_maxrss >= 0).then_some(usage.ru_maxrss as u64);
    #[cfg(not(target_os = "macos"))]
    let peak_rss_bytes =
        (usage.ru_maxrss >= 0).then_some((usage.ru_maxrss as u64).saturating_mul(1_024));
    ResourceSnapshot {
        cpu_time_us: Some(user_us.saturating_add(system_us)),
        peak_rss_bytes,
    }
}

#[cfg(unix)]
fn timeval_us(value: libc::timeval) -> u64 {
    (value.tv_sec.max(0) as u64)
        .saturating_mul(1_000_000)
        .saturating_add(value.tv_usec.max(0) as u64)
}

#[cfg(not(unix))]
fn resource_snapshot() -> ResourceSnapshot {
    ResourceSnapshot {
        cpu_time_us: None,
        peak_rss_bytes: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_fixture_semantics_are_deterministic() {
        let report = evaluate_semantics(&load_fixtures(), LocalAgreementConfig::default());
        assert_eq!(report.fixtures, 3);
        assert_eq!(report.updates, 22);
        assert_eq!(report.wer.errors, 1);
        assert_eq!(report.wer.reference_words, 19);
        assert!((report.wer.rate - (1.0 / 19.0)).abs() < f64::EPSILON);
        assert_eq!(report.stable_prefix_delay_ms.count, 18);
        assert_eq!(report.stable_prefix_delay_ms.p50, 300);
        assert_eq!(report.stable_prefix_delay_ms.p95, 300);
        assert_eq!(report.stable_prefix_delay_ms.p99, 300);
        assert_eq!(report.endpoint.false_positive, 0);
        assert_eq!(report.endpoint.false_negative, 0);
        assert_eq!(report.endpoint.true_positive, 3);
        assert_eq!(report.uncommitted_reference_words, 1);
    }

    #[test]
    fn word_error_rate_counts_substitution_insertion_and_deletion() {
        assert_eq!(
            word_error_count(&eval_words("one two three"), &eval_words("one four three")),
            1
        );
        assert_eq!(
            word_error_count(&eval_words("one two"), &eval_words("one two three")),
            1
        );
        assert_eq!(
            word_error_count(&eval_words("one two three"), &eval_words("one three")),
            1
        );
    }

    #[test]
    fn iteration_argument_is_bounded() {
        assert_eq!(
            parse_iterations(["--iterations".to_string(), "0".to_string()].into_iter()),
            1
        );
        assert_eq!(
            parse_iterations(["--iterations".to_string(), "999999".to_string()].into_iter()),
            MAX_ITERATIONS
        );
    }
}
