//! The HRP transport using JSON-RPC over HTTP.
//!
//! Adding the [`RemoteHttpPlugin`] to your [`App`] causes Hearth to accept
//! connections over HTTP (by default, on port 15702) while your app is running.
//!
//! When `hearth_render` is enabled, a second port is available to query the render subapp.
//!
//! Clients are expected to `POST` JSON requests to the root URL; see the `client`
//! example for a trivial example of use.

#![cfg(not(target_family = "wasm"))]

#[cfg(feature = "hearth_render")]
use crate::setup_mailbox_channel;
use crate::{
    error_codes, HrpBatch, HrpError, HrpMessage, HrpRequest, HrpResponse, HrpResult, HrpSender,
};
use anyhow::Result as AnyhowResult;
use async_channel::{Receiver, Sender};
use async_io::Async;
use core::{
    convert::Infallible,
    net::{IpAddr, Ipv4Addr},
    pin::Pin,
    task::{Context, Poll},
};
use hearth_app::{App, Plugin, Startup};
use hearth_ecs::resource::Resource;
#[cfg(feature = "hearth_render")]
use hearth_ecs::schedule::IntoScheduleConfigs as _;
use hearth_ecs::system::Res;
#[cfg(feature = "hearth_render")]
use hearth_render::{RenderApp, RenderStartup};
use hearth_tasks::{futures_lite::StreamExt, IoTaskPool};
use http_body_util::{BodyExt as _, Full};
use hyper::{
    body::{Body, Bytes, Frame, Incoming},
    header::{HeaderName, HeaderValue},
    server::conn::http1,
    service, Request, Response,
};
use serde_json::Value;
use smol_hyper::rt::{FuturesIo, SmolTimer};
use std::{
    collections::HashMap,
    net::{TcpListener, TcpStream},
};

/// The default port that Hearth will listen on.
///
/// This value was chosen randomly.
pub const DEFAULT_PORT: u16 = 15702;

/// The default port that Hearth will listen on for the render subapp.
///
/// The render subapp is available for requests if the `hearth_render` feature is enabled.
pub const DEFAULT_RENDER_PORT: u16 = 15703;

/// The default host address that Hearth will use for its server.
pub const DEFAULT_ADDR: IpAddr = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));

/// A struct that holds a collection of HTTP headers.
///
/// This struct is used to store a set of HTTP headers as key-value pairs, where the keys are
/// of type [`HeaderName`] and the values are of type [`HeaderValue`].
///
#[derive(Debug, Resource, Clone)]
pub struct Headers {
    headers: HashMap<HeaderName, HeaderValue>,
}

impl Headers {
    /// Create a new instance of `Headers`.
    pub fn new() -> Self {
        Self {
            headers: HashMap::default(),
        }
    }

    /// Insert a key value pair to the `Headers` instance.
    pub fn insert(
        mut self,
        name: impl TryInto<HeaderName>,
        value: impl TryInto<HeaderValue>,
    ) -> Self {
        let Ok(header_name) = name.try_into() else {
            panic!("Invalid header name")
        };
        let Ok(header_value) = value.try_into() else {
            panic!("Invalid header value")
        };
        self.headers.insert(header_name, header_value);
        self
    }
}

impl Default for Headers {
    fn default() -> Self {
        Self::new()
    }
}

/// Add this plugin to your [`App`] to allow remote connections over HTTP to inspect and modify entities.
/// It requires the [`RemotePlugin`](super::RemotePlugin).
///
/// This HRP transport cannot be used when targeting WASM.
///
/// The defaults are:
/// - [`DEFAULT_ADDR`] : 127.0.0.1.
/// - [`DEFAULT_PORT`] : 15702.
/// - [`DEFAULT_RENDER_PORT`] : 15703. (when `hearth_render` is enabled)
///
pub struct RemoteHttpPlugin {
    /// The address that Hearth will bind to.
    address: IpAddr,
    /// The port that Hearth will listen on.
    port: u16,
    /// The port that Hearth will listen on for render subapp.
    #[cfg_attr(
        not(feature = "hearth_render"),
        expect(dead_code, reason = "only used by the render sub-app")
    )]
    render_port: u16,
    /// The headers that Hearth will include in its HTTP responses
    headers: Headers,
}

