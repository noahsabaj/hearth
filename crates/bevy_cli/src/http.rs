//! A minimal blocking HTTP/1.1 client, enough to speak the Bevy Remote Protocol.
//!
//! BRP is JSON-RPC posted to a local HTTP server. Ordinary methods answer with one JSON body;
//! `+watch` methods answer with a server-sent event stream that stays open. Owning the socket
//! lets a stream be closed from another thread, which ends the watch on the app's side too.

use alloc::sync::Arc;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

/// Where a BRP server listens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoint {
    /// Host name or IP address.
    pub host: String,
    /// TCP port.
    pub port: u16,
    /// Request path, normally `/`.
    pub path: String,
}

impl Endpoint {
    /// An endpoint on `127.0.0.1`.
    pub fn localhost(port: u16) -> Self {
        Self::new("127.0.0.1", port)
    }

    /// An endpoint on `host:port` at the root path.
    pub fn new(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
            path: "/".to_string(),
        }
    }

    /// Parses `port`, `:port`, `host:port`, `[v6]:port` or `http://host:port/path`.
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let rest = text.strip_prefix("http://").unwrap_or(text);
        let (authority, path) = match rest.find('/') {
            Some(index) => (&rest[..index], rest[index..].to_string()),
            None => (rest, "/".to_string()),
        };
        if let Ok(port) = authority.parse::<u16>() {
            return Some(Self {
                path,
                ..Self::localhost(port)
            });
        }
        let (host, port) = authority.rsplit_once(':')?;
        let port = port.parse::<u16>().ok()?;
        let host = host
            .strip_prefix('[')
            .and_then(|host| host.strip_suffix(']'))
            .unwrap_or(host);
        let host = if host.is_empty() { "127.0.0.1" } else { host };
        Some(Self {
            host: host.to_string(),
            port,
            path,
        })
    }

    /// The `host:port` form, with IPv6 hosts in brackets.
    pub fn authority(&self) -> String {
        if self.host.contains(':') {
            format!("[{}]:{}", self.host, self.port)
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }
}

impl core::fmt::Display for Endpoint {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if self.path == "/" {
            write!(f, "{}", self.authority())
        } else {
            write!(f, "{}{}", self.authority(), self.path)
        }
    }
}

/// Why an HTTP exchange failed.
#[derive(Debug)]
pub enum HttpError {
    /// Nothing accepted the connection.
    Connect(std::io::Error),
    /// The server didn't answer in time.
    Timeout,
    /// The connection broke or the response was malformed.
    Protocol(String),
}

impl core::fmt::Display for HttpError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Connect(error) => write!(f, "could not connect: {error}"),
            Self::Timeout => write!(f, "timed out waiting for a response"),
            Self::Protocol(message) => write!(f, "{message}"),
        }
    }
}

fn map_io(error: std::io::Error) -> HttpError {
    match error.kind() {
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => HttpError::Timeout,
        _ => HttpError::Protocol(format!("connection error: {error}")),
    }
}

/// Opens a TCP connection, trying every resolved address.
fn connect(endpoint: &Endpoint, timeout: Duration) -> Result<TcpStream, HttpError> {
    let addresses: Vec<SocketAddr> = (endpoint.host.as_str(), endpoint.port)
        .to_socket_addrs()
        .map_err(HttpError::Connect)?
        .collect();
    let mut last = None;
    for address in addresses {
        match TcpStream::connect_timeout(&address, timeout) {
            Ok(stream) => {
                let _ = stream.set_nodelay(true);
                return Ok(stream);
            }
            Err(error) => last = Some(error),
        }
    }
    Err(HttpError::Connect(last.unwrap_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "no address")
    })))
}

/// Returns whether something accepts TCP connections at `endpoint`.
pub fn is_listening(endpoint: &Endpoint, timeout: Duration) -> bool {
    connect(endpoint, timeout).is_ok()
}

/// A parsed response head.
struct Head {
    status: u16,
    chunked: bool,
    content_length: Option<usize>,
}

fn send_request(
    endpoint: &Endpoint,
    body: &[u8],
    connect_timeout: Duration,
    read_timeout: Option<Duration>,
) -> Result<(Arc<TcpStream>, BufReader<Shared>, Head), HttpError> {
    let mut stream = connect(endpoint, connect_timeout)?;
    stream
        .set_read_timeout(read_timeout)
        .map_err(|error| HttpError::Protocol(error.to_string()))?;
    let _ = stream.set_write_timeout(Some(Duration::from_secs(30)));
    let request = format!(
        "POST {} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nAccept: application/json, text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\nUser-Agent: hearth-cli/{}\r\n\r\n",
        endpoint.path,
        endpoint.authority(),
        body.len(),
        env!("CARGO_PKG_VERSION"),
    );
    stream.write_all(request.as_bytes()).map_err(map_io)?;
    stream.write_all(body).map_err(map_io)?;
    stream.flush().map_err(map_io)?;

    // One socket handle for reading and for timeouts: on Windows a read timeout set on a
    // cloned handle doesn't apply to reads through the original.
    let control = Arc::new(stream);
    let mut reader = BufReader::with_capacity(64 * 1024, Shared(control.clone()));
    let head = read_head(&mut reader)?;
    Ok((control, reader, head))
}

