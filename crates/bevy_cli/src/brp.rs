//! A Bevy Remote Protocol client: JSON-RPC calls and `+watch` streams, with the app's errors
//! mapped to stable error codes.
//!
//! The app answers requests once per frame, so every call costs at least a frame. Prefer one
//! query that returns many entities over many small calls.

use core::cell::{Cell, OnceCell};
use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};

use crate::http::{self, Endpoint, EventStream, HttpError};
use crate::output::{CliError, Exit};

/// The port `RemoteHttpPlugin` listens on unless the app chooses another.
pub const DEFAULT_PORT: u16 = 15702;

/// A connection to one app.
pub struct Client {
    /// Where the app listens.
    pub endpoint: Endpoint,
    /// How long to wait for an answer.
    pub timeout: Duration,
    /// What to suggest when nothing answers.
    pub unreachable_hint: String,
    next_id: Cell<u64>,
    methods: OnceCell<Vec<String>>,
}

impl Client {
    /// Creates a client. Nothing is sent until the first call.
    pub fn new(endpoint: Endpoint, timeout: Duration) -> Self {
        Self {
            endpoint,
            timeout,
            unreachable_hint: String::new(),
            next_id: Cell::new(1),
            methods: OnceCell::new(),
        }
    }

    /// How long to wait for a connection: short on this machine, where a listening app accepts
    /// within a millisecond however busy it is, since the OS accepts for it.
    fn connect_timeout(&self) -> Duration {
        if is_loopback(&self.endpoint.host) {
            http::LOOPBACK_PROBE * 2
        } else {
            Duration::from_secs(3)
        }
    }

    fn request(&self, method: &str, params: Option<Value>) -> Value {
        let id = self.next_id.get();
        self.next_id.set(id + 1);
        let mut request = json!({ "jsonrpc": "2.0", "id": id, "method": method });
        if let Some(params) = params {
            request["params"] = params;
        }
        request
    }

    /// Calls `method` and returns its result.
    pub fn call(&self, method: &str, params: Option<Value>) -> Result<Value, CliError> {
        self.call_with_timeout(method, params, self.timeout)
    }

    /// Calls `method`, waiting at most `timeout` for the answer.
    pub fn call_with_timeout(
        &self,
        method: &str,
        params: Option<Value>,
        timeout: Duration,
    ) -> Result<Value, CliError> {
        let body = serde_json::to_vec(&self.request(method, params))
            .map_err(|error| CliError::internal(error.to_string()))?;
        let started = Instant::now();
        let answer = http::post(&self.endpoint, &body, self.connect_timeout(), timeout)
            .map_err(|error| self.transport_error(error, method, timeout))?;
        trace(method, body.len(), answer.len(), started);
        let response: Value = serde_json::from_slice(&answer).map_err(|error| {
            CliError::new(
                "PROTOCOL_ERROR",
                Exit::Unreachable,
                format!("The answer to `{method}` isn't JSON: {error}"),
            )
        })?;
        response_result(method, response)
    }

    /// Calls several methods in one request, waiting at most `timeout`. Hearth answers a batch
    /// in one frame instead of one frame per call. Fails only when the app can't be reached;
    /// each call's own error is in its slot.
    pub fn batch(
        &self,
        calls: &[(&str, Option<Value>)],
        timeout: Duration,
    ) -> Result<Vec<Result<Value, CliError>>, CliError> {
        let first_id = self.next_id.get();
        let requests: Vec<Value> = calls
            .iter()
            .map(|(method, params)| self.request(method, params.clone()))
            .collect();
        let body =
            serde_json::to_vec(&requests).map_err(|error| CliError::internal(error.to_string()))?;
        let label = calls
            .iter()
            .map(|(method, _)| *method)
            .collect::<Vec<_>>()
            .join(", ");
        let started = Instant::now();
        let answer = http::post(&self.endpoint, &body, self.connect_timeout(), timeout)
            .map_err(|error| self.transport_error(error, &label, timeout))?;
        trace(
            &format!("batch [{label}]"),
            body.len(),
            answer.len(),
            started,
        );
        let Ok(Value::Array(answers)) = serde_json::from_slice::<Value>(&answer) else {
            // An app that can't take batches answers with a single error: call one at a time.
            return Ok(calls
                .iter()
                .map(|(method, params)| self.call_with_timeout(method, params.clone(), timeout))
                .collect());
        };
        let mut results: Vec<Option<Result<Value, CliError>>> =
            calls.iter().map(|_| None).collect();
        for answer in answers {
            let index = answer
                .get("id")
                .and_then(Value::as_u64)
                .and_then(|id| id.checked_sub(first_id))
                .and_then(|index| usize::try_from(index).ok());
            if let Some((slot, (method, _))) = index.and_then(|index| {
                let call = calls.get(index)?;
                Some((results.get_mut(index)?, call))
            }) {
                *slot = Some(response_result(method, answer));
            }
        }
        Ok(results
            .into_iter()
            .zip(calls)
            .map(|(result, (method, _))| {
                result.unwrap_or_else(|| {
                    Err(CliError::new(
                        "PROTOCOL_ERROR",
                        Exit::Unreachable,
                        format!("The app didn't answer `{method}` in the batch"),
                    ))
                })
            })
            .collect())
    }