impl Default for RemoteHttpPlugin {
    fn default() -> Self {
        Self {
            address: DEFAULT_ADDR,
            port: DEFAULT_PORT,
            render_port: DEFAULT_RENDER_PORT,
            headers: Headers::new(),
        }
    }
}

impl Plugin for RemoteHttpPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(HostAddress(self.address))
            .insert_resource(HostPort(self.port))
            .insert_resource(HostHeaders(self.headers.clone()))
            .add_systems(Startup, start_http_server);

        #[cfg(feature = "hearth_render")]
        {
            use hearth_ecs::schedule::common_conditions::run_once;

            let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
                return;
            };

            render_app
                .insert_resource(HostAddress(self.address))
                .insert_resource(HostPort(self.render_port))
                .insert_resource(HostHeaders(self.headers.clone()))
                .add_systems(
                    RenderStartup,
                    start_http_server
                        .run_if(run_once)
                        .after(setup_mailbox_channel),
                );
        }
    }
}

impl RemoteHttpPlugin {
    /// Set the IP address that the server will use.
    #[must_use]
    pub fn with_address(mut self, address: impl Into<IpAddr>) -> Self {
        self.address = address.into();
        self
    }
    /// Set the remote port that the server will listen on.
    #[must_use]
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }
    /// Set the extra headers that the response will include.
    ///
    /// ////// /// # Example
    ///
    /// ```ignore
    ///
    /// // Create CORS headers
    /// let cors_headers = Headers::new()
    ///        .insert("Access-Control-Allow-Origin", "*")
    ///        .insert("Access-Control-Allow-Headers", "Content-Type");
    ///
    /// // Create the Hearth app and add the RemoteHttpPlugin with CORS headers
    /// fn main() {
    ///     App::new()
    ///     .add_plugins(DefaultPlugins)
    ///     .add_plugins(RemotePlugin::default())
    ///     .add_plugins(RemoteHttpPlugin::default()
    ///         .with_headers(cors_headers))
    ///     .run();
    /// }
    /// ```
    #[must_use]
    pub fn with_headers(mut self, headers: Headers) -> Self {
        self.headers = headers;
        self
    }
    /// Add a single header to the response headers.
    #[must_use]
    pub fn with_header(
        mut self,
        name: impl TryInto<HeaderName>,
        value: impl TryInto<HeaderValue>,
    ) -> Self {
        self.headers = self.headers.insert(name, value);
        self
    }
}

/// A resource containing the IP address that Hearth will host on.
///
/// Currently, changing this while the application is running has no effect; this merely
/// reflects the IP address that is set during the setup of the [`RemoteHttpPlugin`].
#[derive(Debug, Resource)]
pub struct HostAddress(pub IpAddr);

/// A resource containing the port number that Hearth will listen on.
///
/// Currently, changing this while the application is running has no effect; this merely
/// reflects the host that is set during the setup of the [`RemoteHttpPlugin`].
#[derive(Debug, Resource)]
pub struct HostPort(pub u16);

/// A resource containing the headers that Hearth will include in its HTTP responses.
///
#[derive(Debug, Resource)]
struct HostHeaders(pub Headers);

/// A system that starts up the Hearth Remote Protocol HTTP server.
fn start_http_server(
    request_sender: Res<HrpSender>,
    address: Res<HostAddress>,
    remote_port: Res<HostPort>,
    headers: Res<HostHeaders>,
) {
    IoTaskPool::get()
        .spawn(server_main(
            address.0,
            remote_port.0,
            request_sender.clone(),
            headers.0.clone(),
        ))
        .detach();
}

