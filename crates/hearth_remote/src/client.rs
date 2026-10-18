//! An asynchronous JSON-RPC 2.0 over HTTP client for the Hearth Remote Protocol.
//!
//! This module facilitates the creation of external tools
//! (such as entity inspectors, editors or test harnesses)
//! to access the ECS world of a local or remote Hearth application
//! that acts as a server.
//! It provides a thin asynchronous [`HrpClient`]
//! that sends simple HRP requests to the server Hearth application
//! (see [`RemoteHttpPlugin`]).
//!
//! [`RemoteHttpPlugin`]: crate::http::RemoteHttpPlugin
//!
//! [`HrpClient`] speaks to the same wire format that
//! [`RemoteHttpPlugin`](crate::http::RemoteHttpPlugin) serves: it is the client half of the
//! protocol documented at the [crate root](crate), including the method list. It follows the
//! usual shape of a thin RPC client library: send one request, decode the response envelope,
//! return the result or a typed error. There is no connection pooling, no retries, and no
//! authentication; callers that need those build them on top.
//!
//! This lives in `hearth_remote` behind its own `client` feature: it reuses the crate's own
//! request and response types ([`HrpRequest`], [`HrpResponse`], [`HrpPayload`], [`HrpError`])
//! and the same `hyper` and `smol` stack the `http` feature already depends on, adding no new
//! dependencies. Gating it behind its own feature, separate from `hearth_remote` and `http`, keeps
//! apps that only serve HRP from compiling the client. It is meant for in-engine tooling such as
//! the entity inspector, editors, and examples and tests that previously reached for a
//! third-party blocking HTTP client. Like the server, it is native only and does not build on
//! `wasm`.
//!
//! Construct a client with [`HrpClient::localhost`] or [`HrpClient::new`], then either `await`
//! [`HrpClient::call`] from an async context, or call [`HrpClient::spawn_call`] from a system to
//! get back a [`Task`] to store in a resource and poll with
//! [`poll_once`](hearth_tasks::futures_lite::future::poll_once) each frame:
//!
//! ```rust,no_run
//! # use hearth_remote::client::HrpClient;
//! # async fn call() {
//! let client = HrpClient::localhost(15702);
//! match client.call("world.list_components", None).await {
//!     Ok(_value) => { /* use the result */ }
//!     Err(_error) => { /* `Io`, `Http`, `Json`, `Remote`, or `InvalidResponse` */ }
//! }
//! # }
//! ```
//!
//! Errors distinguish where the call failed: [`HrpClientError::Io`] and
//! [`HrpClientError::Http`] for transport failures, [`HrpClientError::Json`] for a body that
//! did not deserialize, [`HrpClientError::Remote`] for a JSON-RPC error the app returned, and
//! [`HrpClientError::InvalidResponse`] for anything else that does not match the protocol.
//!
//! Not yet supported:
//!
//! * Watching methods (`+watch` suffixed), which stream server-sent events rather than
//!   returning a single response.
//! * Batch requests.
//! * Talking to the render sub-app's separate port.

#![cfg(not(target_family = "wasm"))]

use crate::{HrpError, HrpPayload, HrpRequest, HrpResponse};
use alloc::sync::Arc;
use async_io::Async;
use core::{
    net::SocketAddr,
    sync::atomic::{AtomicU64, Ordering},
};
use hearth_tasks::{IoTaskPool, Task, TaskPool};
use http_body_util::{BodyExt as _, Full};
use hyper::{
    body::Bytes,
    client::conn::http1,
    header::{CONTENT_TYPE, HOST},
    Request, StatusCode,
};
use serde_json::Value;
use smol_hyper::rt::FuturesIo;
use std::net::{TcpStream, ToSocketAddrs as _};

/// A client for one Hearth Remote Protocol HTTP endpoint.
#[derive(Clone, Debug)]
pub struct HrpClient {
    host: String,
    port: u16,
    path: String,
    request_id: Arc<AtomicU64>,
}