    /// Remembers the methods from an `rpc.discover` result, so `supports` needn't ask again.
    pub fn remember_methods(&self, discovered: &Value) {
        let _ = self.methods.set(method_names(discovered));
    }

    /// Opens a `+watch` stream.
    pub fn watch(&self, method: &str, params: Option<Value>) -> Result<Watch, CliError> {
        let body = serde_json::to_vec(&self.request(method, params))
            .map_err(|error| CliError::internal(error.to_string()))?;
        let stream = http::post_stream(&self.endpoint, &body, self.connect_timeout())
            .map_err(|error| self.transport_error(error, method, self.timeout))?;
        Ok(Watch {
            stream,
            method: method.to_string(),
            endpoint: self.endpoint.clone(),
            ended: false,
        })
    }

    /// The methods the app supports, from `rpc.discover`.
    pub fn methods(&self) -> Result<&[String], CliError> {
        if let Some(methods) = self.methods.get() {
            return Ok(methods);
        }
        let discovered = self.call("rpc.discover", None)?;
        Ok(self.methods.get_or_init(|| method_names(&discovered)))
    }

    /// Whether the method list was already fetched.
    pub fn knows_methods(&self) -> bool {
        self.methods.get().is_some()
    }

    /// Whether the app supports `method`.
    pub fn supports(&self, method: &str) -> Result<bool, CliError> {
        Ok(self.methods()?.iter().any(|name| name == method))
    }

    /// Whether the app runs Bevy 0.20 or newer, which added `app.info`. These versions write
    /// entity ids as `<index>v<generation>` strings and split window events into typed
    /// messages themselves.
    pub fn is_modern(&self) -> Result<bool, CliError> {
        self.supports("app.info")
    }

    /// Waits until the app answers, or `timeout` passes. `alive` is polled between attempts, so
    /// a process that exits ends the wait early.
    pub fn wait_until_ready(
        &self,
        timeout: Duration,
        mut alive: impl FnMut() -> bool,
    ) -> Result<(), CliError> {
        let deadline = Instant::now() + timeout;
        loop {
            let attempt = self.call_with_timeout("rpc.discover", None, Duration::from_secs(5));
            match attempt {
                Ok(_) => return Ok(()),
                Err(error) if error.code != "APP_UNREACHABLE" && error.code != "TIMEOUT" => {
                    return Err(error);
                }
                Err(_) => {}
            }
            if !alive() {
                return Err(CliError::new(
                    "APP_EXITED",
                    Exit::Error,
                    "The app exited before it answered over BRP",
                ));
            }
            if Instant::now() >= deadline {
                return Err(CliError::new(
                    "TIMEOUT",
                    Exit::Timeout,
                    format!(
                        "The app didn't answer at {} within {:.0}s",
                        self.endpoint,
                        timeout.as_secs_f64()
                    ),
                ));
            }
            std::thread::sleep(Duration::from_millis(250));
        }
    }

    fn transport_error(&self, error: HttpError, method: &str, timeout: Duration) -> CliError {
        match error {
            HttpError::Connect(reason) => CliError::new(
                "APP_UNREACHABLE",
                Exit::Unreachable,
                format!("No app answered at {}", self.endpoint),
            )
            .hint(self.unreachable_hint.clone())
            .data(json!({ "endpoint": self.endpoint.to_string(), "reason": reason.to_string() })),
            HttpError::Timeout => CliError::new(
                "TIMEOUT",
                Exit::Timeout,
                format!(
                    "The app at {} didn't answer `{method}` within {:.1}s",
                    self.endpoint,
                    timeout.as_secs_f64()
                ),
            )
            .hint("Raise --timeout, or check the app isn't stalled: paused in a debugger, loading, or minimized with a low update rate."),
            HttpError::Protocol(message) => CliError::new(
                "PROTOCOL_ERROR",
                Exit::Unreachable,
                format!("Talking to {} failed: {message}", self.endpoint),
            )
            .hint("Check that this address belongs to an app with `RemoteHttpPlugin`."),
        }
    }
}