/// The Hearth Remote Protocol server main loop.
async fn server_main(
    address: IpAddr,
    port: u16,
    request_sender: Sender<HrpMessage>,
    headers: Headers,
) -> AnyhowResult<()> {
    listen(
        Async::<TcpListener>::bind((address, port))?,
        &request_sender,
        &headers,
    )
    .await
}

async fn listen(
    listener: Async<TcpListener>,
    request_sender: &Sender<HrpMessage>,
    headers: &Headers,
) -> AnyhowResult<()> {
    loop {
        let (client, _) = listener.accept().await?;

        let request_sender = request_sender.clone();
        let headers = headers.clone();
        IoTaskPool::get()
            .spawn(async move {
                let _ = handle_client(client, request_sender, headers).await;
            })
            .detach();
    }
}

async fn handle_client(
    client: Async<TcpStream>,
    request_sender: Sender<HrpMessage>,
    headers: Headers,
) -> AnyhowResult<()> {
    http1::Builder::new()
        .timer(SmolTimer::new())
        .serve_connection(
            FuturesIo::new(client),
            service::service_fn(|request| {
                process_request_batch(request, &request_sender, &headers)
            }),
        )
        .await?;

    Ok(())
}

/// A helper function for the Hearth Remote Protocol server that handles a batch
/// of requests coming from a client.
async fn process_request_batch(
    request: Request<Incoming>,
    request_sender: &Sender<HrpMessage>,
    headers: &Headers,
) -> AnyhowResult<Response<HrpHttpBody>> {
    let batch_bytes = request.into_body().collect().await?.to_bytes();
    let batch: Result<HrpBatch, _> = serde_json::from_slice(&batch_bytes);

    let result = match batch {
        Ok(HrpBatch::Single(request)) => {
            let response = process_single_request(request, request_sender).await?;
            match response {
                HrpHttpResponse::Complete(res) => {
                    HrpHttpResponse::Complete(serde_json::to_string(&res)?)
                }
                HrpHttpResponse::Stream(stream) => HrpHttpResponse::Stream(stream),
            }
        }
        Ok(HrpBatch::Batch(requests)) => {
            // Hand every request to the app before waiting for any answer, so the app
            // answers the whole batch in one frame instead of one frame per request.
            let mut pending = Vec::with_capacity(requests.len());
            for request in requests {
                pending.push(dispatch_request(request, request_sender, false).await);
            }
            let mut responses = Vec::with_capacity(pending.len());
            for dispatched in pending {
                responses.push(match dispatched {
                    Dispatched::Answered(response) => response,
                    Dispatched::Waiting { id, receiver } => {
                        HrpResponse::new(id, receiver.recv().await?)
                    }
                    Dispatched::Stream(HrpStream { id, .. }) => HrpResponse::new(
                        id,
                        Err(HrpError {
                            code: error_codes::INVALID_REQUEST,
                            message: "Streaming can not be used in batch requests".to_string(),
                            data: None,
                        }),
                    ),
                });
            }

            HrpHttpResponse::Complete(serde_json::to_string(&responses)?)
        }
        Err(err) => {
            let err = HrpResponse::new(
                None,
                Err(HrpError {
                    code: error_codes::INVALID_REQUEST,
                    message: err.to_string(),
                    data: None,
                }),
            );

            HrpHttpResponse::Complete(serde_json::to_string(&err)?)
        }
    };

    let mut response = match result {
        HrpHttpResponse::Complete(serialized) => {
            let mut response = Response::new(HrpHttpBody::Complete(Full::new(Bytes::from(
                serialized.as_bytes().to_owned(),
            ))));
            response.headers_mut().insert(
                hyper::header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            );
            response
        }
        HrpHttpResponse::Stream(stream) => {
            let mut response = Response::new(HrpHttpBody::Stream(stream));
            response.headers_mut().insert(
                hyper::header::CONTENT_TYPE,
                HeaderValue::from_static("text/event-stream"),
            );
            response
        }
    };
    for (key, value) in &headers.headers {
        response.headers_mut().insert(key, value.clone());
    }
    Ok(response)
}