/// A socket read through a shared handle.
struct Shared(Arc<TcpStream>);

impl Read for Shared {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        (&*self.0).read(buf)
    }
}

fn read_head(reader: &mut impl BufRead) -> Result<Head, HttpError> {
    let mut line = String::new();
    if reader.read_line(&mut line).map_err(map_io)? == 0 {
        return Err(HttpError::Protocol(
            "the server closed the connection without answering".to_string(),
        ));
    }
    let mut parts = line.split_whitespace();
    let version = parts.next().unwrap_or_default();
    if !version.starts_with("HTTP/") {
        return Err(HttpError::Protocol(format!(
            "not an HTTP response: {}",
            line.trim()
        )));
    }
    let status = parts
        .next()
        .and_then(|status| status.parse::<u16>().ok())
        .ok_or_else(|| HttpError::Protocol(format!("bad status line: {}", line.trim())))?;
    let mut head = Head {
        status,
        chunked: false,
        content_length: None,
    };
    loop {
        line.clear();
        if reader.read_line(&mut line).map_err(map_io)? == 0 {
            return Err(HttpError::Protocol(
                "truncated response headers".to_string(),
            ));
        }
        let header = line.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            let value = value.trim();
            if name.eq_ignore_ascii_case("transfer-encoding") {
                head.chunked = value.to_ascii_lowercase().contains("chunked");
            } else if name.eq_ignore_ascii_case("content-length") {
                head.content_length = value.parse().ok();
            }
        }
    }
    Ok(head)
}

/// Appends bytes to `line` up to the next `\n`, which is dropped along with a `\r` before it.
///
/// Returns `false` at the end of the input. Bytes read before an error, such as a read timeout,
/// stay in `line`, so the call can be repeated without losing data.
fn read_line_into(reader: &mut impl BufRead, line: &mut Vec<u8>) -> std::io::Result<bool> {
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return Ok(false);
        }
        if let Some(end) = available.iter().position(|&byte| byte == b'\n') {
            line.extend_from_slice(&available[..end]);
            reader.consume(end + 1);
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            return Ok(true);
        }
        let read = available.len();
        line.extend_from_slice(available);
        reader.consume(read);
    }
}

/// Where a [`ChunkedReader`] is in the chunked encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Chunk {
    /// Reading a chunk-size line.
    Size,
    /// Reading chunk data; the number of bytes left.
    Data(usize),
    /// Skipping the CRLF after chunk data; the number of bytes left.
    End(usize),
    /// Skipping trailers after the last chunk.
    Trailers,
    /// The body is complete.
    Done,
}

/// A reader that decodes `Transfer-Encoding: chunked`.
///
/// Its state survives errors, so reads interrupted by a timeout can be retried.
struct ChunkedReader<R> {
    inner: R,
    state: Chunk,
    line: Vec<u8>,
}

impl<R: BufRead> ChunkedReader<R> {
    fn new(inner: R) -> Self {
        Self {
            inner,
            state: Chunk::Size,
            line: Vec::new(),
        }
    }
}

impl<R: BufRead> Read for ChunkedReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        loop {
            match self.state {
                Chunk::Size => {
                    if !read_line_into(&mut self.inner, &mut self.line)? {
                        self.state = Chunk::Done;
                        continue;
                    }
                    let line = String::from_utf8_lossy(&self.line);
                    let size = line.split(';').next().unwrap_or_default().trim();
                    let size = usize::from_str_radix(size, 16).map_err(|_| {
                        std::io::Error::new(std::io::ErrorKind::InvalidData, "bad chunk size")
                    })?;
                    self.line.clear();
                    self.state = if size == 0 {
                        Chunk::Trailers
                    } else {
                        Chunk::Data(size)
                    };
                }
                Chunk::Data(remaining) => {
                    let wanted = buf.len().min(remaining);
                    let read = self.inner.read(&mut buf[..wanted])?;
                    if read == 0 {
                        self.state = Chunk::Done;
                        return Ok(0);
                    }
                    self.state = if read == remaining {
                        Chunk::End(2)
                    } else {
                        Chunk::Data(remaining - read)
                    };
                    return Ok(read);
                }
                Chunk::End(remaining) => {
                    let available = self.inner.fill_buf()?;
                    if available.is_empty() {
                        self.state = Chunk::Done;
                        continue;
                    }
                    let skip = available.len().min(remaining);
                    self.inner.consume(skip);
                    self.state = if skip == remaining {
                        Chunk::Size
                    } else {
                        Chunk::End(remaining - skip)
                    };
                }
                Chunk::Trailers => {
                    let more = read_line_into(&mut self.inner, &mut self.line)?;
                    let blank = self.line.is_empty();
                    self.line.clear();
                    if !more || blank {
                        self.state = Chunk::Done;
                    }
                }
                Chunk::Done => return Ok(0),
            }
        }
    }
}

