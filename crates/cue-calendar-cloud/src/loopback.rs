//! Transient single-shot loopback listener for the OAuth redirect (RFC 8252).
//!
//! The native app can't register a public https redirect, so the provider
//! redirects the browser back to `http://127.0.0.1:PORT/?code=...&state=...`.
//! We bind an ephemeral loopback port BEFORE opening the browser (so the caller
//! can build `redirect_uri` with the real port), then accept exactly one
//! authorization redirect, verify `state` (CSRF guard), and serve a minimal
//! "you can close this tab" page. The listener is torn down as soon as the
//! code arrives — it exists only for the ~60s consent window.

use std::net::SocketAddr;

use anyhow::{anyhow, bail, Context, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use url::Url;

/// Bind a transient listener on `127.0.0.1:0` and return it together with the
/// OS-assigned port, so the caller can build `redirect_uri=http://127.0.0.1:PORT`
/// before opening the browser.
pub async fn bind_loopback() -> Result<(TcpListener, u16)> {
    let addr: SocketAddr = ([127, 0, 0, 1], 0).into();
    let listener = TcpListener::bind(addr)
        .await
        .with_context(|| format!("bind loopback OAuth listener on {addr}"))?;
    let port = listener
        .local_addr()
        .context("read bound loopback port")?
        .port();
    Ok((listener, port))
}

/// Accept redirects on `listener` until one carries an authorization `code`,
/// verify its `state` against `expected_state` (CSRF guard), serve a friendly
/// close-tab page, and return the code.
///
/// Requests without a `code` (favicon fetches, prefetches, the odd double
/// request some browsers make) are answered with a 204 and ignored, so a noisy
/// browser can't derail the flow. A request that DOES carry a `code` but whose
/// `state` mismatches is a hard error (possible CSRF), served a 400.
pub async fn accept_code(listener: TcpListener, expected_state: &str) -> Result<String> {
    loop {
        let (mut stream, _peer) = listener
            .accept()
            .await
            .context("accept OAuth redirect connection")?;

        let request_line = match read_request_line(&mut stream).await {
            Ok(line) => line,
            Err(error) => {
                tracing::debug!("ignoring unreadable loopback request: {error:#}");
                continue;
            }
        };

        let target = match request_target(&request_line) {
            Some(target) => target,
            None => {
                let _ = respond(&mut stream, 400, "text/plain", "Bad Request").await;
                continue;
            }
        };

        let parsed = parse_redirect_query(&target);
        match parsed {
            Some(RedirectParams { code, state }) => {
                if state.as_deref() != Some(expected_state) {
                    let _ = respond(
                        &mut stream,
                        400,
                        "text/plain",
                        "State mismatch. Please retry connecting from Bluey.",
                    )
                    .await;
                    bail!("OAuth state mismatch (possible CSRF); redirect rejected");
                }
                let _ = respond(&mut stream, 200, "text/html; charset=utf-8", SUCCESS_PAGE).await;
                return Ok(code);
            }
            None => {
                // No code (favicon, prefetch, or an error redirect we don't
                // handle here). Acknowledge and keep waiting for the real one.
                let _ = respond(&mut stream, 204, "text/plain", "").await;
                continue;
            }
        }
    }
}

/// The minimal success page shown in the browser once the code is captured.
const SUCCESS_PAGE: &str = "<!doctype html><html><head><meta charset=\"utf-8\">\
<title>Bluey connected</title></head>\
<body style=\"font-family:-apple-system,system-ui,sans-serif;text-align:center;padding:3rem\">\
<h2>You can close this tab.</h2><p>Bluey is connected.</p></body></html>";

struct RedirectParams {
    code: String,
    state: Option<String>,
}

/// Read only the HTTP request line (`GET /path?query HTTP/1.1`). We don't need
/// headers or a body — the auth code rides in the query string.
async fn read_request_line(stream: &mut TcpStream) -> Result<String> {
    let mut buf = [0u8; 4096];
    let n = stream
        .read(&mut buf)
        .await
        .context("read loopback request bytes")?;
    if n == 0 {
        bail!("empty loopback request");
    }
    let text = String::from_utf8_lossy(&buf[..n]);
    let line = text
        .lines()
        .next()
        .ok_or_else(|| anyhow!("no request line in loopback request"))?;
    Ok(line.to_string())
}

/// Extract the request target (second whitespace-delimited token of the
/// request line).
fn request_target(request_line: &str) -> Option<String> {
    request_line.split_whitespace().nth(1).map(str::to_string)
}

/// Parse `code`/`state` out of the request target's query string, if a `code`
/// is present. Resolves the (relative) target against a dummy loopback base so
/// the `url` crate can parse the query.
fn parse_redirect_query(target: &str) -> Option<RedirectParams> {
    let base = Url::parse("http://127.0.0.1/").ok()?;
    let url = base.join(target).ok()?;
    let mut code = None;
    let mut state = None;
    for (k, v) in url.query_pairs() {
        match k.as_ref() {
            "code" => code = Some(v.into_owned()),
            "state" => state = Some(v.into_owned()),
            _ => {}
        }
    }
    code.map(|code| RedirectParams { code, state })
}

/// Write a minimal HTTP/1.1 response and close the connection.
async fn respond(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &str,
) -> Result<()> {
    let reason = match status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        _ => "OK",
    };
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\n\
         Content-Type: {content_type}\r\n\
         Content-Length: {len}\r\n\
         Connection: close\r\n\r\n{body}",
        len = body.len(),
    );
    stream
        .write_all(response.as_bytes())
        .await
        .context("write loopback response")?;
    stream.flush().await.context("flush loopback response")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_target_extracts_path_and_query() {
        let line = "GET /?code=abc&state=xyz HTTP/1.1";
        assert_eq!(
            request_target(line).as_deref(),
            Some("/?code=abc&state=xyz")
        );
    }

    #[test]
    fn parse_redirect_query_pulls_code_and_state() {
        let params = parse_redirect_query("/?code=the-code&state=the-state").expect("has code");
        assert_eq!(params.code, "the-code");
        assert_eq!(params.state.as_deref(), Some("the-state"));
    }

    #[test]
    fn parse_redirect_query_is_none_without_code() {
        assert!(parse_redirect_query("/favicon.ico").is_none());
        assert!(parse_redirect_query("/?state=only-state").is_none());
    }

    #[test]
    fn parse_redirect_query_url_decodes_values() {
        let params = parse_redirect_query("/?code=a%2Fb%2Bc&state=s").expect("has code");
        assert_eq!(params.code, "a/b+c");
    }

    #[tokio::test]
    async fn bind_loopback_returns_nonzero_port() {
        let (listener, port) = bind_loopback().await.expect("bind");
        assert_ne!(port, 0);
        assert_eq!(listener.local_addr().unwrap().port(), port);
    }

    #[tokio::test]
    async fn accept_code_captures_code_and_verifies_state() {
        let (listener, port) = bind_loopback().await.expect("bind");
        let state = "expected-state".to_string();

        let server = tokio::spawn({
            let state = state.clone();
            async move { accept_code(listener, &state).await }
        });

        // Simulate the browser redirect hitting the loopback listener.
        let mut client = TcpStream::connect(("127.0.0.1", port))
            .await
            .expect("connect");
        client
            .write_all(
                b"GET /?code=auth-code-123&state=expected-state HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n",
            )
            .await
            .expect("send redirect");

        // Drain the response so the server can finish writing.
        let mut resp = Vec::new();
        let _ = client.read_to_end(&mut resp).await;
        assert!(String::from_utf8_lossy(&resp).contains("200 OK"));

        let code = server.await.expect("join").expect("code");
        assert_eq!(code, "auth-code-123");
    }

    #[tokio::test]
    async fn accept_code_rejects_state_mismatch() {
        let (listener, port) = bind_loopback().await.expect("bind");
        let server = tokio::spawn(async move { accept_code(listener, "good-state").await });

        let mut client = TcpStream::connect(("127.0.0.1", port))
            .await
            .expect("connect");
        client
            .write_all(b"GET /?code=x&state=WRONG HTTP/1.1\r\n\r\n")
            .await
            .expect("send");
        let mut resp = Vec::new();
        let _ = client.read_to_end(&mut resp).await;
        assert!(String::from_utf8_lossy(&resp).contains("400"));

        let result = server.await.expect("join");
        assert!(result.is_err(), "state mismatch must error");
    }

    #[tokio::test]
    async fn accept_code_ignores_favicon_then_captures_code() {
        let (listener, port) = bind_loopback().await.expect("bind");
        let server = tokio::spawn(async move { accept_code(listener, "st").await });

        // First: a favicon request with no code — must be ignored (204).
        let mut noise = TcpStream::connect(("127.0.0.1", port))
            .await
            .expect("connect noise");
        noise
            .write_all(b"GET /favicon.ico HTTP/1.1\r\n\r\n")
            .await
            .expect("send favicon");
        let mut noise_resp = Vec::new();
        let _ = noise.read_to_end(&mut noise_resp).await;
        assert!(String::from_utf8_lossy(&noise_resp).contains("204"));

        // Then: the real redirect with the code.
        let mut client = TcpStream::connect(("127.0.0.1", port))
            .await
            .expect("connect real");
        client
            .write_all(b"GET /?code=real-code&state=st HTTP/1.1\r\n\r\n")
            .await
            .expect("send real");
        let mut resp = Vec::new();
        let _ = client.read_to_end(&mut resp).await;

        let code = server.await.expect("join").expect("code");
        assert_eq!(code, "real-code");
    }
}