impl HrpClient {
    /// Creates a client targeting `host:port` at the root path.
    pub fn new(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
            path: "/".to_string(),
            request_id: Arc::new(AtomicU64::new(1)),
        }
    }

    /// Creates a client targeting `127.0.0.1:port`.
    pub fn localhost(port: u16) -> Self {
        Self::new("127.0.0.1", port)
    }

    /// Sets the request path, which defaults to `/`.
    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = path.into();
        self
    }

    /// Returns the full URL this client posts to.
    pub fn url(&self) -> String {
        format!("http://{}:{}{}", self.host, self.port, self.path)
    }

    /// Sends one request and awaits the JSON-RPC result. Watching methods return a stream that
    /// this method does not decode.
    pub async fn call(&self, method: &str, params: Option<Value>) -> Result<Value, HrpClientError> {
        let request = HrpRequest {
            method: method.to_string(),
            id: Some(Value::from(self.request_id.fetch_add(1, Ordering::Relaxed))),
            params,
        };
        let body = serde_json::to_vec(&request)?;

        let address = self.resolve()?;
        let stream = Async::<TcpStream>::connect(address).await?;
        let (mut sender, connection) = http1::handshake(FuturesIo::new(stream)).await?;
        IoTaskPool::get_or_init(TaskPool::default)
            .spawn(async move {
                let _ = connection.await;
            })
            .detach();

        let http_request = Request::post(&self.path)
            .header(HOST, format!("{}:{}", self.host, self.port))
            .header(CONTENT_TYPE, "application/json")
            .body(Full::new(Bytes::from(body)))
            .map_err(|error| HrpClientError::InvalidResponse(error.to_string()))?;

        let http_response = sender.send_request(http_request).await?;
        let status = http_response.status();
        let bytes = http_response.into_body().collect().await?.to_bytes();

        if status != StatusCode::OK {
            return Err(HrpClientError::InvalidResponse(format!(
                "server responded with status {status}"
            )));
        }

        let response: HrpResponse = serde_json::from_slice(&bytes)?;
        match response.payload {
            HrpPayload::Result(value) => Ok(value),
            HrpPayload::Error(error) => Err(HrpClientError::Remote(error)),
        }
    }

    /// Spawns [`HrpClient::call`] on the [`IoTaskPool`] and returns the task.
    pub fn spawn_call(
        &self,
        method: &str,
        params: Option<Value>,
    ) -> Task<Result<Value, HrpClientError>> {
        let client = self.clone();
        let method = method.to_string();
        IoTaskPool::get_or_init(TaskPool::default)
            .spawn(async move { client.call(&method, params).await })
    }

    fn resolve(&self) -> Result<SocketAddr, HrpClientError> {
        (self.host.as_str(), self.port)
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| {
                HrpClientError::InvalidResponse(format!("no address for host {}", self.host))
            })
    }
}

/// Errors from a HRP client call.
#[derive(Debug)]
pub enum HrpClientError {
    /// The connection could not be established or was lost.
    Io(std::io::Error),
    /// The HTTP exchange failed.
    Http(hyper::Error),
    /// The request or response could not be (de)serialized.
    Json(serde_json::Error),
    /// The remote app returned a JSON-RPC error.
    Remote(HrpError),
    /// The response was not a well-formed HRP response.
    InvalidResponse(String),
}

impl From<std::io::Error> for HrpClientError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<hyper::Error> for HrpClientError {
    fn from(value: hyper::Error) -> Self {
        Self::Http(value)
    }
}

impl From<serde_json::Error> for HrpClientError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

impl core::fmt::Display for HrpClientError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "I/O error: {error}"),
            Self::Http(error) => write!(f, "HTTP error: {error}"),
            Self::Json(error) => write!(f, "JSON error: {error}"),
            Self::Remote(error) => {
                write!(f, "remote error {}: {}", error.code, error.message)
            }
            Self::InvalidResponse(message) => write!(f, "invalid response: {message}"),
        }
    }
}

impl core::error::Error for HrpClientError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Http(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::Remote(_) | Self::InvalidResponse(_) => None,
        }
    }
}

#[cfg(all(test, feature = "http", feature = "client"))]
mod tests {
    use super::*;
    use crate::{http::RemoteHttpPlugin, RemotePlugin};
    use core::time::Duration;
    use hearth_app::{App, TaskPoolPlugin};
    use hearth_platform::sync::atomic::AtomicBool;
    use hearth_tasks::futures_lite::future;
    use std::net::TcpListener as StdTcpListener;

    fn free_port() -> u16 {
        let listener = StdTcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    }

    fn run_app(port: u16, stop: Arc<AtomicBool>) -> std::thread::JoinHandle<()> {
        std::thread::spawn(move || {
            let mut app = App::new();
            app.add_plugins((
                TaskPoolPlugin::default(),
                RemotePlugin::default(),
                RemoteHttpPlugin::default().with_port(port),
            ));
            app.finish();
            app.cleanup();
            while !stop.load(Ordering::Relaxed) {
                app.update();
                std::thread::sleep(Duration::from_millis(5));
            }
        })
    }

    #[test]
    fn round_trip() {
        let port = free_port();
        let stop = Arc::new(AtomicBool::new(false));
        let server = run_app(port, stop.clone());
        let client = HrpClient::localhost(port);

        assert_eq!(client.url(), format!("http://127.0.0.1:{port}/"));

        let discover = future::block_on(retry(&client, "rpc.discover"));
        assert!(discover.is_ok(), "rpc.discover failed: {discover:?}");

        let resources = future::block_on(client.call("world.list_resources", None));
        assert!(resources.is_ok(), "list_resources failed: {resources:?}");

        let missing = future::block_on(client.call("not.a.method", None));
        assert!(
            matches!(missing, Err(HrpClientError::Remote(_))),
            "expected remote error, got {missing:?}"
        );

        stop.store(true, Ordering::Relaxed);
        server.join().unwrap();
    }

    #[test]
    fn connection_refused() {
        let port = free_port();
        let client = HrpClient::localhost(port);
        let result = future::block_on(client.call("rpc.discover", None));
        assert!(
            matches!(result, Err(HrpClientError::Io(_))),
            "expected io error, got {result:?}"
        );
    }

    async fn retry(client: &HrpClient, method: &str) -> Result<Value, HrpClientError> {
        let mut last = client.call(method, None).await;
        for _ in 0..100 {
            if last.is_ok() {
                return last;
            }
            std::thread::sleep(Duration::from_millis(20));
            last = client.call(method, None).await;
        }
        last
    }
}
