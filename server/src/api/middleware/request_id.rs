//! Request/trace id middleware for Bluey's HTTP boundary.

use std::time::Instant;

use axum::{body::Body, extract::Request, http::HeaderValue, middleware::Next, response::Response};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestId(pub String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceId(pub String);

pub async fn request_id_middleware(mut req: Request<Body>, next: Next) -> Response {
    let request_id = header_id(req.headers(), cue_core::BLUEY_REQUEST_ID_HEADER)
        .unwrap_or_else(cue_core::new_request_id);
    let trace_id = header_id(req.headers(), cue_core::BLUEY_TRACE_ID_HEADER)
        .unwrap_or_else(cue_core::new_trace_id);
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let started = Instant::now();

    req.extensions_mut().insert(RequestId(request_id.clone()));
    req.extensions_mut().insert(TraceId(trace_id.clone()));

    tracing::info!(
        component = "bluey-server",
        version = env!("CARGO_PKG_VERSION"),
        platform = %cue_core::platform(),
        request_id = %request_id,
        trace_id = %trace_id,
        method = %method,
        path = %path,
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

    tracing::info!(
        component = "bluey-server",
        version = env!("CARGO_PKG_VERSION"),
        platform = %cue_core::platform(),
        request_id = %request_id,
        trace_id = %trace_id,
        method = %method,
        path = %path,
        status = response.status().as_u16(),
        latency_ms = started.elapsed().as_millis() as u64,
        "request done"
    );

    response
}

fn header_id(headers: &axum::http::HeaderMap, name: &'static str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .and_then(cue_core::sanitize_observability_id)
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

    async fn echo_ids(
        Extension(request_id): Extension<RequestId>,
        Extension(trace_id): Extension<TraceId>,
    ) -> String {
        format!("{}|{}", request_id.0, trace_id.0)
    }

    #[tokio::test]
    async fn echoes_incoming_trace_and_request_headers() {
        let app = Router::new()
            .route("/ok", get(echo_ids))
            .layer(axum::middleware::from_fn(request_id_middleware));

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/ok")
                    .header(cue_core::BLUEY_REQUEST_ID_HEADER, "req-123")
                    .header(cue_core::BLUEY_TRACE_ID_HEADER, "trace-123")
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
            "req-123"
        );
        assert_eq!(
            response
                .headers()
                .get(cue_core::BLUEY_TRACE_ID_HEADER)
                .unwrap(),
            "trace-123"
        );
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
    }
}
