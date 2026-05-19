//! Bluey Auto Router — task classifier + routing policy that sits above
//! `cue_llm::LlmProvider` / `cue_llm::LlmRouter`.
//!
//! The product premise: Bluey Auto should give the user the **fastest useful
//! answer first** (an `Instant`-lane response that streams immediately), and
//! optionally a **smarter answer when needed** (a `Deep`-lane response that
//! refines or replaces the draft when complete). The classifier decides which
//! lane the question belongs in, and a routing policy translates that into a
//! concrete provider configuration.
//!
//! Architecture:
//!
//! ```text
//! User input ───►  TaskClassifier  ───► TaskClassification
//!                       │
//!                       ▼
//!                 RoutingPolicy
//!                       │
//!                       ▼
//!                 ProviderRoute (instant | balanced | deep | vision | local)
//!                       │
//!                       ▼
//!                 cue_llm::LlmProvider (existing)
//! ```
//!
//! `TaskClassifier` is a trait so the local heuristic implementation can be
//! swapped for a tiny-model classifier later (the managed Bluey routing
//! service). A `LayeredClassifier` runs the heuristic first and only escalates
//! to the model when confidence drops below a threshold.
//!
//! `SpeculativeRouter` is an optional wrapper that emits the `Instant` answer
//! immediately and, if the policy requested a `Deep` follow-up, fires it in
//! parallel and yields its chunks once the draft completes.
//!
//! See `docs/AUTO-ROUTING-USP.md` for the product framing.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod auto;
pub mod classifier;
pub mod heuristic;
pub mod model;
pub mod policy;
pub mod speculative;

pub use auto::{AutoRouter, RouteOptions, RoutedRequest};
pub use classifier::{ClassifierInput, LayeredClassifier, TaskClassifier};
pub use heuristic::HeuristicClassifier;
pub use model::{
    ContextNeeds, Difficulty, LatencyLane, ProviderRoute, TaskClassification, TaskType,
};
pub use policy::{LocalFallbackPolicy, ManagedPolicy, RoutingPolicy, StaticPolicy};
pub use speculative::{SpeculativeChunk, SpeculativeRouter};
