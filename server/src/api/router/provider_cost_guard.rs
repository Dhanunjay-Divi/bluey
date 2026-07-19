//! Durable projected upstream-cost holds for non-Jobs provider attempts.

use anyhow::Result;

use crate::{
    config::UpstreamSpendGuard,
    db::{
        jobs_provider_cost_holds::{self, CostHoldReservation},
        usage::{UsageEvent, MAX_AUTHORITATIVE_EVENT_COST_CENTS},
        DbPool,
    },
    pricing::UsageProvenance,
};

pub(crate) enum Admission {
    Unconfigured,
    Held(Box<ProviderCostGuard>),
    GlobalLimit,
}

pub(crate) struct ProviderCostGuard {
    pool: DbPool,
    account_id: String,
    scope_key: String,
    reservation_token: String,
    request_id: String,
    requested_provider: String,
    requested_model: String,
    projected_cost_cents: i64,
    fallback_cost_cents: i64,
    fallback_event: UsageEvent,
    armed: bool,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn reserve(
    pool: &DbPool,
    guard: Option<UpstreamSpendGuard>,
    account_id: &str,
    scope_key: &str,
    request_id: &str,
    provider: &str,
    model: &str,
    projected_cost_cents: i64,
    kind: &str,
    task_type: &str,
) -> Result<Admission> {
    if projected_cost_cents <= 0 {
        // Truly zero-cost local routes do not create paid exposure.
        return Ok(Admission::Unconfigured);
    }
    let Some(guard) = guard else {
        // Paid dispatch is fail-closed when no durable global spend boundary
        // is configured. Callers must treat this as a denied route.
        return Ok(Admission::GlobalLimit);
    };
    let projected_cost_cents = projected_cost_cents.clamp(1, MAX_AUTHORITATIVE_EVENT_COST_CENTS);
    let reservation_token = uuid::Uuid::new_v4().to_string();
    match jobs_provider_cost_holds::reserve(
        pool,
        account_id,
        scope_key,
        &reservation_token,
        request_id,
        provider,
        model,
        projected_cost_cents,
        MAX_AUTHORITATIVE_EVENT_COST_CENTS,
        guard,
    )? {
        CostHoldReservation::Held { reservation_token } => {
            Ok(Admission::Held(Box::new(ProviderCostGuard::new(
                pool,
                account_id,
                scope_key,
                reservation_token,
                request_id,
                provider,
                model,
                projected_cost_cents,
                kind,
                task_type,
            ))))
        }
        CostHoldReservation::RecoveredAmbiguous { reservation_token } => {
            // A prior process may have dispatched this exact attempt and
            // crashed before settlement. Conservatively terminalize the old
            // hold and deny redispatch; a new immutable attempt id is required.
            drop(ProviderCostGuard::new(
                pool,
                account_id,
                scope_key,
                reservation_token,
                request_id,
                provider,
                model,
                projected_cost_cents,
                kind,
                task_type,
            ));
            Ok(Admission::GlobalLimit)
        }
        CostHoldReservation::GlobalLimit | CostHoldReservation::GenerationLimit => {
            Ok(Admission::GlobalLimit)
        }
    }
}

impl ProviderCostGuard {
    #[allow(clippy::too_many_arguments)]
    fn new(
        pool: &DbPool,
        account_id: &str,
        scope_key: &str,
        reservation_token: String,
        request_id: &str,
        provider: &str,
        model: &str,
        projected_cost_cents: i64,
        kind: &str,
        task_type: &str,
    ) -> Self {
        Self {
            pool: pool.clone(),
            account_id: account_id.to_string(),
            scope_key: scope_key.to_string(),
            reservation_token,
            request_id: request_id.to_string(),
            requested_provider: provider.to_string(),
            requested_model: model.to_string(),
            projected_cost_cents,
            fallback_cost_cents: projected_cost_cents,
            fallback_event: UsageEvent {
                request_id: request_id.to_string(),
                kind: kind.to_string(),
                task_type: Some(task_type.to_string()),
                lane: None,
                provider: Some(provider.to_string()),
                model: Some(model.to_string()),
                input_tokens: 0,
                output_tokens: 0,
                latency_ms: 0,
                cost_cents_to_bluey: projected_cost_cents,
                cost_cents_to_customer: 0,
                was_speculative: false,
                was_fallback: false,
            },
            armed: true,
        }
    }

    pub(crate) fn settle(
        &mut self,
        mut event: UsageEvent,
        actual_cost_cents: i64,
        usage_provenance: UsageProvenance,
    ) -> Result<()> {
        let route_matches = event.provider.as_deref() == Some(self.requested_provider.as_str())
            && event.model.as_deref() == Some(self.requested_model.as_str());
        let reported_cost = actual_cost_cents.clamp(0, MAX_AUTHORITATIVE_EVENT_COST_CENTS);
        let trusted_exact = route_matches && usage_provenance.is_exact();
        let settled_cost = if trusted_exact {
            reported_cost
        } else {
            self.projected_cost_cents
                .max(reported_cost)
                .clamp(0, MAX_AUTHORITATIVE_EVENT_COST_CENTS)
        };
        let projection_exceeded = trusted_exact && reported_cost > self.projected_cost_cents;
        let persisted_provenance = if route_matches {
            usage_provenance
        } else {
            UsageProvenance::Missing
        };
        event.request_id = self.request_id.clone();
        // The hold is bound to the requested route. A malformed/crossed
        // completion is still durably charged to that immutable boundary.
        event.provider = Some(self.requested_provider.clone());
        event.model = Some(self.requested_model.clone());
        event.cost_cents_to_bluey = settled_cost;
        self.fallback_cost_cents = settled_cost;
        self.fallback_event = event.clone();
        jobs_provider_cost_holds::settle_with_usage(
            &self.pool,
            &self.account_id,
            &self.request_id,
            &self.reservation_token,
            reported_cost,
            persisted_provenance,
            &event,
        )?;
        self.armed = false;
        if projection_exceeded {
            tracing::error!(
                account_id_hash = %cue_core::account_id_hash_prefix(&self.account_id),
                request_id = %self.request_id,
                provider = %self.requested_provider,
                model = %self.requested_model,
                projected_cost_cents = self.projected_cost_cents,
                exact_cost_cents = reported_cost,
                "exact provider cost exceeded its pre-dispatch upper bound"
            );
            anyhow::bail!("exact provider cost exceeded pre-dispatch upper bound")
        }
        Ok(())
    }

    /// Terminalize an attempt whose exact provider usage is unavailable (for
    /// example, timeout or transport failure after dispatch). Callers must use
    /// this synchronously before continuing to another provider or settling
    /// customer usage; Drop remains only a last-resort crash safeguard.
    pub(crate) fn settle_conservative(&mut self) -> Result<()> {
        jobs_provider_cost_holds::settle_with_usage(
            &self.pool,
            &self.account_id,
            &self.request_id,
            &self.reservation_token,
            self.fallback_cost_cents,
            UsageProvenance::Missing,
            &self.fallback_event,
        )?;
        self.armed = false;
        Ok(())
    }
}

impl Drop for ProviderCostGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        if let Err(error) = jobs_provider_cost_holds::settle_with_usage(
            &self.pool,
            &self.account_id,
            &self.request_id,
            &self.reservation_token,
            self.fallback_cost_cents,
            UsageProvenance::Missing,
            &self.fallback_event,
        ) {
            tracing::error!(
                account_id_hash = %cue_core::account_id_hash_prefix(&self.account_id),
                scope = %self.scope_key,
                error = %error,
                "failed to settle durable provider cost hold"
            );
        }
    }
}
