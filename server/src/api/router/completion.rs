
#[allow(clippy::result_large_err)]
async fn complete_inner(
    state: AppState,
    account: Account,
    req: CompleteRequest,
    trace_id: String,
) -> Result<CompleteResponse, (StatusCode, Json<ApiError>)> {
    reconcile_expired_llm_usage(&state.pool, &account.id).map_err(|error| *error)?;
    if let Some(err) = billing_restricted_error(&account) {
        return Err(err);
    }
    // 0. Validate request_id is non-empty.
    if req.request_id.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: "request_id is required and must be non-empty".into(),
                reason: Some("missing_request_id".into()),
                ..Default::default()
            }),
        ));
    }
    let trusted_envelope = TrustedInternalEnvelope::validate_direct_request(&req)
        .map_err(InternalDisclosureBlocked::into_api_error)?;

    validate_complete_images(&req.image_data_urls)
        .map_err(|error| image_validation_error(error.error, error.reason.unwrap_or_default()))?;
    validate_complete_context_schema_version(req.context_schema_version)
        .map_err(|error| image_validation_error(error.error, error.reason.unwrap_or_default()))?;
    validate_complete_context(&req.context)
        .map_err(|error| image_validation_error(error.error, error.reason.unwrap_or_default()))?;

    let requested_effective_lane = if req.image_data_urls.is_empty() {
        req.lane.clone()
    } else {
        "vision".to_string()
    };

    // Codex S4.4: managed dispatcher does not run local models.
    // The daemon's LocalFallbackPolicy must dispatch local-lane work
    // directly to on-device Ollama; the managed cloud path is not the
    // right home for it.
    if requested_effective_lane == "local" {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: "local lane is daemon-only; managed cloud does not run local models".into(),
                reason: Some("local_lane_unsupported".into()),
                ..Default::default()
            }),
        ));
    }

    let account_id_hash = cue_core::account_id_hash_prefix(&account.id);
    let session_id_log = log_session_id(req.session_id.as_deref()).to_string();
    let request_ref_log = short_observability_ref(Some(&req.request_id));
    let session_ref_log = short_observability_ref(req.session_id.as_deref());
    let lane_log = req.lane.clone();
    let requested_effective_lane_log = requested_effective_lane.clone();
    tracing::info!(
        account_id_hash = %account_id_hash,
        request_id = %req.request_id,
        request_ref = %request_ref_log,
        session_id = %session_id_log,
        session_ref = %session_ref_log,
        lane = %lane_log,
        requested_effective_lane = %requested_effective_lane_log,
        streaming = false,
        image_count = req.image_data_urls.len(),
        "managed chat request accepted"
    );

    // 1. Idempotency check + reservation. Codex S4.1.
    match idempotency::reserve(&state.pool, &account.id, &req.request_id).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: format!("idempotency: {e}"),
                ..Default::default()
            }),
        )
    })? {
        idempotency::ReserveOutcome::FreshReservation => {
            tracing::debug!(
                account_id_hash = %account_id_hash,
                request_id = %req.request_id,
                session_id = %session_id_log,
                streaming = false,
                "managed chat idempotency reserved"
            );
        }
        idempotency::ReserveOutcome::CachedComplete(json) => {
            // Replay: return the cached terminal response.
            let cached: CompleteResponse = serde_json::from_str(&json).map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiError {
                        error: format!("idempotency cache decode: {e}"),
                        ..Default::default()
                    }),
                )
            })?;
            tracing::info!(
                account_id_hash = %account_id_hash,
                request_id = %req.request_id,
                session_id = %session_id_log,
                streaming = false,
                "managed chat idempotency replayed completed response"
            );
            return Ok(cached);
        }
        idempotency::ReserveOutcome::InProgress => {
            tracing::warn!(
                account_id_hash = %account_id_hash,
                request_id = %req.request_id,
                session_id = %session_id_log,
                streaming = false,
                "managed chat duplicate request still in progress"
            );
            return Err((
                StatusCode::CONFLICT,
                Json(ApiError {
                    error: "request already in progress; wait for original to complete".into(),
                    reason: Some("request_in_progress".into()),
                    ..Default::default()
                }),
            ));
        }
        idempotency::ReserveOutcome::CachedFailed => {
            tracing::warn!(
                account_id_hash = %account_id_hash,
                request_id = %req.request_id,
                session_id = %session_id_log,
                streaming = false,
                "managed chat duplicate request previously failed terminally"
            );
            return Err((
                StatusCode::CONFLICT,
                Json(ApiError {
                    error: "previous attempt with this request_id failed; use a new request_id"
                        .into(),
                    reason: Some("request_failed_terminal".into()),
                    ..Default::default()
                }),
            ));
        }
    }

    if let Some(err) =
        prior_provider_prefix_exposure_error(&state.pool, &account.id, &req.request_id)
    {
        return Err(err);
    }

    if let Some(err) = account_not_active_error(&state.pool, &account.id) {
        let _ = idempotency::release(&state.pool, &account.id, &req.request_id);
        tracing::warn!(
            account_id_hash = %account_id_hash,
            request_id = %req.request_id,
            streaming = false,
            "managed chat stopped before dispatch because account is no longer active"
        );
        return Err(err);
    }

    let preliminary_answer_plan = answer_plan_for_request(&req, &requested_effective_lane, &[]);
    let story_grounding = behavioral_story_grounding(&req, &preliminary_answer_plan);
    if let BehavioralStoryGrounding::Missing { fields } = &story_grounding {
        let response =
            complete_grounding_guard_response(&state.pool, &account, &req.request_id, fields)
                .map_err(|error| *error)?;
        tracing::info!(
            account_id_hash = %account_id_hash,
            request_id = %req.request_id,
            session_id = %session_id_log,
            missing_story_fields = %fields.join(","),
            streaming = false,
            "behavioral story stopped before provider dispatch because verified facts were incomplete"
        );
        return Ok(response);
    }
    let story_provider_user =
        behavioral_provider_user(&req, &preliminary_answer_plan, &story_grounding);
    check_account_llm_or_short_wait(
        &state,
        &account.id,
        &req.request_id,
        &session_ref_log,
        false,
    )
    .await?;
    let should_lookup_memory = story_provider_user.is_none()
        && answer_plan_allows_memory_lookup(&preliminary_answer_plan)
        && should_lookup_completion_memory(&req, &requested_effective_lane);
    let rag_matches = if should_lookup_memory {
        completion_rag_matches_budgeted(
            &state.pool,
            &account.id,
            req.session_id.as_deref(),
            &req.user,
        )
        .await
    } else {
        Vec::new()
    };
    tracing::debug!(
        account_id_hash = %account_id_hash,
        request_id = %req.request_id,
        session_id = %session_id_log,
        memory_lookup = should_lookup_memory,
        rag_match_count = rag_matches.len(),
        streaming = false,
        "managed chat memory context prepared"
    );
    let resolved_answer_plan = resolve_answer_plan_for_request(
        &state,
        &account,
        &req,
        &requested_effective_lane,
        &rag_matches,
    )
    .await;
    if resolved_answer_plan.provider_accounting_pending {
        return Err(provider_accounting_pending_error(
            &state.pool,
            &account.id,
            &req.request_id,
        ));
    }
    let answer_plan = resolved_answer_plan.plan.clone();
    let resolved_story_grounding = behavioral_story_grounding(&req, &answer_plan);
    if let BehavioralStoryGrounding::Missing { fields } = &resolved_story_grounding {
        let response =
            complete_grounding_guard_response(&state.pool, &account, &req.request_id, fields)
                .map_err(|error| *error)?;
        tracing::info!(
            account_id_hash = %account_id_hash,
            request_id = %req.request_id,
            session_id = %session_id_log,
            missing_story_fields = %fields.join(","),
            answer_plan_source = resolved_answer_plan.source,
            streaming = false,
            "resolved behavioral story stopped before provider dispatch because user facts were incomplete"
        );
        return Ok(response);
    }
    let resolved_story_provider_user =
        behavioral_provider_user(&req, &answer_plan, &resolved_story_grounding)
            .or(story_provider_user);
    let request_diag = answer_request_diagnostics(&req);
    let answer_plan_routing = answer_plan_routing_enabled();
    let effective_lane =
        lane_for_answer_plan(&requested_effective_lane, &answer_plan, answer_plan_routing);
    let effective_lane_log = effective_lane.clone();
    tracing::debug!(
        account_id_hash = %account_id_hash,
        request_id = %req.request_id,
        requested_effective_lane = %requested_effective_lane_log,
        effective_lane = %effective_lane_log,
        answer_plan_routing,
        answer_plan_source = resolved_answer_plan.source,
        answer_plan_ai_attempted = resolved_answer_plan.ai_attempted,
        answer_plan_ai_reason = resolved_answer_plan.ai_reason,
        answer_intent = %answer_plan.intent.as_str(),
        answer_output = %answer_plan.output.as_str(),
        answer_confidence = answer_plan.confidence,
        needs_web_search = answer_plan.needs_web_search,
        needs_screen = answer_plan.needs_screen,
        needs_docs = answer_plan.needs_docs,
        needs_transcript = answer_plan.needs_transcript,
        user_chars = request_diag.user_chars,
        question_chars = request_diag.question_chars,
        question_hash = %request_diag.question_hash,
        context_chars = request_diag.context_chars,
        context_hash = %request_diag.context_hash,
        context_coding_signal = request_diag.context_coding_signal,
        transcript_chars = request_diag.transcript_chars,
        transcript_hash = %request_diag.transcript_hash,
        transcript_source_labels = request_diag.transcript_source_labels,
        generic_live_transcript_prompt = request_diag.generic_live_transcript_prompt,
        image_count = req.image_data_urls.len(),
        "managed chat answer plan resolved"
    );
    let web_search = completion_web_search_budgeted(
        &state.pool,
        state.config.upstream_spend_guard,
        &account,
        &req.request_id,
        &req.user,
        &answer_plan,
    )
    .await;
    if web_search.provider_accounting_pending {
        return Err(provider_accounting_pending_error(
            &state.pool,
            &account.id,
            &req.request_id,
        ));
    }
    let web_sources = web_search.sources.clone();
    let provider_rag_matches = if resolved_story_provider_user.is_some() {
        &[][..]
    } else {
        rag_matches.as_slice()
    };
    let (provider_system, provider_user) = prompt_with_rag_context(
        trusted_envelope.system,
        resolved_story_provider_user
            .as_deref()
            .unwrap_or(trusted_envelope.user),
        provider_rag_matches,
    );
    let (provider_system, provider_user) =
        prompt_with_web_context(&provider_system, &provider_user, &web_sources);
    let (provider_system, provider_user) = prompt_with_answer_plan_context(
        &provider_system,
        &provider_user,
        &req.context,
        &answer_plan,
        &web_search,
        req.max_tokens,
    );

    // 2. Resolve lane → provider+model candidates. The reservation uses the
    // maximum candidate estimate so provider failover cannot overrun a
    // customer's hard-stop budget.
    let thinking = routing::resolve_thinking_budget(
        &effective_lane,
        req.reasoning_effort.as_deref(),
        req.thinking_budget_tokens,
    );
    let vision_text_fallback_lane = managed_vision_text_fallback_lane(&answer_plan);
    let (vision_text_fallback_system, vision_text_fallback_user) =
        managed_vision_text_fallback_prompt(&provider_system, &provider_user);
    let vision_text_fallback_thinking = routing::resolve_thinking_budget(
        vision_text_fallback_lane,
        req.reasoning_effort.as_deref(),
        req.thinking_budget_tokens,
    );
    let vision_text_fallback_possible =
        effective_lane == "vision" && !req.image_data_urls.is_empty();
    let provider_max_tokens = max_tokens_for_answer_plan(req.max_tokens, answer_plan.output);
    let effective_max_out =
        estimate_max_output_tokens_for_answer_plan(req.max_tokens, thinking, answer_plan.output);
    let vision_text_fallback_max_out = estimate_max_output_tokens_for_answer_plan(
        req.max_tokens,
        vision_text_fallback_thinking,
        answer_plan.output,
    );
    let quality_max_tokens = if vision_text_fallback_possible {
        effective_max_out.max(vision_text_fallback_max_out)
    } else {
        effective_max_out
    };
    let max_out = i64::from(quality_max_tokens);
    let primary_server_est_in = complete_input_token_upper_bound(
        &provider_system,
        &provider_user,
        req.image_data_urls.len(),
    );
    let server_est_in = if vision_text_fallback_possible {
        primary_server_est_in.max(pricing::utf8_input_token_upper_bound([
            vision_text_fallback_system.as_str(),
            vision_text_fallback_user.as_str(),
        ]))
    } else {
        primary_server_est_in
    };
    let est_in = req
        .estimated_input_tokens
        .unwrap_or_default()
        .max(server_est_in);
    let mut routes = priced_routes_for(&effective_lane, est_in, max_out, &req.request_id);
    let normalized_route_question = normalize_guardrail_text(&extract_search_question(&req.user));
    let design_quality_route_prioritized = prioritize_routes_for_answer_plan(
        &mut routes,
        &effective_lane,
        &answer_plan,
        &normalized_route_question,
        !env_flag_is_false("BLUEY_BALANCED_DESIGN_QUALITY_ROUTE"),
    );
    if routes.is_empty() {
        let _ = idempotency::mark_failed(&state.pool, &account.id, &req.request_id);
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: format!("no priced route for lane {effective_lane}"),
                ..Default::default()
            }),
        ));
    }
    if let Some(first_route) = routes.first() {
        tracing::debug!(
            request_id = %req.request_id,
            lane = %effective_lane,
            first_provider = %first_route.provider,
            first_model = %first_route.model,
            candidate_count = routes.len(),
            design_quality_route_prioritized,
            "resolved LLM route candidates"
        );
    }
    let vision_text_fallback_routes = if vision_text_fallback_possible {
        priced_routes_for(vision_text_fallback_lane, est_in, max_out, &req.request_id)
    } else {
        Vec::new()
    };

    // 3. Estimate cost ceiling for the entry check.
    let est_cost = routes
        .iter()
        .chain(vision_text_fallback_routes.iter())
        .map(|route| route.estimated_cost_cents)
        .max()
        .unwrap_or(1)
        .saturating_add(web_search.customer_cost_cents);
    if let Some(err) = prior_provider_exposure_error(
        &state,
        &account.id,
        &req.request_id,
        &format!("router:{}:llm", req.request_id),
    ) {
        return Err(err);
    }

    // 4. Atomically reserve the maximum customer charge before dispatch.
    let usage_reservation =
        reserve_llm_usage(&state, &account, &req.request_id, est_cost, 0, "llm")
            .map_err(|error| *error)?;
    let on_trial = usage_reservation.is_trial();
    tracing::info!(
        account_id_hash = %account_id_hash,
        request_id = %req.request_id,
        attempt = usage_reservation.attempt,
        reserved_cents = usage_reservation.reserved_cents,
        reserved_trial_seconds = usage_reservation.reserved_trial_seconds,
        expires_at_ms = usage_reservation.expires_at_ms,
        streaming = false,
        "managed chat usage reserved before provider dispatch"
    );

    // 5. Dispatch to upstream provider. Try candidate routes in order. Provider
    //    capacity is checked before each attempt, so a provider 429/rate-limit
    //    storm degrades to another route instead of failing the active call.
    //    Pass the entry estimate so the dispatcher can fall back to it if the
    //    upstream omits `usage`. Codex S4.5.
    let started = Instant::now();
    let mut last_error: Option<anyhow::Error> = None;
    let mut last_capacity: Option<crate::rate_limit::CapacityDenied> = None;
    let mut last_failure_was_capacity = false;
    let mut selected_route_idx = 0usize;
    let mut selected_route: Option<&PricedRoute> = None;
    let mut selected_completion: Option<routing::Completion> = None;
    let mut vision_text_fallback_active = false;
    let mut vision_media_rejection_seen = false;
    let mut provider_dispatch_index = 0_usize;

    let mut capacity_sweeps_used = 0usize;
    for capacity_sweep in 0..=1 {
        capacity_sweeps_used = capacity_sweep;
        if capacity_sweep > 0 {
            last_error = None;
            last_capacity = None;
            last_failure_was_capacity = false;
            selected_route_idx = 0;
            selected_route = None;
            selected_completion = None;
        }

        let mut route_cursor = 0usize;
        loop {
            let active_routes = if vision_text_fallback_active {
                &vision_text_fallback_routes
            } else {
                &routes
            };
            if route_cursor >= active_routes.len() {
                if !vision_text_fallback_active
                    && managed_vision_text_fallback_ready(
                        route_cursor >= routes.len(),
                        vision_media_rejection_seen,
                        !vision_text_fallback_routes.is_empty(),
                    )
                {
                    tracing::warn!(
                        account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                        request_id = %req.request_id,
                        fallback_lane = vision_text_fallback_lane,
                        vision_routes_attempted = routes.len(),
                        "all managed vision routes exhausted after explicit media rejection; activating degraded text fallback"
                    );
                    vision_text_fallback_active = true;
                    route_cursor = 0;
                    last_capacity = None;
                    last_failure_was_capacity = false;
                    continue;
                }
                break;
            }
            let route_index_offset = if vision_text_fallback_active {
                routes.len()
            } else {
                0
            };
            let dispatch_lane = if vision_text_fallback_active {
                vision_text_fallback_lane
            } else {
                effective_lane.as_str()
            };
            let dispatch_system = if vision_text_fallback_active {
                vision_text_fallback_system.as_str()
            } else {
                provider_system.as_str()
            };
            let dispatch_user = if vision_text_fallback_active {
                vision_text_fallback_user.as_str()
            } else {
                provider_user.as_str()
            };
            let dispatch_thinking = if vision_text_fallback_active {
                vision_text_fallback_thinking
            } else {
                thinking
            };
            let dispatch_images: &[String] = if vision_text_fallback_active {
                &[]
            } else {
                &req.image_data_urls
            };
            let idx = route_cursor;
            route_cursor += 1;
            let route = &active_routes[idx];
            let route_index = route_index_offset + idx;
            let key_candidates = state.config.upstream.key_candidates(
                route.provider,
                &format!("llm:{}:{}:{}", req.request_id, route.provider, route.model),
            );
            if key_candidates.is_empty() {
                last_error = Some(missing_provider_key_error(route.provider));
                continue;
            }
            loop {
                let selected_key = match state
                    .provider_health
                    .choose_key(route.provider, route.model, &key_candidates)
                    .await
                {
                    Ok(key) => key,
                    Err(denied) => {
                        tracing::warn!(
                            account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                            request_id = %req.request_id,
                            provider = %route.provider,
                            model = %route.model,
                            retry_after_secs = denied.retry_after_secs,
                            reason = denied.reason,
                            "provider key pool cooling down; trying next route"
                        );
                        last_capacity = Some(denied);
                        last_failure_was_capacity = true;
                        break;
                    }
                };

                if let Err(denied) = state
                    .rate_limiters
                    .check_provider_llm(route.provider, route.model)
                    .await
                {
                    tracing::warn!(
                        account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                        request_id = %req.request_id,
                        provider = %route.provider,
                        model = %route.model,
                        retry_after_secs = denied.retry_after_secs,
                        reason = denied.reason,
                        "provider capacity busy; trying next route"
                    );
                    last_capacity = Some(denied);
                    last_failure_was_capacity = true;
                    break;
                }

                let attempt_request_id =
                    format!("{}:llm-attempt:{}", req.request_id, provider_dispatch_index);
                provider_dispatch_index = provider_dispatch_index.saturating_add(1);
                let mut attempt_guard = match provider_cost_guard::reserve(
                    &state.pool,
                    state.config.upstream_spend_guard,
                    &account.id,
                    &format!("router:{}:llm", req.request_id),
                    &attempt_request_id,
                    route.provider,
                    route.model,
                    route.estimated_bluey_cost_cents,
                    "llm_attempt",
                    dispatch_lane,
                ) {
                    Ok(provider_cost_guard::Admission::Held(guard)) => guard,
                    Ok(provider_cost_guard::Admission::Unconfigured) => {
                        last_error = Some(anyhow::anyhow!(
                            "paid LLM route unexpectedly had zero projected exposure"
                        ));
                        break;
                    }
                    Ok(provider_cost_guard::Admission::GlobalLimit) => {
                        release_llm_usage(
                            &state.pool,
                            &account.id,
                            &req.request_id,
                            "upstream_spend_guard",
                        );
                        return Err(release_and_upstream_spend_guard_error(
                            &state.pool,
                            &account.id,
                            &req.request_id,
                        ));
                    }
                    Err(error) => {
                        last_error = Some(
                            error.context("durable upstream spend admission failed for LLM route"),
                        );
                        last_failure_was_capacity = false;
                        break;
                    }
                };
                let attempt_started = Instant::now();

                match routing::complete_with_key(
                    &selected_key.secret,
                    route.provider,
                    route.model,
                    dispatch_system,
                    dispatch_user,
                    provider_max_tokens,
                    req.temperature,
                    dispatch_thinking,
                    Some(est_in),
                    dispatch_images,
                )
                .await
                {
                    Ok(completion) => {
                        let route_matches = completion.provider == route.provider
                            && completion.model == route.model;
                        let actual_bluey_cost = returned_route_bluey_cost_or_cap(
                            &completion.provider,
                            &completion.model,
                            completion.input_tokens,
                            completion.output_tokens,
                        );
                        let attempt_event = UsageEvent {
                            request_id: attempt_request_id,
                            kind: "llm_attempt".into(),
                            task_type: Some(dispatch_lane.to_string()),
                            lane: Some(dispatch_lane.to_string()),
                            provider: Some(completion.provider.clone()),
                            model: Some(completion.model.clone()),
                            input_tokens: completion.input_tokens,
                            output_tokens: completion.output_tokens,
                            latency_ms: attempt_started
                                .elapsed()
                                .as_millis()
                                .try_into()
                                .unwrap_or(i64::MAX),
                            cost_cents_to_bluey: actual_bluey_cost,
                            cost_cents_to_customer: 0,
                            was_speculative: false,
                            was_fallback: route_index > 0,
                        };
                        settle_provider_attempt_before_customer(
                            &state.pool,
                            &account.id,
                            &req.request_id,
                            &mut attempt_guard,
                            attempt_event,
                            actual_bluey_cost,
                            completion.usage_provenance,
                        )?;
                        if !route_matches {
                            last_error = Some(anyhow::anyhow!("provider route identity mismatch"));
                            last_failure_was_capacity = false;
                            break;
                        }
                        selected_route_idx = route_index;
                        selected_route = Some(route);
                        tracing::info!(
                            account_id_hash = %account_id_hash,
                            request_id = %req.request_id,
                            request_ref = %request_ref_log,
                            session_id = %session_id_log,
                            session_ref = %session_ref_log,
                            lane = %lane_log,
                            effective_lane = %effective_lane_log,
                            provider = %route.provider,
                            model = %route.model,
                            dispatch_lane,
                            vision_text_fallback = vision_text_fallback_active,
                            route_index,
                            was_fallback = route_index > 0,
                            streaming = false,
                            "managed chat route selected"
                        );
                        selected_completion = Some(completion);
                        break;
                    }
                    Err(e) => {
                        if let Err(error) = attempt_guard.settle_conservative() {
                            tracing::error!(request_id = %req.request_id, error = %error, "LLM failed-attempt settlement pending reconciliation");
                            return Err(provider_accounting_pending_error(
                                &state.pool,
                                &account.id,
                                &req.request_id,
                            ));
                        }
                        if !vision_text_fallback_active
                            && !vision_text_fallback_routes.is_empty()
                            && managed_vision_text_fallback_eligible(
                                &req,
                                &effective_lane,
                                route.provider,
                                &e,
                            )
                        {
                            tracing::warn!(
                                account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                                request_id = %req.request_id,
                                provider = %route.provider,
                                model = %route.model,
                                error = %e,
                                "managed vision request explicitly rejected media; trying remaining vision routes"
                            );
                            last_error = Some(e);
                            last_capacity = None;
                            last_failure_was_capacity = false;
                            vision_media_rejection_seen = true;
                            break;
                        }
                        if let Some(retry_after_secs) = routing::upstream_retry_after(&e) {
                            let cooldown_secs = state
                                .provider_health
                                .record_cooldown(
                                    route.provider,
                                    route.model,
                                    &selected_key.fingerprint,
                                    retry_after_secs,
                                )
                                .await;
                            tracing::warn!(
                                account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                                request_id = %req.request_id,
                                provider = %route.provider,
                                model = %route.model,
                                key_fingerprint = %selected_key.fingerprint,
                                retry_after_secs = cooldown_secs,
                                error = %e,
                                "upstream capacity response; cooled key and retrying route"
                            );
                            last_capacity = Some(crate::rate_limit::CapacityDenied {
                                retry_after_secs: cooldown_secs,
                                reason: "provider_key_cooling_down",
                            });
                            last_failure_was_capacity = true;
                            continue;
                        }
                        tracing::warn!(
                            account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                            request_id = %req.request_id,
                            provider = %route.provider,
                            model = %route.model,
                            error = %e,
                            "upstream dispatch failed; trying next route"
                        );
                        last_error = Some(e);
                        last_failure_was_capacity = false;
                        break;
                    }
                }
            }

            if selected_completion.is_some() {
                break;
            }
        }

        if selected_completion.is_some() {
            break;
        }
        if let Some(denied) = last_capacity
            .as_ref()
            .filter(|_| last_failure_was_capacity)
            .and_then(internal_capacity_retry_delay)
        {
            tracing::warn!(
                account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                request_id = %req.request_id,
                request_ref = %request_ref_log,
                session_ref = %session_ref_log,
                capacity_sweep,
                wait_ms = denied.as_millis() as u64,
                "all routes briefly capacity busy; waiting before internal retry sweep"
            );
            tokio::time::sleep(denied).await;
            continue;
        }
        break;
    }
    let elapsed = started.elapsed();
    let elapsed_ms = elapsed.as_millis() as i64;

    let (selected_route, comp) = match (selected_route, selected_completion) {
        (Some(route), Some(completion)) => (route, completion),
        _ => {
            release_llm_usage(
                &state.pool,
                &account.id,
                &req.request_id,
                "provider_dispatch_failed",
            );
            if last_failure_was_capacity {
                let denied = last_capacity.expect("capacity flag set with no capacity denial");
                tracing::warn!(
                    account_id_hash = %account_id_hash,
                    request_id = %req.request_id,
                    request_ref = %request_ref_log,
                    session_ref = %session_ref_log,
                    lane = %lane_log,
                    effective_lane = %effective_lane_log,
                    reason = denied.reason,
                    retry_after_secs = denied.retry_after_secs,
                    candidate_routes = routes.len(),
                    capacity_sweeps_used,
                    streaming = false,
                    "all routes still capacity-busy after fallback scan"
                );
                record_answer_ops_event(
                    &state.pool,
                    AnswerOpsEvent {
                        account_id: &account.id,
                        request_id: &req.request_id,
                        session_id: req.session_id.as_deref(),
                        trace_id: Some(&trace_id),
                        event_type: "answer_capacity_busy",
                        status: "capacity_busy",
                        metadata: serde_json::json!({
                        "lane": lane_log.as_str(),
                        "effective_lane": effective_lane_log.as_str(),
                        "streaming": false,
                        "reason": denied.reason,
                        "retry_after_secs": denied.retry_after_secs,
                        "candidate_routes": routes.len(),
                        "capacity_sweeps_used": capacity_sweeps_used
                        }),
                    },
                );
                return Err(capacity_error(denied.reason, denied.retry_after_secs));
            }
            if let Some(e) = last_error {
                // Codex S4.6: log raw upstream details, return sanitized
                // message to the customer. We DO NOT mark the idempotency
                // row as failed-terminal because a transient upstream error
                // should be retryable with the same request_id.
                tracing::warn!(
                    account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                    request_id = %req.request_id,
                    request_ref = %request_ref_log,
                    session_ref = %session_ref_log,
                    error = %e,
                    "all upstream dispatch routes failed"
                );
                record_answer_ops_event(
                    &state.pool,
                    AnswerOpsEvent {
                        account_id: &account.id,
                        request_id: &req.request_id,
                        session_id: req.session_id.as_deref(),
                        trace_id: Some(&trace_id),
                        event_type: "answer_failed",
                        status: "upstream_error",
                        metadata: serde_json::json!({
                        "lane": lane_log.as_str(),
                        "effective_lane": effective_lane_log.as_str(),
                        "streaming": false,
                        "error_kind": "all_routes_failed",
                        "error_preview": truncate_chars(&e.to_string(), 180),
                        "candidate_routes": routes.len()
                        }),
                    },
                );
            }
            return Err((
                StatusCode::BAD_GATEWAY,
                Json(ApiError {
                    error: "upstream provider error; please retry".into(),
                    reason: Some("upstream_error".into()),
                    ..Default::default()
                }),
            ));
        }
    };

    if let Some(err) = account_not_active_error(&state.pool, &account.id) {
        let _ = idempotency::mark_failed(&state.pool, &account.id, &req.request_id);
        release_llm_usage(
            &state.pool,
            &account.id,
            &req.request_id,
            "account_inactive",
        );
        tracing::warn!(
            account_id_hash = %account_id_hash,
            request_id = %req.request_id,
            streaming = false,
            "managed chat stopped before billing because account is no longer active"
        );
        return Err(err);
    }

    let strip_interview_coaching_appendix =
        should_strip_unsolicited_coaching_appendix(&answer_plan, &req.user);
    let evidence_bound_role_reference = interview_contracts::evidence_bound_role_reference(
        &normalize_guardrail_text(&extract_search_question(&req.user)),
        &req.context,
        &answer_plan,
    );
    let mut provider_quality_output =
        BufferedDisclosureOutput::new(strip_interview_coaching_appendix);
    let _ = provider_quality_output.push(&comp.text);
    let (provider_quality_text, _) = provider_quality_output.finish();
    let presentation_text = interview_contracts::anchor_complete_provider_answer(
        &comp.text,
        evidence_bound_role_reference,
    );
    let mut output = BufferedDisclosureOutput::new(strip_interview_coaching_appendix);
    let _ = output.push(&presentation_text);
    let (response_text, _) = output.finish();
    if let Some(reason) = generated_answer_quality_failure(
        &provider_quality_text,
        comp.output_tokens,
        Some(quality_max_tokens),
        &answer_plan,
        &normalized_route_question,
    ) {
        let _ = idempotency::mark_failed(&state.pool, &account.id, &req.request_id);
        release_llm_usage(&state.pool, &account.id, &req.request_id, reason);
        tracing::warn!(
            account_id_hash = %account_id_hash,
            request_id = %req.request_id,
            provider = %comp.provider,
            model = %comp.model,
            output_tokens = comp.output_tokens,
            max_tokens = quality_max_tokens,
            reason,
            streaming = false,
            "provider returned an incomplete answer"
        );
        return Err((
            StatusCode::BAD_GATEWAY,
            Json(ApiError {
                error: "upstream provider returned an incomplete answer; please retry".into(),
                reason: Some(reason.into()),
                retry_after_secs: Some(1),
                ..Default::default()
            }),
        ));
    }
    let artifact = response_artifact_for_plan(&response_text, &answer_plan);
    if code_artifact_missing_for_plan(&answer_plan, artifact.as_ref()) {
        let _ = idempotency::mark_failed(&state.pool, &account.id, &req.request_id);
        release_llm_usage(
            &state.pool,
            &account.id,
            &req.request_id,
            "code_artifact_missing",
        );
        tracing::warn!(
            account_id_hash = %account_id_hash,
            request_id = %req.request_id,
            request_ref = %request_ref_log,
            session_id = %session_id_log,
            session_ref = %session_ref_log,
            lane = %lane_log,
            effective_lane = %effective_lane_log,
            provider = %comp.provider,
            model = %comp.model,
            answer_intent = %answer_plan.intent.as_str(),
            answer_output = %answer_plan.output.as_str(),
            text_chars = response_text.chars().count(),
            streaming = false,
            "code artifact expected but missing before billing"
        );
        record_answer_ops_event(
            &state.pool,
            AnswerOpsEvent {
                account_id: &account.id,
                request_id: &req.request_id,
                session_id: req.session_id.as_deref(),
                trace_id: Some(&trace_id),
                event_type: "answer_failed",
                status: "code_artifact_missing",
                metadata: serde_json::json!({
                "lane": lane_log.as_str(),
                "effective_lane": effective_lane_log.as_str(),
                "provider": comp.provider.as_str(),
                "model": comp.model.as_str(),
                "streaming": false,
                "answer_intent": answer_plan.intent.as_str(),
                "answer_output": answer_plan.output.as_str(),
                "text_chars": response_text.chars().count(),
                "question_hash": request_diag.question_hash,
                "context_hash": request_diag.context_hash,
                "context_coding_signal": request_diag.context_coding_signal
                }),
            },
        );
        return Err(code_artifact_missing_error());
    }

    // 6. Compute actual cost from real token counts.
    let (llm_bluey_cost, llm_customer_cost) = pricing::compute_cost(
        &selected_route.pricing,
        comp.input_tokens,
        comp.output_tokens,
    );
    let bluey_cost = llm_bluey_cost.saturating_add(web_search.bluey_cost_cents);
    let customer_cost = llm_customer_cost.saturating_add(web_search.customer_cost_cents);
    let event = UsageEvent {
        request_id: req.request_id.clone(),
        kind: "llm".into(),
        task_type: None,
        lane: Some(effective_lane.clone()),
        provider: Some(comp.provider.clone()),
        model: Some(comp.model.clone()),
        input_tokens: comp.input_tokens,
        output_tokens: comp.output_tokens,
        latency_ms: elapsed_ms,
        // Per-attempt durable holds are the sole upstream-cost authority.
        cost_cents_to_bluey: 0,
        cost_cents_to_customer: llm_customer_cost,
        was_speculative: false,
        was_fallback: selected_route_idx > 0,
    };
    let settlement_events = managed_completion_settlement_events(
        &req.request_id,
        event,
        llm_customer_cost,
        &web_search,
    );

    // 7. Settle actual usage and atomically refund the unused ceiling.
    let settled_usage = settle_llm_usage_with_retry(
        &state.pool,
        &account.id,
        &req.request_id,
        customer_cost,
        elapsed_ms,
        "completed",
        &settlement_events,
    )
    .await
    .map_err(|error| {
        tracing::error!(
            account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
            request_id = %req.request_id,
            error = %error,
            "managed usage settlement failed after provider completion"
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: "usage settlement is pending reconciliation".into(),
                reason: Some("usage_settlement_pending".into()),
                ..Default::default()
            }),
        )
    })?;
    let charged_customer_cost = settled_usage.charged_customer_cents;
    let charged_llm_customer_cost = if on_trial {
        0
    } else {
        llm_customer_cost.min(charged_customer_cost)
    };
    let charged_web_search_customer_cost = charged_customer_cost
        .saturating_sub(charged_llm_customer_cost)
        .min(web_search.customer_cost_cents);
    let trial_remaining = settled_usage.trial_seconds_remaining;
    let balance_after = settled_usage.balance_cents_after;

    // Codex Stage 10: auto top-up trigger. Fire-and-forget; the actual
    // charge resolves on the executor and the webhook for the
    // resulting payment_intent.succeeded credits the balance via the
    // existing /billing/webhook flow. This closes the v0.2 dealbreaker
    // gap: customers no longer hit hard-stop without an obvious recovery.
    if !on_trial {
        crate::billing::topup::maybe_spawn(
            state.pool.clone(),
            state.config.clone(),
            account.id.clone(),
            balance_after,
            account.auto_topup_enabled,
            account.auto_topup_threshold_cents,
            account.stripe_customer_id.clone(),
            account.stripe_payment_method_id.clone(),
            account.square_customer_id.clone(),
            account.square_card_id.clone(),
            account.auto_topup_amount_cents,
        );
    }

    tracing::info!(
        account_id_hash = %account_id_hash,
        request_id = %req.request_id,
        request_ref = %request_ref_log,
        trace_id = %trace_id,
        session_id = %session_id_log,
        session_ref = %session_ref_log,
        provider = %comp.provider,
        model = %comp.model,
        cost_cents = charged_llm_customer_cost,
        web_search_cost_cents = charged_web_search_customer_cost,
        balance_cents_after = balance_after,
        latency_ms = elapsed_ms,
        streaming = false,
        "managed chat usage settled with all authoritative events"
    );

    let artifact_type = artifact
        .as_ref()
        .map(|artifact| artifact.artifact_type)
        .unwrap_or("none");
    let artifact_confidence = artifact
        .as_ref()
        .map(|artifact| artifact.confidence)
        .unwrap_or(0.0);
    let web_search_skipped_reason = web_search.skipped_reason.unwrap_or("none");

    tracing::info!(
        account_id_hash = %account_id_hash,
        request_id = %req.request_id,
        request_ref = %request_ref_log,
        session_id = %session_id_log,
        session_ref = %session_ref_log,
        lane = %lane_log,
        effective_lane = %effective_lane_log,
        provider = %comp.provider,
        model = %comp.model,
        input_tokens = comp.input_tokens,
        output_tokens = comp.output_tokens,
        cost_cents = charged_customer_cost,
        bluey_cost_cents = bluey_cost,
        llm_cost_cents = charged_llm_customer_cost,
        web_search_cost_cents = web_search.customer_cost_cents,
        balance_cents_after = balance_after,
        trial_seconds_remaining = trial_remaining,
        latency_ms = elapsed_ms,
        was_fallback = selected_route_idx > 0,
        answer_plan_source = resolved_answer_plan.source,
        answer_plan_ai_attempted = resolved_answer_plan.ai_attempted,
        answer_plan_ai_reason = resolved_answer_plan.ai_reason,
        answer_intent = %answer_plan.intent.as_str(),
        answer_output = %answer_plan.output.as_str(),
        answer_confidence = answer_plan.confidence,
        user_chars = request_diag.user_chars,
        question_chars = request_diag.question_chars,
        question_hash = %request_diag.question_hash,
        context_chars = request_diag.context_chars,
        context_hash = %request_diag.context_hash,
        context_coding_signal = request_diag.context_coding_signal,
        transcript_chars = request_diag.transcript_chars,
        transcript_hash = %request_diag.transcript_hash,
        transcript_source_labels = request_diag.transcript_source_labels,
        generic_live_transcript_prompt = request_diag.generic_live_transcript_prompt,
        image_count = req.image_data_urls.len(),
        canvas_artifact_type = artifact_type,
        canvas_artifact_confidence = artifact_confidence,
        web_search_attempted = web_search.attempted,
        web_search_searches_used = web_search.searches_used,
        web_search_sources = web_search.sources.len(),
        web_search_skipped_reason,
        streaming = false,
        "managed chat completed and billed"
    );

    let visible_response_text =
        visible_response_text_for_plan(&response_text, artifact.as_ref(), &answer_plan);
    let response = CompleteResponse {
        text: visible_response_text,
        provider: comp.provider,
        model: comp.model,
        input_tokens: comp.input_tokens,
        output_tokens: comp.output_tokens,
        cost_cents: charged_customer_cost,
        balance_cents_after: balance_after,
        trial_seconds_remaining: trial_remaining,
        artifact_type: artifact
            .as_ref()
            .map(|artifact| artifact.artifact_type.to_string()),
        artifact_body: artifact.as_ref().map(|artifact| artifact.body.clone()),
        cost_label: Some(router_cost_label_with_web_search(
            charged_customer_cost,
            balance_after,
            &web_search,
        )),
        confidence: artifact.as_ref().map(|artifact| artifact.confidence),
        sources: web_sources,
    };

    // 9. Cache the terminal response in the idempotency row so a retry
    //    returns this exact body without re-dispatching.
    // Codex Stage 9c (S4 round-2 nit): mark_complete failure must NOT
    // be silently dropped. The customer has been billed and the upstream
    // call has finished; if we cannot persist the cached response, a
    // retry hits the in_progress reservation and 409s the customer
    // permanently. Log at error with the request_id so SREs can
    // reconcile manually. A future stage adds a Prometheus counter at
    // /admin/metrics.
    match serde_json::to_string(&response) {
        Ok(json) => {
            if let Err(e) =
                idempotency::mark_complete(&state.pool, &account.id, &req.request_id, &json)
            {
                tracing::error!(
                    account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                    request_id = %req.request_id,
                    error = %e,
                    "idempotency::mark_complete failed AFTER customer billed; retry will return 409 — manual reconciliation required"
                );
            }
        }
        Err(e) => {
            tracing::error!(
                account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                request_id = %req.request_id,
                error = %e,
                "failed to serialize response for idempotency cache; retry will return 409 — manual reconciliation required"
            );
        }
    }

    Ok(response)
}