/// A helper function for the Hearth Remote Protocol server that processes a single
/// request coming from a client.
async fn process_single_request(
    request: Value,
    request_sender: &Sender<HrpMessage>,
) -> AnyhowResult<HrpHttpResponse<HrpResponse, HrpStream>> {
    Ok(
        match dispatch_request(request, request_sender, true).await {
            Dispatched::Answered(response) => HrpHttpResponse::Complete(response),
            Dispatched::Waiting { id, receiver } => {
                HrpHttpResponse::Complete(HrpResponse::new(id, receiver.recv().await?))
            }
            Dispatched::Stream(stream) => HrpHttpResponse::Stream(stream),
        },
    )
}

/// A request handed to the app, or answered without it.
enum Dispatched {
    /// The request was answered without reaching the app.
    Answered(HrpResponse),
    /// The app will send the answer to `receiver`.
    Waiting {
        id: Option<Value>,
        receiver: Receiver<HrpResult>,
    },
    /// A `+watch` request, whose answers stream.
    Stream(HrpStream),
}

/// Parses a request and sends it to the app without waiting for the answer. A `+watch`
/// request is only sent when `allow_watch` is set; otherwise it gets a [`Dispatched::Stream`]
/// whose receiver never gets an answer.
async fn dispatch_request(
    request: Value,
    request_sender: &Sender<HrpMessage>,
    allow_watch: bool,
) -> Dispatched {
    // Reach in and get the request ID early so that we can report it even when parsing fails.
    let id = request.as_object().and_then(|map| map.get("id")).cloned();

    let request: HrpRequest = match serde_json::from_value(request) {
        Ok(v) => v,
        Err(err) => {
            return Dispatched::Answered(HrpResponse::new(
                id,
                Err(HrpError {
                    code: error_codes::INVALID_REQUEST,
                    message: err.to_string(),
                    data: None,
                }),
            ));
        }
    };

    let watch = request.method.contains("+watch");
    let size = if watch { 8 } else { 1 };
    let (result_sender, result_receiver) = async_channel::bounded(size);

    if watch && !allow_watch {
        return Dispatched::Stream(HrpStream {
            id: request.id,
            rx: Box::pin(result_receiver),
        });
    }

    let _ = request_sender
        .send(HrpMessage {
            method: request.method,
            params: request.params,
            sender: result_sender,
        })
        .await;

    if watch {
        Dispatched::Stream(HrpStream {
            id: request.id,
            rx: Box::pin(result_receiver),
        })
    } else {
        Dispatched::Waiting {
            id: request.id,
            receiver: result_receiver,
        }
    }
}

struct HrpStream {
    id: Option<Value>,
    rx: Pin<Box<Receiver<HrpResult>>>,
}

impl Body for HrpStream {
    type Data = Bytes;
    type Error = Infallible;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        match self.as_mut().rx.poll_next(cx) {
            Poll::Ready(result) => match result {
                Some(result) => {
                    let response = HrpResponse::new(self.id.clone(), result);
                    let serialized = serde_json::to_string(&response).unwrap();
                    let bytes =
                        Bytes::from(format!("data: {serialized}\n\n").as_bytes().to_owned());
                    let frame = Frame::data(bytes);
                    Poll::Ready(Some(Ok(frame)))
                }
                None => Poll::Ready(None),
            },
            Poll::Pending => Poll::Pending,
        }
    }

    fn is_end_stream(&self) -> bool {
        self.rx.is_closed()
    }
}

enum HrpHttpResponse<C, S> {
    Complete(C),
    Stream(S),
}

enum HrpHttpBody {
    Complete(Full<Bytes>),
    Stream(HrpStream),
}

impl Body for HrpHttpBody {
    type Data = Bytes;
    type Error = Infallible;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        match &mut *self.get_mut() {
            HrpHttpBody::Complete(body) => Body::poll_frame(Pin::new(body), cx),
            HrpHttpBody::Stream(body) => Body::poll_frame(Pin::new(body), cx),
        }
    }
}
