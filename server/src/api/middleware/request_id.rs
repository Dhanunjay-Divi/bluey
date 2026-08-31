//! Request/trace id middleware for Bluey's HTTP boundary.

use std::time::Instant;

use axum::{
    body::Body,
    extract::{MatchedPath, Request},
    http::HeaderValue,
    middleware::Next,
    response::Response,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestId(pub String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceId(pub String);

/// Optional UI-originated interaction UUID for one ask/render lifecycle.
/// The server never mints this value because non-UI and legacy callers do not
/// have a visible interaction to correlate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InteractionId(pub Option<String>);

pub async fn request_id_middleware(mut req: Request<Body>, next: Next) -> Response {
    let request_id = header_id(req.headers(), cue_core::BLUEY_REQUEST_ID_HEADER)
        .unwrap_or_else(cue_core::new_request_id);
    let trace_id = header_id(req.headers(), cue_core::BLUEY_TRACE_ID_HEADER)
        .unwrap_or_else(cue_core::new_trace_id);
    let interaction_id = req
        .headers()
        .get(cue_core::BLUEY_INTERACTION_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .and_then(cue_core::sanitize_interaction_id);
    match interaction_id.as_deref() {
        Some(interaction_id) => insert_header(
            req.headers_mut(),
            cue_core::BLUEY_INTERACTION_ID_HEADER,
            interaction_id,
        ),
        None => {
            req.headers_mut()
                .remove(cue_core::BLUEY_INTERACTION_ID_HEADER);
        }
    }
    let method = req.method().clone();
    // Log the router template, never the concrete URI. Session UUIDs, bundle
    // identifiers, connect codes, and other user/account identifiers can be
    // present in path segments even when the query string is absent.
    let route = request_route_label(&req);
    let started = Instant::now();

    req.extensions_mut().insert(RequestId(request_id.clone()));
    req.extensions_mut().insert(TraceId(trace_id.clone()));
    req.extensions_mut()
        .insert(InteractionId(interaction_id.clone()));

    tracing::info!(
        component = "bluey-server",
        version = env!("CARGO_PKG_VERSION"),
        platform = %cue_core::platform(),
        request_id = %request_id,
        trace_id = %trace_id,
        interaction_id = %interaction_id.as_deref().unwrap_or(""),
        method = %method,
        route = %route,
        "request received"
    );

    let mut response = next.run(req).await;
    insert_header(
        response.headers_mut(),
        cue_core::BLUEY_REQUEST_ID_HEADER,
        &request_id,
    );
    insert_header(
        response.headers_mut(),
        cue_core::BLUEY_TRACE_ID_HEADER,
        &trace_id,
    );
    if let Some(interaction_id) = interaction_id.as_deref() {
        insert_header(
            response.headers_mut(),
            cue_core::BLUEY_INTERACTION_ID_HEADER,
            interaction_id,
        );
    }

    tracing::info!(
        component = "bluey-server",
        version = env!("CARGO_PKG_VERSION"),
        platform = %cue_core::platform(),
        request_id = %request_id,
        trace_id = %trace_id,
        interaction_id = %interaction_id.as_deref().unwrap_or(""),
        method = %method,
        route = %route,
        status = response.status().as_u16(),
        latency_ms = started.elapsed().as_millis() as u64,
        "request done"
    );

    response
}

fn request_route_label(req: &Request<Body>) -> String {
    req.extensions()
        .get::<MatchedPath>()
        .map(MatchedPath::as_str)
        .filter(|route| {
            !route.is_empty()
                && route.len() <= 160
                && route.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric()
                        || matches!(byte, b'/' | b'_' | b'-' | b'.' | b':' | b'{' | b'}' | b'*')
                })
        })
        .unwrap_or("/unmatched")
        .to_string()
}

fn header_id(headers: &axum::http::HeaderMap, name: &'static str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .and_then(cue_core::sanitize_interaction_id)
}

fn insert_header(headers: &mut axum::http::HeaderMap, name: &'static str, value: &str) {
    if let Ok(value) = HeaderValue::from_str(value) {
        headers.insert(name, value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{extract::Extension, routing::get, Router};
    use tower::ServiceExt;

    #[test]
    fn concrete_request_path_is_never_used_as_log_route() {
        let request = Request::builder()
            .uri("/v1/support-diagnostics/private-session/private-bundle")
            .body(Body::empty())
            .unwrap();

        assert_eq!(request_route_label(&request), "/unmatched");
    }

    async fn echo_ids(
        Extension(request_id): Extension<RequestId>,
        Extension(trace_id): Extension<TraceId>,
        Extension(interaction_id): Extension<InteractionId>,
    ) -> String {
        format!(
            "{}|{}|{}",
            request_id.0,
            trace_id.0,
            interaction_id.0.as_deref().unwrap_or("")
        )
    }

    #[tokio::test]
    async fn echoes_incoming_uuid_trace_and_request_headers() {
        const REQUEST_ID: &str = "d0c8c639-b7d8-44f3-bce7-f4df0ef98b63";
        const TRACE_ID: &str = "a6e0b5d7-a0fb-44f8-a58c-a4e33297c678";
        let app = Router::new()
            .route("/ok", get(echo_ids))
            .layer(axum::middleware::from_fn(request_id_middleware));

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/ok")
                    .header(cue_core::BLUEY_REQUEST_ID_HEADER, REQUEST_ID)
                    .header(cue_core::BLUEY_TRACE_ID_HEADER, TRACE_ID)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            response
                .headers()
                .get(cue_core::BLUEY_REQUEST_ID_HEADER)
                .unwrap(),
            REQUEST_ID
        );
        assert_eq!(
            response
                .headers()
                .get(cue_core::BLUEY_TRACE_ID_HEADER)
                .unwrap(),
            TRACE_ID
        );
    }

    #[tokio::test]
    async fn replaces_free_form_request_and_trace_headers() {
        let app = Router::new()
            .route("/ok", get(echo_ids))
            .layer(axum::middleware::from_fn(request_id_middleware));

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/ok")
                    .header(cue_core::BLUEY_REQUEST_ID_HEADER, "user-secret-shaped-id")
                    .header(cue_core::BLUEY_TRACE_ID_HEADER, "private.account.value")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert!(uuid::Uuid::parse_str(
            response
                .headers()
                .get(cue_core::BLUEY_REQUEST_ID_HEADER)
                .unwrap()
                .to_str()
                .unwrap()
        )
        .is_ok());
        assert!(uuid::Uuid::parse_str(
            response
                .headers()
                .get(cue_core::BLUEY_TRACE_ID_HEADER)
                .unwrap()
                .to_str()
                .unwrap()
        )
        .is_ok());
    }

    #[tokio::test]
    async fn mints_missing_ids_and_injects_extensions() {
        let app = Router::new()
            .route("/ok", get(echo_ids))
            .layer(axum::middleware::from_fn(request_id_middleware));

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/ok")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let request_id = response
            .headers()
            .get(cue_core::BLUEY_REQUEST_ID_HEADER)
            .unwrap()
            .to_str()
            .unwrap();
        let trace_id = response
            .headers()
            .get(cue_core::BLUEY_TRACE_ID_HEADER)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(uuid::Uuid::parse_str(request_id).is_ok());
        assert!(uuid::Uuid::parse_str(trace_id).is_ok());
        assert!(response
            .headers()
            .get(cue_core::BLUEY_INTERACTION_ID_HEADER)
            .is_none());
    }

    #[tokio::test]
    async fn validates_injects_and_echoes_interaction_id() {
        const INTERACTION_ID: &str = "550e8400-e29b-41d4-a716-446655440000";
        let app = Router::new()
            .route("/ok", get(echo_ids))
            .layer(axum::middleware::from_fn(request_id_middleware));

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/ok")
                    .header(cue_core::BLUEY_INTERACTION_ID_HEADER, INTERACTION_ID)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            response
                .headers()
                .get(cue_core::BLUEY_INTERACTION_ID_HEADER)
                .unwrap(),
            INTERACTION_ID
        );
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert!(String::from_utf8(body.to_vec())
            .unwrap()
            .ends_with(INTERACTION_ID));
    }

    #[tokio::test]
    async fn drops_malformed_interaction_id_without_echoing_it() {
        let app = Router::new()
            .route("/ok", get(echo_ids))
            .layer(axum::middleware::from_fn(request_id_middleware));

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/ok")
                    .header(cue_core::BLUEY_INTERACTION_ID_HEADER, "person@example.com")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert!(response
            .headers()
            .get(cue_core::BLUEY_INTERACTION_ID_HEADER)
            .is_none());
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert!(String::from_utf8(body.to_vec()).unwrap().ends_with('|'));
    }
}