/// The method names in an `rpc.discover` result.
fn method_names(discovered: &Value) -> Vec<String> {
    discovered
        .get("methods")
        .and_then(Value::as_array)
        .map(|methods| {
            methods
                .iter()
                .filter_map(|method| method.get("name").and_then(Value::as_str))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// With `HEARTH_TRACE` set, writes each call's method, sizes and time to stderr.
fn trace(method: &str, sent: usize, received: usize, started: Instant) {
    use std::io::Write;
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if *ENABLED.get_or_init(|| std::env::var_os("HEARTH_TRACE").is_some()) {
        let _ = writeln!(
            std::io::stderr(),
            "[trace] {method}: {sent} B sent, {received} B received, {:.1} ms",
            started.elapsed().as_secs_f64() * 1000.0
        );
    }
}

/// Whether `host` names this machine.
pub fn is_loopback(host: &str) -> bool {
    host == "localhost"
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

/// Takes the `result` of a JSON-RPC response, or turns its `error` into a [`CliError`].
pub fn response_result(method: &str, response: Value) -> Result<Value, CliError> {
    let Value::Object(mut response) = response else {
        return Err(CliError::new(
            "PROTOCOL_ERROR",
            Exit::Unreachable,
            format!("The answer to `{method}` isn't a JSON-RPC response"),
        ));
    };
    match response.remove("error") {
        Some(error) if !error.is_null() => Err(brp_error(method, &error)),
        _ => Ok(response.remove("result").unwrap_or(Value::Null)),
    }
}

/// Maps a BRP error object to an error with a stable code.
pub fn brp_error(method: &str, error: &Value) -> CliError {
    let code = error.get("code").and_then(Value::as_i64).unwrap_or(0);
    let message = error
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("The app reported an error without a message")
        .to_string();
    let (name, hint) = match code {
        -32700 | -32600 => ("PROTOCOL_ERROR", None),
        -32601 => (
            "METHOD_NOT_FOUND",
            Some(format!(
                "This app doesn't support `{method}`: it may run an older engine version, or lack the plugin that adds it. `hearth methods` lists what it supports."
            )),
        ),
        -32602 => ("INVALID_PARAMS", None),
        -32603 => ("APP_ERROR", None),
        -23401 => (
            "ENTITY_NOT_FOUND",
            Some("Entity ids change between runs: find the current one with `hearth find` or `hearth tree`.".to_string()),
        ),
        -23402 => (
            "COMPONENT_ERROR",
            Some("`hearth schema <component>` shows the fields and an example value.".to_string()),
        ),
        -23403 => (
            "COMPONENT_NOT_PRESENT",
            Some("`hearth get <entity>` lists the entity's components.".to_string()),
        ),
        -23404 => ("SELF_REPARENT", None),
        -23405 => ("COMPONENT_NOT_IN_METADATA", None),
        -23501 => (
            "RESOURCE_ERROR",
            Some("`hearth resource list` lists the resources and `hearth schema <resource>` shows their fields.".to_string()),
        ),
        -23502 => (
            "RESOURCE_NOT_PRESENT",
            Some("`hearth resource list` lists the resources in the app.".to_string()),
        ),
        _ => ("BRP_ERROR", None),
    };
    let mut data = Map::new();
    data.insert("method".into(), json!(method));
    data.insert("brp_code".into(), json!(code));
    if let Some(details) = error.get("data").filter(|details| !details.is_null()) {
        data.insert("brp_data".into(), details.clone());
    }
    let error = CliError::new(name, Exit::Rejected, message).data(Value::Object(data));
    match hint {
        Some(hint) => error.hint(hint),
        None => error,
    }
}

/// What reading a [`Watch`] produced.
pub enum Next {
    /// A frame's worth of results.
    Event(Value),
    /// Nothing arrived in time.
    Idle,
    /// The app ended the stream.
    End,
}

/// An open `+watch` stream. Dropping it ends the watch in the app.
pub struct Watch {
    stream: EventStream,
    method: String,
    endpoint: Endpoint,
    ended: bool,
}

impl Watch {
    /// Reads the next result, waiting at most `wait`, or forever when `None`.
    pub fn next(&mut self, wait: Option<Duration>) -> Result<Next, CliError> {
        match self.next_raw(wait)? {
            Some(data) => {
                let response: Value = serde_json::from_str(&data).map_err(|error| {
                    CliError::new(
                        "PROTOCOL_ERROR",
                        Exit::Unreachable,
                        format!("A `{}` event isn't JSON: {error}", self.method),
                    )
                })?;
                response_result(&self.method, response).map(Next::Event)
            }
            None if self.ended => Ok(Next::End),
            None => Ok(Next::Idle),
        }
    }

    /// Whether the app ended the stream.
    pub fn ended(&self) -> bool {
        self.ended
    }

    /// Reads the next event's raw JSON text, or `None` when nothing arrived in time or the
    /// stream ended.
    pub fn next_raw(&mut self, wait: Option<Duration>) -> Result<Option<String>, CliError> {
        self.stream.set_timeout(wait);
        match self.stream.next_data() {
            Ok(Some(data)) => Ok(Some(data)),
            Ok(None) => {
                self.ended = true;
                Ok(None)
            }
            Err(HttpError::Timeout) => Ok(None),
            Err(error) => Err(CliError::new(
                "PROTOCOL_ERROR",
                Exit::Unreachable,
                format!(
                    "The `{}` stream from {} broke: {error}",
                    self.method, self.endpoint
                ),
            )),
        }
    }
}