fn read_body(reader: BufReader<Shared>, head: &Head) -> Result<Vec<u8>, HttpError> {
    let mut body = Vec::new();
    if head.chunked {
        ChunkedReader::new(reader)
            .read_to_end(&mut body)
            .map_err(map_io)?;
    } else if let Some(length) = head.content_length {
        body.resize(length, 0);
        let mut reader = reader;
        reader.read_exact(&mut body).map_err(map_io)?;
    } else {
        let mut reader = reader;
        reader.read_to_end(&mut body).map_err(map_io)?;
    }
    Ok(body)
}

/// Posts `body` and returns the response body.
pub fn post(
    endpoint: &Endpoint,
    body: &[u8],
    connect_timeout: Duration,
    read_timeout: Duration,
) -> Result<Vec<u8>, HttpError> {
    let (_control, reader, head) =
        send_request(endpoint, body, connect_timeout, Some(read_timeout))?;
    if head.status != 200 {
        return Err(HttpError::Protocol(format!(
            "the server answered with HTTP status {}",
            head.status
        )));
    }
    read_body(reader, &head)
}

/// A server-sent event stream: the response to a BRP `+watch` method.
pub struct EventStream {
    control: Arc<TcpStream>,
    reader: Box<dyn BufRead + Send>,
    /// The non-stream body, when the server answered with a plain JSON body.
    immediate: Option<Vec<u8>>,
    /// The line being read, kept across read timeouts.
    line: Vec<u8>,
    /// The data of the event being read, kept across read timeouts.
    data: String,
}

impl EventStream {
    /// Reads the next event's data, or `None` when the stream ended.
    ///
    /// A read timeout returns [`HttpError::Timeout`]; calling again continues where it stopped.
    pub fn next_data(&mut self) -> Result<Option<String>, HttpError> {
        if let Some(body) = self.immediate.take() {
            return Ok(Some(String::from_utf8_lossy(&body).into_owned()));
        }
        loop {
            let complete = read_line_into(&mut self.reader, &mut self.line).map_err(map_io)?;
            if !complete {
                self.line.clear();
                let data = core::mem::take(&mut self.data);
                return Ok((!data.is_empty()).then_some(data));
            }
            let line = core::mem::take(&mut self.line);
            if line.is_empty() {
                if !self.data.is_empty() {
                    return Ok(Some(core::mem::take(&mut self.data)));
                }
                continue;
            }
            // Other fields (`event:`, `id:`, `retry:`) and comments aren't used by BRP.
            if let Some(payload) = line.strip_prefix(b"data:") {
                let payload = payload.strip_prefix(b" ").unwrap_or(payload);
                let payload = String::from_utf8_lossy(payload);
                if !self.data.is_empty() {
                    self.data.push('\n');
                }
                self.data.push_str(&payload);
            }
        }
    }

    /// Sets how long [`EventStream::next_data`] blocks; `None` waits forever.
    pub fn set_timeout(&self, timeout: Option<Duration>) {
        let timeout = timeout.map(|timeout| timeout.max(Duration::from_millis(1)));
        let _ = self.control.set_read_timeout(timeout);
    }

    /// Closes the connection, which also ends the watch in the app.
    pub fn close(&self) {
        let _ = self.control.shutdown(Shutdown::Both);
    }
}

impl Drop for EventStream {
    fn drop(&mut self) {
        self.close();
    }
}

