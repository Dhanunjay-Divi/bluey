
pub async fn complete(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Extension(crate::api::middleware::request_id::TraceId(trace_id)): Extension<
        crate::api::middleware::request_id::TraceId,
    >,
    Json(req): Json<CompleteRequest>,
) -> Result<Json<CompleteResponse>, (StatusCode, Json<ApiError>)> {
    match tokio::spawn(complete_inner(state, account, req, trace_id)).await {
        Ok(result) => result.map(Json),
        Err(error) => {
            tracing::error!(error = %error, "detached managed completion task failed");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "managed completion task failed".into(),
                    reason: Some("managed_task_failed".into()),
                    ..Default::default()
                }),
            ))
        }
    }
}

/// Streaming variant of `/router/complete`.
///
/// This path performs the same entry checks/idempotency reservation as the
/// non-streaming endpoint, then proxies provider deltas from a detached worker.
/// The worker owns the provider stream, billing, usage recording, and
/// idempotency caching, so dropping the HTTP response body cannot cancel
/// settlement. A terminal `billing` event carries the final `CompleteResponse`.
pub async fn complete_stream(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Extension(crate::api::middleware::request_id::TraceId(trace_id)): Extension<
        crate::api::middleware::request_id::TraceId,
    >,
    Json(req): Json<CompleteRequest>,
) -> Result<Sse<RouterSseStream>, (StatusCode, Json<ApiError>)> {
    match tokio::spawn(complete_stream_inner(state, account, req, trace_id)).await {
        Ok(result) => result,
        Err(error) => {
            tracing::error!(error = %error, "detached managed streaming setup task failed");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "managed streaming task failed".into(),
                    reason: Some("managed_task_failed".into()),
                    ..Default::default()
                }),
            ))
        }
    }
}

