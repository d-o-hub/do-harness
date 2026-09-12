//! Raw JSON-RPC forwarding to the upstream MCP endpoint.

#![forbid(unsafe_code)]

use rmcp::ErrorData as McpError;
use rmcp::model::{ErrorCode, RequestId};
use serde_json::{Value, json};

use crate::server::MAX_UPSTREAM_BYTES;
use crate::state::AppState;

/// Forwards a single JSON-RPC request and returns its `result` value.
///
/// Fail-closed: transport errors, oversized bodies, non-JSON payloads, and
/// upstream JSON-RPC errors all surface as [`McpError`]; the caller never
/// receives a partial result. SSE responses are scanned for the final
/// response event; full streaming is out of scope.
pub(super) async fn request(
    state: &AppState,
    method: &str,
    name: Option<&str>,
    params: Value,
    id: &RequestId,
) -> Result<Value, McpError> {
    let id = serde_json::to_value(id).unwrap_or(Value::Null);
    let body = json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params,
    });

    let mut req = state
        .client
        .post(&state.upstream)
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .header("mcp-protocol-version", super::MCP_PROTOCOL_VERSION)
        .header("mcp-method", method);
    if let Some(name) = name {
        let value = axum::http::HeaderValue::from_str(name).map_err(|_| {
            McpError::invalid_params("tool name is not header-safe for Mcp-Name", None)
        })?;
        req = req.header("mcp-name", value);
    }

    let response = req.json(&body).send().await.map_err(|err| {
        state.metrics.inc_upstream_failure();
        McpError::internal_error(format!("upstream unreachable: {err}"), None)
    })?;

    let is_sse = response
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("text/event-stream"));
    if response
        .content_length()
        .is_some_and(|len| len > MAX_UPSTREAM_BYTES as u64)
    {
        state.metrics.inc_upstream_failure();
        return Err(McpError::internal_error(
            "upstream response exceeds size limit",
            None,
        ));
    }
    let bytes = response.bytes().await.map_err(|err| {
        state.metrics.inc_upstream_failure();
        McpError::internal_error(format!("upstream read failed: {err}"), None)
    })?;
    if bytes.len() > MAX_UPSTREAM_BYTES {
        state.metrics.inc_upstream_failure();
        return Err(McpError::internal_error(
            "upstream response exceeds size limit",
            None,
        ));
    }

    let envelope = if is_sse {
        parse_sse(&bytes)
    } else {
        serde_json::from_slice::<Value>(&bytes).ok()
    };
    let Some(envelope) = envelope else {
        state.metrics.inc_upstream_failure();
        return Err(McpError::internal_error(
            "upstream response is not a JSON-RPC message",
            None,
        ));
    };

    if let Some(error) = envelope.get("error") {
        state.metrics.inc_upstream_failure();
        let code = error
            .get("code")
            .and_then(Value::as_i64)
            .and_then(|code| i32::try_from(code).ok())
            .unwrap_or(ErrorCode::INTERNAL_ERROR.0);
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("upstream error");
        return Err(McpError::new(
            ErrorCode(code),
            format!("upstream error {code}: {message}"),
            error.get("data").cloned(),
        ));
    }

    let Some(result) = envelope.get("result") else {
        state.metrics.inc_upstream_failure();
        return Err(McpError::internal_error(
            "upstream response missing result",
            None,
        ));
    };
    state.metrics.inc_upstream_ok();
    Ok(result.clone())
}

/// Extracts the last JSON-RPC response event from an SSE body.
fn parse_sse(bytes: &[u8]) -> Option<Value> {
    let text = std::str::from_utf8(bytes).ok()?;
    text.lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .filter_map(|data| serde_json::from_str::<Value>(data.trim()).ok())
        .rev()
        .find(|value| value.get("result").is_some() || value.get("error").is_some())
}
