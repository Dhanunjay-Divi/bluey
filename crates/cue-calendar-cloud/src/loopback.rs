//! Transient single-shot loopback listener for the OAuth redirect (RFC 8252).
//!
//! The native app can't register a public https redirect, so the provider
//! redirects the browser back to a loopback URI with `code` + `state`.
//! We bind an ephemeral loopback port BEFORE opening the browser (so the caller
//! can build `redirect_uri` with the real port), then accept exactly one
//! authorization redirect, verify `state` (CSRF guard), and serve a minimal
//! "you can close this tab" page. The listener is torn down as soon as the
//! code arrives — it exists only for the ~60s consent window.

use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use url::Url;

/// Bind a transient provider-compatible loopback host and return it with the
/// OS-assigned port. Google desktop clients use `127.0.0.1`; Microsoft's
/// mobile/desktop platform registers `localhost`. Binding the same hostname
/// placed in the redirect URI avoids an IPv4/IPv6 mismatch on Microsoft flows.
pub async fn bind_loopback(host: &str) -> Result<(TcpListener, u16)> {
    if !matches!(host, "127.0.0.1" | "localhost") {
        bail!("unsupported OAuth loopback host");
    }
    let listener = TcpListener::bind((host, 0))
        .await
        .with_context(|| format!("bind loopback OAuth listener on {host}"))?;
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
        let (mut stream, peer) = listener
            .accept()
            .await
            .context("accept OAuth redirect connection")?;
        if !peer.ip().is_loopback() {
            tracing::warn!(%peer, "rejected non-loopback OAuth callback");
            continue;
        }

        let request_line = match tokio::time::timeout(
            Duration::from_secs(5),
            read_request_line(&mut stream),
        )
        .await
        {
            Ok(Ok(line)) => line,
            Err(_) => {
                tracing::debug!("ignoring timed-out loopback request");
                continue;
            }
            Ok(Err(error)) => {
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

        match parse_redirect_query(&target) {
            Some(RedirectResult::Code { code, state }) => {
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
            Some(RedirectResult::Error {
                error,
                description,
                state,
            }) => {
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
                let _ = respond(&mut stream, 400, "text/html; charset=utf-8", ERROR_PAGE).await;
                let description = description
                    .as_deref()
                    .map(sanitize_oauth_message)
                    .filter(|value| !value.is_empty());
                return Err(match description {
                    Some(description) => {
                        anyhow!("authorization was rejected ({error}): {description}")
                    }
                    None => anyhow!("authorization was rejected ({error})"),
                });
            }
            None => {
                // No OAuth response (favicon, prefetch, or the odd double
                // request some browsers make). Acknowledge and keep waiting.
                let _ = respond(&mut stream, 204, "text/plain", "").await;
            }
        }
    }
}

/// The minimal success page shown in the browser once the code is captured.
const SUCCESS_PAGE: &str = "<!doctype html><html><head><meta charset=\"utf-8\">\
<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
<title>Bluey connected</title></head>\
<body style=\"font-family:-apple-system,system-ui,sans-serif;text-align:center;padding:3rem\">\
<h2>Calendar connected.</h2><p>You can close this tab and return to Bluey.</p></body></html>";

const ERROR_PAGE: &str = "<!doctype html><html><head><meta charset=\"utf-8\">\
<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
<title>Bluey connection cancelled</title></head>\
<body style=\"font-family:-apple-system,system-ui,sans-serif;text-align:center;padding:3rem\">\
<h2>Calendar was not connected.</h2><p>Return to Bluey to retry or skip this step.</p></body></html>";

enum RedirectResult {
    Code {
        code: String,
        state: Option<String>,
    },
    Error {
        error: String,
        description: Option<String>,
        state: Option<String>,
    },
}

fn sanitize_oauth_message(message: &str) -> String {
    message
        .chars()
        .filter(|character| !character.is_control())
        .take(300)
        .collect::<String>()
        .trim()
        .to_string()
}

/// Read only the HTTP request line (`GET /path?query HTTP/1.1`). We don't need
/// headers or a body — the auth code rides in the query string.
async fn read_request_line(stream: &mut TcpStream) -> Result<String> {
    const MAX_REQUEST_LINE_BYTES: usize = 4_096;
    let mut line = Vec::with_capacity(512);
    let mut chunk = [0u8; 512];
    loop {
        let n = stream
            .read(&mut chunk)
            .await
            .context("read loopback request bytes")?;
        if n == 0 {
            bail!("incomplete loopback request line");
        }
        line.extend_from_slice(&chunk[..n]);
        if let Some(newline) = line.iter().position(|byte| *byte == b'\n') {
            if newline >= MAX_REQUEST_LINE_BYTES {
                bail!("loopback request line exceeded {MAX_REQUEST_LINE_BYTES} bytes");
            }
            line.truncate(newline);
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            let line = String::from_utf8(line).context("decode loopback request line")?;
            if line.trim().is_empty() {
                bail!("empty loopback request line");
            }
            return Ok(line);
        }
        if line.len() >= MAX_REQUEST_LINE_BYTES {
            bail!("loopback request line exceeded {MAX_REQUEST_LINE_BYTES} bytes");
        }
    }
}

/// Extract the request target (second whitespace-delimited token of the
/// request line).
fn request_target(request_line: &str) -> Option<String> {
    request_line.split_whitespace().nth(1).map(str::to_string)
}

/// Parse either a successful `code` response or an OAuth `error` response.
/// Requests without either field are browser noise and return `None`.
fn parse_redirect_query(target: &str) -> Option<RedirectResult> {
    let base = Url::parse("http://127.0.0.1/").ok()?;
    let url = base.join(target).ok()?;
    let mut code = None;
    let mut state = None;
    let mut error = None;
    let mut description = None;
    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "code" => code = Some(value.into_owned()),
            "state" => state = Some(value.into_owned()),
            "error" => error = Some(value.into_owned()),
            "error_description" => description = Some(value.into_owned()),
            _ => {}
        }
    }
    if let Some(error) = error {
        return Some(RedirectResult::Error {
            error,
            description,
            state,
        });
    }
    code.map(|code| RedirectResult::Code { code, state })
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
         Cache-Control: no-store\r\n\
         Content-Security-Policy: default-src 'none'; style-src 'unsafe-inline'\r\n\
         X-Content-Type-Options: nosniff\r\n\
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
        match params {
            RedirectResult::Code { code, state } => {
                assert_eq!(code, "the-code");
                assert_eq!(state.as_deref(), Some("the-state"));
            }
            RedirectResult::Error { .. } => panic!("expected code"),
        }
    }

    #[test]
    fn parse_redirect_query_is_none_without_code() {
        assert!(parse_redirect_query("/favicon.ico").is_none());
        assert!(parse_redirect_query("/?state=only-state").is_none());
    }

    #[test]
    fn parse_redirect_query_url_decodes_values() {
        let params = parse_redirect_query("/?code=a%2Fb%2Bc&state=s").expect("has code");
        match params {
            RedirectResult::Code { code, .. } => assert_eq!(code, "a/b+c"),
            RedirectResult::Error { .. } => panic!("expected code"),
        }
    }

    #[test]
    fn parse_redirect_query_captures_provider_error() {
        let params = parse_redirect_query(
            "/?error=access_denied&error_description=User%20cancelled&state=s",
        )
        .expect("has error");
        match params {
            RedirectResult::Error {
                error,
                description,
                state,
            } => {
                assert_eq!(error, "access_denied");
                assert_eq!(description.as_deref(), Some("User cancelled"));
                assert_eq!(state.as_deref(), Some("s"));
            }
            RedirectResult::Code { .. } => panic!("expected error"),
        }
    }

    #[tokio::test]
    async fn bind_loopback_returns_nonzero_port() {
        let (listener, port) = bind_loopback("127.0.0.1").await.expect("bind");
        assert_ne!(port, 0);
        assert_eq!(listener.local_addr().unwrap().port(), port);
    }

    #[tokio::test]
    async fn microsoft_localhost_redirect_reaches_the_bound_listener() {
        let (listener, port) = bind_loopback("localhost").await.expect("bind");
        let accept = tokio::spawn(async move { listener.accept().await });
        TcpStream::connect(("localhost", port))
            .await
            .expect("connect via the redirect hostname");
        let (_, peer) = accept.await.expect("join").expect("accept");
        assert!(peer.ip().is_loopback());
    }

    #[tokio::test]
    async fn request_line_can_arrive_in_multiple_tcp_reads() {
        let (listener, port) = bind_loopback("127.0.0.1").await.expect("bind");
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept");
            read_request_line(&mut stream).await
        });
        let mut client = TcpStream::connect(("127.0.0.1", port))
            .await
            .expect("connect");
        client.write_all(b"GET /?code=frag").await.expect("part 1");
        tokio::task::yield_now().await;
        client
            .write_all(b"mented&state=s HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .await
            .expect("part 2");
        assert_eq!(
            server.await.expect("join").expect("line"),
            "GET /?code=fragmented&state=s HTTP/1.1"
        );
    }

    #[tokio::test]
    async fn accept_code_captures_code_and_verifies_state() {
        let (listener, port) = bind_loopback("127.0.0.1").await.expect("bind");
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
        let (listener, port) = bind_loopback("127.0.0.1").await.expect("bind");
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
    async fn accept_code_surfaces_provider_denial_without_timeout() {
        let (listener, port) = bind_loopback("127.0.0.1").await.expect("bind");
        let server = tokio::spawn(async move { accept_code(listener, "good-state").await });

        let mut client = TcpStream::connect(("127.0.0.1", port))
            .await
            .expect("connect");
        client
            .write_all(
                b"GET /?error=access_denied&error_description=User%20cancelled&state=good-state HTTP/1.1\r\n\r\n",
            )
            .await
            .expect("send");
        let mut response = Vec::new();
        let _ = client.read_to_end(&mut response).await;
        assert!(String::from_utf8_lossy(&response).contains("400"));

        let error = server
            .await
            .expect("join")
            .expect_err("provider denial must fail");
        assert!(error.to_string().contains("access_denied"));
        assert!(error.to_string().contains("User cancelled"));
    }

    #[tokio::test]
    async fn accept_code_ignores_favicon_then_captures_code() {
        let (listener, port) = bind_loopback("127.0.0.1").await.expect("bind");
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