async fn complete_stream_inner(
    state: AppState,
    account: Account,
    req: CompleteRequest,
    trace_id: String,
) -> Result<Sse<RouterSseStream>, (StatusCode, Json<ApiError>)> {
    let request_started = Instant::now();
    reconcile_expired_llm_usage(&state.pool, &account.id).map_err(|error| *error)?;
    if let Some(err) = billing_restricted_error(&account) {
        return Err(err);
    }
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
        streaming = true,
        image_count = req.image_data_urls.len(),
        "managed chat request accepted"
    );

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
                streaming = true,
                "managed chat idempotency reserved"
            );
        }
        idempotency::ReserveOutcome::CachedComplete(json) => {
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
                streaming = true,
                "managed chat idempotency replayed completed response"
            );
            let events = response_to_sse_events(cached);
            return Ok(router_sse(Box::pin(stream::iter(
                events.into_iter().map(Ok),
            ))));
        }
        idempotency::ReserveOutcome::InProgress => {
            tracing::warn!(
                account_id_hash = %account_id_hash,
                request_id = %req.request_id,
                session_id = %session_id_log,
                streaming = true,
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
                streaming = true,
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
            streaming = true,
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
            streaming = true,
            "behavioral story stopped before provider dispatch because verified facts were incomplete"
        );
        let events = response_to_sse_events(response);
        return Ok(router_sse(Box::pin(stream::iter(
            events.into_iter().map(Ok),
        ))));
    }
    let story_provider_user =
        behavioral_provider_user(&req, &preliminary_answer_plan, &story_grounding);
    check_account_llm_or_short_wait(&state, &account.id, &req.request_id, &session_ref_log, true)
        .await?;
    let should_lookup_memory = story_provider_user.is_none()
        && answer_plan_allows_memory_lookup(&preliminary_answer_plan)
        && should_lookup_completion_memory(&req, &requested_effective_lane);
    let memory_started = Instant::now();
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
    let memory_lookup_ms = memory_started.elapsed().as_millis() as i64;
    tracing::debug!(
        account_id_hash = %account_id_hash,
        request_id = %req.request_id,
        session_id = %session_id_log,
        memory_lookup = should_lookup_memory,
        memory_lookup_ms,
        rag_match_count = rag_matches.len(),
        streaming = true,
        "managed chat memory context prepared"
    );
    let answer_plan_started = Instant::now();
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
    let answer_plan_ms = answer_plan_started.elapsed().as_millis() as i64;
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
            streaming = true,
            "resolved behavioral story stopped before provider dispatch because user facts were incomplete"
        );
        let events = response_to_sse_events(response);
        return Ok(router_sse(Box::pin(stream::iter(
            events.into_iter().map(Ok),
        ))));
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
        answer_plan_ms,
        request_elapsed_ms = request_started.elapsed().as_millis() as i64,
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
    let web_search_started = Instant::now();
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
    let web_search_ms = web_search_started.elapsed().as_millis() as i64;
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
    let vision_text_fallback_lane = managed_vision_text_fallback_lane(&answer_plan);
    let (vision_text_fallback_system, vision_text_fallback_user) =
        managed_vision_text_fallback_prompt(&provider_system, &provider_user);
    let vision_text_fallback_possible =
        effective_lane == "vision" && !req.image_data_urls.is_empty();

    let thinking = routing::resolve_thinking_budget(
        &effective_lane,
        req.reasoning_effort.as_deref(),
        req.thinking_budget_tokens,
    );
    let vision_text_fallback_thinking = routing::resolve_thinking_budget(
        vision_text_fallback_lane,
        req.reasoning_effort.as_deref(),
        req.thinking_budget_tokens,
    );
    let has_thinking_budget = !matches!(thinking.mode, routing::ThinkingMode::Off);
    let vision_text_fallback_has_thinking_budget = !matches!(
        vision_text_fallback_thinking.mode,
        routing::ThinkingMode::Off
    );
    let first_output_deadline = first_token_deadline_for_lane(&effective_lane, has_thinking_budget);
    let vision_text_fallback_first_output_deadline = first_token_deadline_for_lane(
        vision_text_fallback_lane,
        vision_text_fallback_has_thinking_budget,
    );
    let stream_connect_deadline =
        stream_route_connect_deadline_for_lane(&effective_lane, has_thinking_budget);
    let vision_text_fallback_stream_connect_deadline = stream_route_connect_deadline_for_lane(
        vision_text_fallback_lane,
        vision_text_fallback_has_thinking_budget,
    );
    let stream_idle_deadline = stream_idle_deadline_for_lane(&effective_lane, has_thinking_budget);
    let slow_first_token_audit_ms =
        slow_first_token_audit_ms_for_lane(&effective_lane, has_thinking_budget);
    let vision_text_fallback_slow_first_token_audit_ms = slow_first_token_audit_ms_for_lane(
        vision_text_fallback_lane,
        vision_text_fallback_has_thinking_budget,
    );
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
    let pre_dispatch_ms = request_started.elapsed().as_millis() as i64;
    tracing::info!(
        account_id_hash = %account_id_hash,
        request_id = %req.request_id,
        request_ref = %request_ref_log,
        session_ref = %session_ref_log,
        effective_lane = %effective_lane_log,
        memory_lookup_ms,
        answer_plan_ms,
        web_search_ms,
        pre_dispatch_ms,
        system_chars = provider_system.chars().count(),
        user_chars = provider_user.chars().count(),
        estimated_input_tokens = est_in,
        first_token_deadline_ms = first_output_deadline.as_millis() as u64,
        route_connect_deadline_ms = stream_connect_deadline.as_millis() as u64,
        stream_idle_deadline_ms = stream_idle_deadline.as_millis() as u64,
        thinking = ?thinking.mode,
        "managed chat pre-dispatch phases completed"
    );
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
            "resolved streaming LLM route candidates"
        );
    }
    let vision_text_fallback_routes = if vision_text_fallback_possible {
        priced_routes_for(vision_text_fallback_lane, est_in, max_out, &req.request_id)
    } else {
        Vec::new()
    };

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

    let usage_reservation =
        reserve_llm_usage(&state, &account, &req.request_id, est_cost, 0, "llm_stream")
            .map_err(|error| *error)?;
    let on_trial = usage_reservation.is_trial();
    tracing::info!(
        account_id_hash = %account_id_hash,
        request_id = %req.request_id,
        attempt = usage_reservation.attempt,
        reserved_cents = usage_reservation.reserved_cents,
        reserved_trial_seconds = usage_reservation.reserved_trial_seconds,
        expires_at_ms = usage_reservation.expires_at_ms,
        streaming = true,
        "managed chat usage reserved before provider dispatch"
    );

    let started = Instant::now();
    let mut last_error: Option<anyhow::Error> = None;
    let mut last_capacity: Option<crate::rate_limit::CapacityDenied> = None;
    let mut last_failure_was_capacity = false;
    let mut selected_route_idx = 0usize;
    let mut selected_route: Option<PricedRoute> = None;
    let mut selected_stream: Option<routing::StreamingCompletion> = None;
    let mut selected_first_event: Option<anyhow::Result<routing::CompletionStreamEvent>> = None;
    let mut selected_attempt_guard: Option<Box<provider_cost_guard::ProviderCostGuard>> = None;
    let mut selected_stream_idle_deadline = stream_idle_deadline;
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
            selected_stream = None;
            selected_first_event = None;
            selected_attempt_guard = None;
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
            let dispatch_first_output_deadline = if vision_text_fallback_active {
                vision_text_fallback_first_output_deadline
            } else {
                first_output_deadline
            };
            let dispatch_stream_connect_deadline = if vision_text_fallback_active {
                vision_text_fallback_stream_connect_deadline
            } else {
                stream_connect_deadline
            };
            let dispatch_stream_idle_deadline = if vision_text_fallback_active {
                stream_idle_deadline_for_lane(
                    vision_text_fallback_lane,
                    vision_text_fallback_has_thinking_budget,
                )
            } else {
                stream_idle_deadline
            };
            let dispatch_slow_first_token_audit_ms = if vision_text_fallback_active {
                vision_text_fallback_slow_first_token_audit_ms
            } else {
                slow_first_token_audit_ms
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
                &format!(
                    "llm-stream:{}:{}:{}",
                    req.request_id, route.provider, route.model
                ),
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
                            "provider key pool cooling down; trying next streaming route"
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
                        "provider capacity busy; trying next streaming route"
                    );
                    last_capacity = Some(denied);
                    last_failure_was_capacity = true;
                    break;
                }

                let attempt_request_id = format!(
                    "{}:llm-stream-attempt:{}",
                    req.request_id, provider_dispatch_index
                );
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
                            "paid streaming LLM route unexpectedly had zero projected exposure"
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
                        last_error = Some(error.context(
                            "durable upstream spend admission failed for streaming LLM route",
                        ));
                        last_failure_was_capacity = false;
                        break;
                    }
                };
                let dispatch = routing::complete_stream_with_key(
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
                );

                match tokio::time::timeout(dispatch_stream_connect_deadline, dispatch).await {
                    Ok(Ok(streaming)) => {
                        // B2: a 2xx connection is not yet a usable stream. Only a
                        // non-empty text delta commits this route. A pre-output
                        // error, empty completion, or silent end falls back while
                        // Bluey still has other providers available.
                        let routing::StreamingCompletion {
                            provider: stream_provider,
                            model: stream_model,
                            events: mut stream_events,
                        } = streaming;
                        match tokio::time::timeout(
                            dispatch_first_output_deadline,
                            next_nonempty_completion_event(&mut stream_events),
                        )
                        .await
                        {
                            Ok(Some(Ok(routing::CompletionStreamEvent::Delta(delta)))) => {
                                if stream_provider != route.provider || stream_model != route.model
                                {
                                    let returned_cost = returned_route_bluey_cost_or_cap(
                                        &stream_provider,
                                        &stream_model,
                                        est_in,
                                        max_out,
                                    );
                                    let mismatch_event = UsageEvent {
                                        request_id: attempt_request_id,
                                        kind: "llm_attempt".into(),
                                        task_type: Some(dispatch_lane.to_string()),
                                        lane: Some(dispatch_lane.to_string()),
                                        provider: Some(stream_provider),
                                        model: Some(stream_model),
                                        input_tokens: est_in,
                                        output_tokens: max_out,
                                        latency_ms: started
                                            .elapsed()
                                            .as_millis()
                                            .try_into()
                                            .unwrap_or(i64::MAX),
                                        cost_cents_to_bluey: returned_cost,
                                        cost_cents_to_customer: 0,
                                        was_speculative: false,
                                        was_fallback: route_index > 0,
                                    };
                                    settle_provider_attempt_before_customer(
                                        &state.pool,
                                        &account.id,
                                        &req.request_id,
                                        &mut attempt_guard,
                                        mismatch_event,
                                        returned_cost,
                                        pricing::UsageProvenance::Missing,
                                    )?;
                                    last_error = Some(anyhow::anyhow!(
                                        "streaming provider route identity mismatch"
                                    ));
                                    last_failure_was_capacity = false;
                                    break;
                                }
                                selected_attempt_guard = Some(attempt_guard);
                                selected_route_idx = route_index;
                                selected_route = Some(*route);
                                selected_stream_idle_deadline = dispatch_stream_idle_deadline;
                                let first_event_latency_ms = started.elapsed().as_millis() as i64;
                                let request_to_first_event_ms =
                                    request_started.elapsed().as_millis() as i64;
                                let first_event_kind = "delta";
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
                                    first_event_latency_ms,
                                    request_to_first_event_ms,
                                    pre_dispatch_ms,
                                    first_event_kind,
                                    streaming = true,
                                    "managed chat route selected"
                                );
                                if request_to_first_event_ms >= dispatch_slow_first_token_audit_ms {
                                    record_answer_ops_event(
                                        &state.pool,
                                        AnswerOpsEvent {
                                            account_id: &account.id,
                                            request_id: &req.request_id,
                                            session_id: req.session_id.as_deref(),
                                            trace_id: Some(&trace_id),
                                            event_type: "answer_slow_first_token",
                                            status: "warning",
                                            metadata: serde_json::json!({
                                                "lane": lane_log.as_str(),
                                                "effective_lane": effective_lane_log.as_str(),
                                                "provider": route.provider,
                                                "model": route.model,
                                                "dispatch_lane": dispatch_lane,
                                                "vision_text_fallback": vision_text_fallback_active,
                                                "route_index": route_index,
                                                "was_fallback": route_index > 0,
                                                "first_event_latency_ms": first_event_latency_ms,
                                                "request_to_first_event_ms": request_to_first_event_ms,
                                                "pre_dispatch_ms": pre_dispatch_ms,
                                                "memory_lookup_ms": memory_lookup_ms,
                                                "answer_plan_ms": answer_plan_ms,
                                                "web_search_ms": web_search_ms,
                                                "system_chars": dispatch_system.chars().count(),
                                                "user_chars": dispatch_user.chars().count(),
                                                "estimated_input_tokens": est_in,
                                                "slow_threshold_ms": dispatch_slow_first_token_audit_ms,
                                                "first_event_kind": first_event_kind,
                                                "streaming": true
                                            }),
                                        },
                                    );
                                }
                                selected_first_event =
                                    Some(Ok(routing::CompletionStreamEvent::Delta(delta)));
                                selected_stream = Some(routing::StreamingCompletion {
                                    provider: stream_provider,
                                    model: stream_model,
                                    events: stream_events,
                                });
                                break;
                            }
                            Ok(Some(Ok(routing::CompletionStreamEvent::Done {
                                input_tokens,
                                output_tokens,
                                usage_provenance,
                            }))) => {
                                // Even an empty stream can carry an exact terminal
                                // provider usage frame. Prefer that truth over the
                                // projected fallback before trying another route.
                                let actual_bluey_cost = returned_route_bluey_cost_or_cap(
                                    &stream_provider,
                                    &stream_model,
                                    input_tokens,
                                    output_tokens,
                                );
                                let event = UsageEvent {
                                    request_id: attempt_request_id,
                                    kind: "llm_attempt".into(),
                                    task_type: Some(dispatch_lane.to_string()),
                                    lane: Some(dispatch_lane.to_string()),
                                    provider: Some(stream_provider),
                                    model: Some(stream_model),
                                    input_tokens,
                                    output_tokens,
                                    latency_ms: started
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
                                    event,
                                    actual_bluey_cost,
                                    usage_provenance,
                                )?;
                                tracing::warn!(
                                    account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                                    request_id = %req.request_id,
                                    provider = %route.provider,
                                    model = %route.model,
                                    "streaming route completed before producing output; trying next route"
                                );
                                last_error = Some(anyhow::anyhow!(
                                    "streaming route completed before producing output"
                                ));
                                last_failure_was_capacity = false;
                                break;
                            }
                            Ok(Some(Err(e))) => {
                                if let Err(error) = attempt_guard.settle_conservative() {
                                    tracing::error!(request_id = %req.request_id, error = %error, "pre-output stream failure settlement pending reconciliation");
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
                                        "managed vision stream explicitly rejected media; trying remaining vision routes"
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
                                        "streaming route failed before output; cooled key and retrying route"
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
                                    "streaming route failed before output; trying next route"
                                );
                                last_error = Some(e);
                                last_failure_was_capacity = false;
                                break;
                            }
                            Ok(None) => {
                                if let Err(error) = attempt_guard.settle_conservative() {
                                    tracing::error!(request_id = %req.request_id, error = %error, "ended stream attempt settlement pending reconciliation");
                                    return Err(provider_accounting_pending_error(
                                        &state.pool,
                                        &account.id,
                                        &req.request_id,
                                    ));
                                }
                                tracing::warn!(
                                    account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                                    request_id = %req.request_id,
                                    provider = %route.provider,
                                    model = %route.model,
                                    "streaming route ended before producing output; trying next route"
                                );
                                last_error = Some(anyhow::anyhow!(
                                    "streaming route ended before producing output"
                                ));
                                last_failure_was_capacity = false;
                                break;
                            }
                            Err(_elapsed) => {
                                if let Err(error) = attempt_guard.settle_conservative() {
                                    tracing::error!(request_id = %req.request_id, error = %error, "first-token timeout settlement pending reconciliation");
                                    return Err(provider_accounting_pending_error(
                                        &state.pool,
                                        &account.id,
                                        &req.request_id,
                                    ));
                                }
                                tracing::warn!(
                                    account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                                    request_id = %req.request_id,
                                    provider = %route.provider,
                                    model = %route.model,
                                    first_token_timeout_ms = dispatch_first_output_deadline.as_millis() as u64,
                                    "streaming first-token deadline exceeded; trying next route"
                                );
                                last_error = Some(anyhow::anyhow!("first-token deadline exceeded"));
                                last_failure_was_capacity = false;
                                break;
                            }
                        }
                    }
                    Ok(Err(e)) => {
                        if let Err(error) = attempt_guard.settle_conservative() {
                            tracing::error!(request_id = %req.request_id, error = %error, "stream connect failure settlement pending reconciliation");
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
                                "streaming upstream capacity response; cooled key and retrying route"
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
                            "streaming upstream dispatch failed; trying next route"
                        );
                        last_error = Some(e);
                        last_failure_was_capacity = false;
                        break;
                    }
                    Err(_elapsed) => {
                        if let Err(error) = attempt_guard.settle_conservative() {
                            tracing::error!(request_id = %req.request_id, error = %error, "stream connect-timeout settlement pending reconciliation");
                            return Err(provider_accounting_pending_error(
                                &state.pool,
                                &account.id,
                                &req.request_id,
                            ));
                        }
                        tracing::warn!(
                            account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                            request_id = %req.request_id,
                            provider = %route.provider,
                            model = %route.model,
                            route_connect_timeout_ms = dispatch_stream_connect_deadline.as_millis() as u64,
                            "streaming route connect deadline exceeded; trying next route"
                        );
                        last_error =
                            Some(anyhow::anyhow!("streaming route connect deadline exceeded"));
                        last_failure_was_capacity = false;
                        break;
                    }
                }
            }

            if selected_stream.is_some() {
                break;
            }
        }

        if selected_stream.is_some() {
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
                "all streaming routes briefly capacity busy; waiting before internal retry sweep"
            );
            tokio::time::sleep(denied).await;
            continue;
        }
        break;
    }

    let (selected_route, streaming) = match (selected_route, selected_stream) {
        (Some(route), Some(streaming)) => (route, streaming),
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
                    streaming = true,
                    "all streaming routes still capacity-busy after fallback scan"
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
                        "streaming": true,
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
                tracing::warn!(
                    account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                    request_id = %req.request_id,
                    request_ref = %request_ref_log,
                    session_ref = %session_ref_log,
                    error = %e,
                    "all streaming upstream dispatch routes failed"
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
                        "streaming": true,
                        "error_kind": "all_streaming_routes_failed",
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
    let stream_idle_deadline = selected_stream_idle_deadline;

    let stream_status_events =
        retrieval_status_events(&answer_plan, rag_matches.len(), &web_search);
    let stream_sources = web_sources.clone();
    // Canvas output contains a compact spoken section followed by durable
    // workbench detail. Stream the spoken section line-by-line while keeping
    // the diagram/body out of the overlay. Strict first-principles LRU code
    // is the narrow exception: its implementation stays private until the
    // completed response has passed the contract gate.
    let split_canvas_stream = answer_plan.output == AnswerOutput::CanvasDetail
        && answer_plan.intent == AnswerIntent::SystemDesign;
    let strict_lru_code_stream =
        requires_first_principles_lru_code(&answer_plan, &normalized_route_question);
    let strip_interview_coaching_appendix =
        should_strip_unsolicited_coaching_appendix(&answer_plan, &req.user);
    let evidence_bound_role_reference = interview_contracts::evidence_bound_role_reference(
        &normalize_guardrail_text(&extract_search_question(&req.user)),
        &req.context,
        &answer_plan,
    );
    let event_stream = async_stream::stream! {
        let mut events = streaming.events;
        let mut pending_first = selected_first_event;
        let mut output = BufferedDisclosureOutput::new(strip_interview_coaching_appendix);
        let mut provider_quality_output =
            BufferedDisclosureOutput::new(strip_interview_coaching_appendix);
        let mut role_anchor = interview_contracts::EvidenceBoundRoleAnchor::new(
            evidence_bound_role_reference,
        );
        let mut canvas_visible = CanvasSpokenStream::default();
        let mut strict_lru_gate = StrictLruCodeStreamGate::new(strict_lru_code_stream);
        let mut final_tokens: Option<(i64, i64)> = None;

        for status_event in stream_status_events {
            yield Ok(status_event);
        }
        if let Some(source_event) = sources_sse_event(&stream_sources) {
            yield Ok(source_event);
        }
        loop {
            // B2: replay the event prefetched during the first-token deadline
            // check before resuming the live stream.
            let event = match pending_first.take() {
                Some(first) => Some(first),
                None => match tokio::time::timeout(stream_idle_deadline, events.next()).await {
                    Ok(event) => event,
                    Err(_) => {
                        let already_delivered = if split_canvas_stream {
                            canvas_visible.has_delivered()
                        } else {
                            strict_lru_gate.has_delivered()
                        };
                        let partial =
                            flush_interrupted_role_anchor(&mut role_anchor, &mut output);
                        let partial_chars = output.char_count();
                        let mut visible_partial = if split_canvas_stream {
                            canvas_visible.push(&partial)
                        } else {
                            strict_lru_gate.push(&partial)
                        };
                        if !split_canvas_stream {
                            append_visible_delta(
                                &mut visible_partial,
                                strict_lru_gate.release_after_failure(),
                            );
                        }
                        let delivered_delta = already_delivered
                            || visible_partial
                                .as_deref()
                                .is_some_and(|value| !value.trim().is_empty());
                        if let Err(error) = settle_selected_provider_attempt_conservative(
                            &mut selected_attempt_guard,
                        ) {
                            tracing::error!(request_id = %req.request_id, error = %error, "stream idle exposure settlement pending reconciliation");
                            let _ = provider_accounting_pending_error(
                                &state.pool,
                                &account.id,
                                &req.request_id,
                            );
                            yield Ok(Event::default().event("error").data(
                                serde_json::json!({
                                    "error": "provider accounting is pending reconciliation",
                                    "reason": "provider_accounting_pending",
                                }).to_string(),
                            ));
                            return;
                        }
                        fail_stream_llm_usage(
                            &state.pool,
                            &account.id,
                            &req.request_id,
                            delivered_delta,
                            "stream_idle_timeout",
                        );
                        tracing::warn!(
                            account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                            request_id = %req.request_id,
                            request_ref = %request_ref_log,
                            session_ref = %session_ref_log,
                            provider = %streaming.provider,
                            model = %streaming.model,
                            delivered_delta,
                            stream_idle_timeout_ms = stream_idle_deadline.as_millis() as u64,
                            "streaming provider stalled between output events"
                        );
                        record_answer_ops_event(
                            &state.pool,
                            AnswerOpsEvent {
                                account_id: &account.id,
                                request_id: &req.request_id,
                                session_id: req.session_id.as_deref(),
                                trace_id: Some(&trace_id),
                                event_type: "answer_failed",
                                status: "upstream_stream_idle_timeout",
                                metadata: serde_json::json!({
                                    "lane": lane_log.as_str(),
                                    "effective_lane": effective_lane_log.as_str(),
                                    "provider": streaming.provider.as_str(),
                                    "model": streaming.model.as_str(),
                                    "streaming": true,
                                    "delivered_delta": delivered_delta,
                                    "stream_idle_timeout_ms": stream_idle_deadline.as_millis() as u64,
                                    "partial_chars": partial_chars
                                }),
                            },
                        );
                        if let Some(visible_partial) = visible_partial {
                            yield Ok(completion_delta_event(&visible_partial));
                        }
                        yield Ok(Event::default().event("error").data(
                            serde_json::json!({
                                "error": "upstream provider stopped responding; please retry",
                                "reason": "upstream_stream_idle_timeout",
                            })
                            .to_string(),
                        ));
                        return;
                    }
                },
            };
            let Some(event) = event else { break };
            if let Some(payload) = live_account_error_payload(live_account_state(&state.pool, &account.id)) {
                if let Err(error) = settle_selected_provider_attempt_conservative(
                    &mut selected_attempt_guard,
                ) {
                    tracing::error!(request_id = %req.request_id, error = %error, "inactive-account stream exposure settlement pending reconciliation");
                    let _ = provider_accounting_pending_error(
                        &state.pool,
                        &account.id,
                        &req.request_id,
                    );
                    yield Ok(Event::default().event("error").data(
                        serde_json::json!({
                            "error": "provider accounting is pending reconciliation",
                            "reason": "provider_accounting_pending",
                        }).to_string(),
                    ));
                    return;
                }
                let _ = idempotency::mark_failed(&state.pool, &account.id, &req.request_id);
                release_llm_usage(
                    &state.pool,
                    &account.id,
                    &req.request_id,
                    "account_inactive",
                );
                tracing::warn!(
                    account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                    request_id = %req.request_id,
                    streaming = true,
                    "streaming answer stopped because account is no longer active"
                );
                yield Ok(Event::default().event("error").data(payload.to_string()));
                return;
            }
            match event {
                Ok(routing::CompletionStreamEvent::Delta(delta)) => {
                    let _ = provider_quality_output.push(&delta);
                    let Some(presentation_delta) = role_anchor.push(&delta) else {
                        continue;
                    };
                    if let Some(safe_delta) = output.push(&presentation_delta) {
                        let visible_delta = if split_canvas_stream {
                            canvas_visible.push(&safe_delta)
                        } else {
                            strict_lru_gate.push(&safe_delta)
                        };
                        if let Some(visible_delta) = visible_delta {
                            yield Ok(completion_delta_event(&visible_delta));
                        }
                    }
                }
                Ok(routing::CompletionStreamEvent::Done {
                    input_tokens,
                    output_tokens,
                    usage_provenance,
                }) => {
                    // Persist provider-attempt accounting immediately, before
                    // any Bluey quality/account gate can settle the customer
                    // root. Estimated or missing usage retains the projection.
                    let (exact_bluey_cost, _) = pricing::compute_cost(
                        &selected_route.pricing,
                        input_tokens,
                        output_tokens,
                    );
                    let mut attempt_guard = match take_selected_provider_attempt_guard(
                        &mut selected_attempt_guard,
                    ) {
                        Ok(guard) => guard,
                        Err(error) => {
                        tracing::error!(
                            request_id = %req.request_id,
                            error = %error,
                            "selected streaming completion reached Done without an armed provider guard"
                        );
                        let (_, Json(payload)) = provider_accounting_pending_error(
                            &state.pool,
                            &account.id,
                            &req.request_id,
                        );
                        yield Ok(Event::default().event("error").data(
                            serde_json::to_string(&payload).unwrap_or_else(|_| {
                                r#"{"error":"provider accounting pending","reason":"provider_accounting_pending"}"#.to_string()
                            }),
                        ));
                        return;
                        }
                    };
                    let attempt_event = UsageEvent {
                        request_id: req.request_id.clone(),
                        kind: "llm_attempt".into(),
                        task_type: Some(effective_lane.clone()),
                        lane: Some(effective_lane.clone()),
                        provider: Some(streaming.provider.clone()),
                        model: Some(streaming.model.clone()),
                        input_tokens,
                        output_tokens,
                        latency_ms: started
                            .elapsed()
                            .as_millis()
                            .try_into()
                            .unwrap_or(i64::MAX),
                        cost_cents_to_bluey: exact_bluey_cost,
                        cost_cents_to_customer: 0,
                        was_speculative: false,
                        was_fallback: selected_route_idx > 0,
                    };
                    if let Err((_, Json(payload))) = settle_provider_attempt_before_customer(
                        &state.pool,
                        &account.id,
                        &req.request_id,
                        &mut attempt_guard,
                        attempt_event,
                        exact_bluey_cost,
                        usage_provenance,
                    ) {
                        yield Ok(Event::default().event("error").data(
                            serde_json::to_string(&payload).unwrap_or_else(|_| {
                                r#"{"error":"provider accounting pending","reason":"provider_accounting_pending"}"#.to_string()
                            }),
                        ));
                        return;
                    }
                    final_tokens = Some((input_tokens, output_tokens));
                    break;
                }
                Err(e) => {
                    let already_delivered = if split_canvas_stream {
                        canvas_visible.has_delivered()
                    } else {
                        strict_lru_gate.has_delivered()
                    };
                    let partial = flush_interrupted_role_anchor(&mut role_anchor, &mut output);
                    let partial_chars = output.char_count();
                    let mut visible_partial = if split_canvas_stream {
                        canvas_visible.push(&partial)
                    } else {
                        strict_lru_gate.push(&partial)
                    };
                    if !split_canvas_stream {
                        append_visible_delta(
                            &mut visible_partial,
                            strict_lru_gate.release_after_failure(),
                        );
                    }
                    let delivered_delta = already_delivered
                        || visible_partial
                            .as_deref()
                            .is_some_and(|value| !value.trim().is_empty());
                    let failure_reason = upstream_stream_failure_reason(&e);
                    if let Err(error) = settle_selected_provider_attempt_conservative(
                        &mut selected_attempt_guard,
                    ) {
                        tracing::error!(request_id = %req.request_id, error = %error, "stream read-failure exposure settlement pending reconciliation");
                        let _ = provider_accounting_pending_error(
                            &state.pool,
                            &account.id,
                            &req.request_id,
                        );
                        yield Ok(Event::default().event("error").data(
                            serde_json::json!({
                                "error": "provider accounting is pending reconciliation",
                                "reason": "provider_accounting_pending",
                            }).to_string(),
                        ));
                        return;
                    }
                    fail_stream_llm_usage(
                        &state.pool,
                        &account.id,
                        &req.request_id,
                        delivered_delta,
                        failure_reason,
                    );
                    let retry_after_secs = routing::upstream_retry_after(&e);
                    let terminal_reason = routing::upstream_terminal_reason(&e);
                    tracing::warn!(
                        account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                        request_id = %req.request_id,
                        request_ref = %request_ref_log,
                        session_ref = %session_ref_log,
                        error = %e,
                        delivered_delta,
                        retry_after_secs = retry_after_secs.unwrap_or_default(),
                        "streaming upstream read failed"
                    );
                    record_answer_ops_event(
                        &state.pool,
                        AnswerOpsEvent {
                            account_id: &account.id,
                            request_id: &req.request_id,
                            session_id: req.session_id.as_deref(),
                            trace_id: Some(&trace_id),
                            event_type: "answer_failed",
                            status: if retry_after_secs.is_some() {
                                "provider_capacity"
                            } else {
                                failure_reason
                            },
                            metadata: serde_json::json!({
                                "lane": lane_log.as_str(),
                                "effective_lane": effective_lane_log.as_str(),
                                "provider": streaming.provider.as_str(),
                                "model": streaming.model.as_str(),
                                "streaming": true,
                                "delivered_delta": delivered_delta,
                                "partial_chars": partial_chars,
                                "retry_after_secs": retry_after_secs,
                                "terminal_reason": terminal_reason,
                                "error_preview": truncate_chars(&e.to_string(), 180)
                            }),
                        },
                    );
                    if let Some(visible_partial) = visible_partial {
                        yield Ok(completion_delta_event(&visible_partial));
                    }
                    let payload = if let Some(retry_after_secs) = retry_after_secs {
                        serde_json::json!({
                            "error": "Bluey is handling a burst right now; retry shortly",
                            "reason": "provider_key_cooling_down",
                            "retry_after_secs": retry_after_secs.max(1),
                        })
                    } else if failure_reason == "upstream_output_truncated" {
                        serde_json::json!({
                            "error": "upstream answer reached its output limit; retry for a shorter answer",
                            "reason": failure_reason,
                        })
                    } else if failure_reason == "upstream_output_blocked" {
                        serde_json::json!({
                            "error": "upstream provider could not complete this answer",
                            "reason": failure_reason,
                        })
                    } else {
                        serde_json::json!({
                            "error": "upstream provider stream interrupted; please retry",
                            "reason": failure_reason,
                        })
                    };
                    yield Ok(Event::default().event("error").data(payload.to_string()));
                    return;
                }
            }
        }

        let Some((input_tokens, output_tokens)) = final_tokens else {
            let already_delivered = if split_canvas_stream {
                canvas_visible.has_delivered()
            } else {
                strict_lru_gate.has_delivered()
            };
            let partial = flush_interrupted_role_anchor(&mut role_anchor, &mut output);
            let partial_chars = output.char_count();
            let mut visible_partial = if split_canvas_stream {
                canvas_visible.push(&partial)
            } else {
                strict_lru_gate.push(&partial)
            };
            if !split_canvas_stream {
                append_visible_delta(
                    &mut visible_partial,
                    strict_lru_gate.release_after_failure(),
                );
            }
            let delivered_delta = already_delivered
                || visible_partial
                    .as_deref()
                    .is_some_and(|value| !value.trim().is_empty());
            if let Err(error) = settle_selected_provider_attempt_conservative(
                &mut selected_attempt_guard,
            ) {
                tracing::error!(request_id = %req.request_id, error = %error, "incomplete stream exposure settlement pending reconciliation");
                let _ = provider_accounting_pending_error(
                    &state.pool,
                    &account.id,
                    &req.request_id,
                );
                yield Ok(Event::default().event("error").data(
                    serde_json::json!({
                        "error": "provider accounting is pending reconciliation",
                        "reason": "provider_accounting_pending",
                    }).to_string(),
                ));
                return;
            }
            fail_stream_llm_usage(
                &state.pool,
                &account.id,
                &req.request_id,
                delivered_delta,
                "upstream_stream_incomplete",
            );
            tracing::warn!(
                account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                request_id = %req.request_id,
                request_ref = %request_ref_log,
                session_ref = %session_ref_log,
                delivered_delta,
                "streaming provider ended without a terminal billing event"
            );
            record_answer_ops_event(
                &state.pool,
                AnswerOpsEvent {
                    account_id: &account.id,
                    request_id: &req.request_id,
                    session_id: req.session_id.as_deref(),
                    trace_id: Some(&trace_id),
                    event_type: "answer_failed",
                    status: "upstream_stream_incomplete",
                    metadata: serde_json::json!({
                    "lane": lane_log.as_str(),
                    "effective_lane": effective_lane_log.as_str(),
                    "provider": streaming.provider.as_str(),
                    "model": streaming.model.as_str(),
                    "streaming": true,
                    "delivered_delta": delivered_delta,
                    "partial_chars": partial_chars
                    }),
                },
            );
            if let Some(visible_partial) = visible_partial {
                yield Ok(completion_delta_event(&visible_partial));
            }
            yield Ok(Event::default().event("error").data(
                serde_json::json!({
                    "error": "upstream provider stream ended before completion; please retry",
                    "reason": "upstream_stream_incomplete",
                })
                .to_string(),
            ));
            return;
        };
        if let Some(payload) = live_account_error_payload(live_account_state(&state.pool, &account.id)) {
            let _ = idempotency::mark_failed(&state.pool, &account.id, &req.request_id);
            release_llm_usage(
                &state.pool,
                &account.id,
                &req.request_id,
                "account_inactive",
            );
            tracing::warn!(
                account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                request_id = %req.request_id,
                streaming = true,
                "streaming answer stopped before billing because account is no longer active"
            );
            yield Ok(Event::default().event("error").data(payload.to_string()));
            return;
        }
        if let Some(raw_opening) = role_anchor.finish() {
            if let Some(safe_delta) = output.push(&raw_opening) {
                let visible_delta = if split_canvas_stream {
                    canvas_visible.push(&safe_delta)
                } else {
                    strict_lru_gate.push(&safe_delta)
                };
                if let Some(visible_delta) = visible_delta {
                    yield Ok(completion_delta_event(&visible_delta));
                }
            }
        }
        let (provider_quality_text, _) = provider_quality_output.finish();
        let already_delivered = if split_canvas_stream {
            canvas_visible.has_delivered()
        } else {
            strict_lru_gate.has_delivered()
        };
        let (text, final_delta) = output.finish();
        let mut visible_final_delta = if split_canvas_stream {
            canvas_visible.push(&final_delta)
        } else {
            strict_lru_gate.push(&final_delta)
        };
        if let Some(reason) = generated_answer_quality_failure(
            &provider_quality_text,
            output_tokens,
            Some(quality_max_tokens),
            &answer_plan,
            &normalized_route_question,
        ) {
            if !split_canvas_stream {
                append_visible_delta(
                    &mut visible_final_delta,
                    strict_lru_gate.release_after_failure(),
                );
            }
            if let Some(visible_final_delta) = visible_final_delta.as_deref() {
                yield Ok(completion_delta_event(visible_final_delta));
            }
            let delivered_delta = already_delivered
                || visible_final_delta
                    .as_deref()
                    .is_some_and(|value| !value.trim().is_empty());
            fail_stream_llm_usage(
                &state.pool,
                &account.id,
                &req.request_id,
                delivered_delta,
                reason,
            );
            tracing::warn!(
                account_id_hash = %account_id_hash,
                request_id = %req.request_id,
                provider = %streaming.provider,
                model = %streaming.model,
                output_tokens,
                max_tokens = quality_max_tokens,
                reason,
                streaming = true,
                "provider returned an incomplete answer"
            );
            record_answer_ops_event(
                &state.pool,
                AnswerOpsEvent {
                    account_id: &account.id,
                    request_id: &req.request_id,
                    session_id: req.session_id.as_deref(),
                    trace_id: Some(&trace_id),
                    event_type: "answer_failed",
                    status: reason,
                    metadata: serde_json::json!({
                        "lane": lane_log.as_str(),
                        "effective_lane": effective_lane_log.as_str(),
                        "provider": streaming.provider.as_str(),
                        "model": streaming.model.as_str(),
                        "streaming": true,
                        "delivered_delta": delivered_delta,
                        "output_tokens": output_tokens,
                        "max_tokens": quality_max_tokens,
                        "text_chars": text.chars().count()
                    }),
                },
            );
            let error_message = if reason == "upstream_code_contract_failed" {
                "upstream provider code violated the requested implementation contract; please retry"
            } else {
                "upstream provider returned an incomplete answer; please retry"
            };
            yield Ok(Event::default().event("error").data(
                serde_json::json!({
                    "error": error_message,
                    "reason": reason,
                })
                .to_string(),
            ));
            return;
        }
        if let Some(visible_final_delta) = visible_final_delta {
            yield Ok(completion_delta_event(&visible_final_delta));
        }
        if let Some(held_code) = strict_lru_gate.release_after_quality_pass() {
            yield Ok(completion_delta_event(&held_code));
        }
        let artifact = response_artifact_for_plan(&text, &answer_plan);
        if code_artifact_missing_for_plan(&answer_plan, artifact.as_ref()) {
            tracing::warn!(
                account_id_hash = %account_id_hash,
                request_id = %req.request_id,
                request_ref = %request_ref_log,
                session_id = %session_id_log,
                session_ref = %session_ref_log,
                lane = %lane_log,
                effective_lane = %effective_lane_log,
                provider = %streaming.provider,
                model = %streaming.model,
                answer_intent = %answer_plan.intent.as_str(),
                answer_output = %answer_plan.output.as_str(),
                text_chars = text.chars().count(),
                streaming = true,
                "code artifact expected but missing after streaming; preserving streamed answer"
            );
            record_answer_ops_event(
                &state.pool,
                AnswerOpsEvent {
                    account_id: &account.id,
                    request_id: &req.request_id,
                    session_id: req.session_id.as_deref(),
                    trace_id: Some(&trace_id),
                    event_type: "answer_warning",
                    status: "code_artifact_missing_stream_preserved",
                    metadata: serde_json::json!({
                    "lane": lane_log.as_str(),
                    "effective_lane": effective_lane_log.as_str(),
                    "provider": streaming.provider.as_str(),
                    "model": streaming.model.as_str(),
                    "streaming": true,
                    "answer_intent": answer_plan.intent.as_str(),
                    "answer_output": answer_plan.output.as_str(),
                    "text_chars": text.chars().count(),
                    "question_hash": request_diag.question_hash,
                    "context_hash": request_diag.context_hash,
                    "context_coding_signal": request_diag.context_coding_signal
                    }),
                },
            );
        }
        let elapsed_ms = started.elapsed().as_millis() as i64;
        let (llm_bluey_cost, llm_customer_cost) = pricing::compute_cost(
            &selected_route.pricing,
            input_tokens,
            output_tokens,
        );
        let bluey_cost = llm_bluey_cost.saturating_add(web_search.bluey_cost_cents);
        let customer_cost = llm_customer_cost.saturating_add(web_search.customer_cost_cents);
        let event = UsageEvent {
            request_id: req.request_id.clone(),
            kind: "llm".into(),
            task_type: None,
            lane: Some(effective_lane.clone()),
            provider: Some(streaming.provider.clone()),
            model: Some(streaming.model.clone()),
            input_tokens,
            output_tokens,
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

        let settled_usage = match settle_llm_usage_with_retry(
            &state.pool,
            &account.id,
            &req.request_id,
            customer_cost,
            elapsed_ms,
            "completed",
            &settlement_events,
        )
        .await
        {
            Ok(settled) => settled,
            Err(error) => {
                tracing::error!(
                    account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                    request_id = %req.request_id,
                    error = %error,
                    "streaming managed usage settlement failed after provider completion"
                );
                yield Ok(Event::default().event("error").data(
                    serde_json::json!({
                        "error": "usage settlement is pending reconciliation",
                        "reason": "usage_settlement_pending",
                    })
                    .to_string(),
                ));
                return;
            }
        };
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
            provider = %streaming.provider,
            model = %streaming.model,
            cost_cents = charged_llm_customer_cost,
            web_search_cost_cents = charged_web_search_customer_cost,
            balance_cents_after = balance_after,
            latency_ms = elapsed_ms,
            streaming = true,
            "managed chat usage settled with all authoritative events"
        );

        let artifact_type = artifact.as_ref().map(|artifact| artifact.artifact_type).unwrap_or("none");
        let artifact_confidence = artifact.as_ref().map(|artifact| artifact.confidence).unwrap_or(0.0);
        let web_search_skipped_reason = web_search.skipped_reason.unwrap_or("none");

        tracing::info!(
            account_id_hash = %account_id_hash,
            request_id = %req.request_id,
            request_ref = %request_ref_log,
            session_id = %session_id_log,
            session_ref = %session_ref_log,
            lane = %lane_log,
            effective_lane = %effective_lane_log,
            provider = %streaming.provider,
            model = %streaming.model,
            input_tokens,
            output_tokens,
            cost_cents = charged_customer_cost,
            bluey_cost_cents = bluey_cost,
            llm_cost_cents = charged_llm_customer_cost,
            web_search_cost_cents = web_search.customer_cost_cents,
            balance_cents_after = balance_after,
            trial_seconds_remaining = trial_remaining,
            latency_ms = elapsed_ms,
            total_latency_ms = request_started.elapsed().as_millis() as i64,
            pre_dispatch_ms,
            memory_lookup_ms,
            answer_plan_ms,
            web_search_ms,
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
            streaming = true,
            "managed chat completed and billed"
        );

        let response_text =
            visible_response_text_for_plan(&text, artifact.as_ref(), &answer_plan);
        let response = CompleteResponse {
            text: response_text,
            provider: streaming.provider,
            model: streaming.model,
            input_tokens,
            output_tokens,
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
            sources: stream_sources.clone(),
        };

        match serde_json::to_string(&response) {
            Ok(json) => {
                if let Err(e) =
                    idempotency::mark_complete(&state.pool, &account.id, &req.request_id, &json)
                {
                    tracing::error!(
                        account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                        request_id = %req.request_id,
                        error = %e,
                        "streaming idempotency::mark_complete failed AFTER customer billed; retry will return 409 — manual reconciliation required"
                    );
                }
            }
            Err(e) => {
                tracing::error!(
                    account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                    request_id = %req.request_id,
                    error = %e,
                    "failed to serialize streaming response for idempotency cache; retry will return 409 — manual reconciliation required"
                );
            }
        }

        if split_canvas_stream {
            let visible_tail = canvas_visible.finish(&canvas_overlay_text(&text));
            if !visible_tail.trim().is_empty() {
                yield Ok(completion_delta_event(&visible_tail));
            }
        }

        let billing = serde_json::to_string(&response).unwrap_or_else(|_| "{}".to_string());
        yield Ok(Event::default().event("billing").data(billing));
        yield Ok(Event::default().data("[DONE]"));
    };

    Ok(router_sse(detach_router_stream(Box::pin(event_stream))))
}

pub(crate) async fn complete_for_account(
    state: AppState,
    account: Account,
    req: CompleteRequest,
    trace_id: String,
) -> Result<CompleteResponse, (StatusCode, Json<ApiError>)> {
    complete_inner(state, account, req, trace_id).await
}
