//! A JSON-RPC batch sent over HTTP reaches the app all at once, so one frame answers it.

#![cfg(feature = "http")]

use core::time::Duration;
use hearth_app::{App, TaskPoolPlugin};
use hearth_remote::http::RemoteHttpPlugin;
use hearth_remote::{HrpReceiver, RemotePlugin};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Instant;

#[test]
fn one_frame_answers_a_batch() {
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let mut app = App::new();
    app.add_plugins((
        TaskPoolPlugin::default(),
        RemotePlugin::default(),
        RemoteHttpPlugin::default().with_port(port),
    ));
    app.finish();
    app.cleanup();
    // Startup opens the mailbox and starts the server.
    app.update();

    let deadline = Instant::now() + Duration::from_secs(10);
    let mut stream = loop {
        if let Ok(stream) = TcpStream::connect(("127.0.0.1", port)) {
            break stream;
        }
        assert!(Instant::now() < deadline, "the server never listened");
        std::thread::sleep(Duration::from_millis(10));
    };
    let body = json!([
        { "jsonrpc": "2.0", "id": 1, "method": "rpc.discover" },
        { "jsonrpc": "2.0", "id": 2, "method": "no.such.method" },
        { "jsonrpc": "2.0", "id": 3, "method": "world.list_resources" },
    ])
    .to_string();
    write!(
        stream,
        "POST / HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .unwrap();

    // All three requests are waiting before the app runs a frame.
    while app.world().resource::<HrpReceiver>().len() < 3 {
        assert!(
            Instant::now() < deadline,
            "the batch reached the app one request at a time"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    app.update();

    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    // The body is chunked: the JSON array sits between the chunk size and the final chunk.
    let json = &response[response.find('[').unwrap()..=response.rfind(']').unwrap()];
    let answers: Vec<Value> = serde_json::from_str(json).expect(&response);
    let ids: Vec<_> = answers.iter().map(|answer| answer["id"].clone()).collect();
    assert_eq!(ids, [json!(1), json!(2), json!(3)]);
    assert!(answers[0].get("result").is_some(), "{}", answers[0]);
    // An unknown method doesn't hold back the requests after it.
    assert!(answers[1].get("error").is_some(), "{}", answers[1]);
    assert!(answers[2].get("result").is_some(), "{}", answers[2]);
}