/// Posts `body` and returns the event stream the server answers with.
pub fn post_stream(
    endpoint: &Endpoint,
    body: &[u8],
    connect_timeout: Duration,
) -> Result<EventStream, HttpError> {
    let (control, reader, head) = send_request(endpoint, body, connect_timeout, None)?;
    if head.status != 200 {
        return Err(HttpError::Protocol(format!(
            "the server answered with HTTP status {}",
            head.status
        )));
    }
    if !head.chunked && head.content_length.is_some() {
        // Not a stream: the server answered at once, typically with an error.
        let body = read_body(reader, &head)?;
        return Ok(EventStream {
            control,
            reader: Box::new(std::io::empty()),
            immediate: Some(body),
            line: Vec::new(),
            data: String::new(),
        });
    }
    let reader: Box<dyn BufRead + Send> = if head.chunked {
        Box::new(BufReader::with_capacity(
            256 * 1024,
            ChunkedReader::new(reader),
        ))
    } else {
        Box::new(reader)
    };
    Ok(EventStream {
        control,
        reader,
        immediate: None,
        line: Vec::new(),
        data: String::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_endpoints() {
        assert_eq!(Endpoint::parse("15702"), Some(Endpoint::localhost(15702)));
        assert_eq!(Endpoint::parse(":15703"), Some(Endpoint::localhost(15703)));
        assert_eq!(
            Endpoint::parse("example.com:80"),
            Some(Endpoint::new("example.com", 80))
        );
        assert_eq!(
            Endpoint::parse("[::1]:9").map(|e| e.authority()),
            Some("[::1]:9".to_string())
        );
        let with_path = Endpoint::parse("http://10.0.0.2:1234/brp").unwrap();
        assert_eq!(with_path.host, "10.0.0.2");
        assert_eq!(with_path.path, "/brp");
        assert_eq!(with_path.to_string(), "10.0.0.2:1234/brp");
        assert_eq!(Endpoint::parse("my_game"), None);
        assert_eq!(Endpoint::parse("host:99999"), None);
    }

    #[test]
    fn decodes_chunked_bodies() {
        let raw =
            b"4\r\nWiki\r\n6;ext=1\r\npedia \r\nE\r\nin \r\n\r\nchunks.\r\n0\r\nTrailer: x\r\n\r\n";
        let mut body = String::new();
        ChunkedReader::new(&raw[..])
            .read_to_string(&mut body)
            .unwrap();
        assert_eq!(body, "Wikipedia in \r\n\r\nchunks.");
    }

    /// A reader that fails with a timeout on every other call, like a slow socket.
    struct Stuttering<'a> {
        data: &'a [u8],
        step: usize,
        calls: usize,
    }

    impl Read for Stuttering<'_> {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            self.calls += 1;
            if self.calls.is_multiple_of(2) {
                return Err(std::io::ErrorKind::TimedOut.into());
            }
            let read = buf.len().min(self.step).min(self.data.len());
            buf[..read].copy_from_slice(&self.data[..read]);
            self.data = &self.data[read..];
            Ok(read)
        }
    }

    #[test]
    fn chunked_reads_survive_timeouts() {
        let raw = b"4\r\nWiki\r\n6\r\npedia \r\n3\r\nin!\r\n0\r\n\r\n";
        let inner = BufReader::with_capacity(
            3,
            Stuttering {
                data: raw,
                step: 3,
                calls: 0,
            },
        );
        let mut reader = ChunkedReader::new(inner);
        let mut body = Vec::new();
        let mut buf = [0u8; 2];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(read) => body.extend_from_slice(&buf[..read]),
                Err(error) => assert_eq!(error.kind(), std::io::ErrorKind::TimedOut),
            }
        }
        assert_eq!(String::from_utf8(body).unwrap(), "Wikipedia in!");
    }

    #[test]
    fn server_sent_events_survive_timeouts() {
        let raw = b"data: {\"a\":1}\n\n: comment\ndata: line one\ndata: line two\n\ndata: last";
        let reader = BufReader::with_capacity(
            4,
            Stuttering {
                data: raw,
                step: 4,
                calls: 0,
            },
        );
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let control = Arc::new(TcpStream::connect(listener.local_addr().unwrap()).unwrap());
        let mut stream = EventStream {
            control,
            reader: Box::new(reader),
            immediate: None,
            line: Vec::new(),
            data: String::new(),
        };
        let mut events = Vec::new();
        loop {
            match stream.next_data() {
                Ok(Some(data)) => events.push(data),
                Ok(None) => break,
                Err(HttpError::Timeout) => {}
                Err(error) => panic!("{error}"),
            }
        }
        // An event the stream ends before finishing is dropped, as the SSE standard says.
        assert_eq!(events, ["{\"a\":1}", "line one\nline two"]);
    }

    #[test]
    fn reads_response_heads() {
        let raw = b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n";
        let head = read_head(&mut &raw[..]).unwrap();
        assert_eq!(head.status, 200);
        assert!(head.chunked);
        let raw = b"HTTP/1.1 200 OK\r\nContent-Length: 12\r\n\r\n";
        let head = read_head(&mut &raw[..]).unwrap();
        assert_eq!(head.content_length, Some(12));
        assert!(read_head(&mut &b"garbage\r\n"[..]).is_err());
    }
}
