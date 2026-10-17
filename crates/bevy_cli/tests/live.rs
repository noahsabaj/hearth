//! End-to-end tests: the `bevy` binary against a headless app that serves the Bevy Remote
//! Protocol on a free port.

use core::time::Duration;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::time::Instant;

use bevy_app::{App, TaskPoolPlugin};
use bevy_ecs::hierarchy::{ChildOf, Children};
use bevy_ecs::name::Name;
use bevy_ecs::prelude::*;
use bevy_ecs::reflect::{ReflectComponent, ReflectResource};
use bevy_reflect::{std_traits::ReflectDefault, Reflect};
use bevy_remote::http::RemoteHttpPlugin;
use bevy_remote::RemotePlugin;
use bevy_time::TimePlugin;
use serde_json::{json, Value};

#[derive(Component, Reflect, Default)]
#[reflect(Component, Default)]
struct Health(u32);

#[derive(Component, Reflect, Default)]
#[reflect(Component, Default)]
struct Stats {
    speed: f32,
    tags: Vec<String>,
}

#[derive(Resource, Reflect)]
#[reflect(Resource)]
struct Score(u32);

/// Starts the app once for all tests and returns its port.
fn app() -> u16 {
    static PORT: OnceLock<u16> = OnceLock::new();
    *PORT.get_or_init(|| {
        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        std::thread::spawn(move || {
            let mut app = App::new();
            app.add_plugins((
                TaskPoolPlugin::default(),
                TimePlugin,
                RemotePlugin::default(),
                RemoteHttpPlugin::default().with_port(port),
            ))
            .register_type::<Name>()
            .register_type::<ChildOf>()
            .register_type::<Children>()
            .register_type::<Health>()
            .register_type::<Stats>()
            .register_type::<Score>()
            .insert_resource(Score(11));

            let world = app.world_mut();
            let player = world
                .spawn((
                    Name::new("Player"),
                    Health(7),
                    Stats {
                        speed: 1.5,
                        tags: vec!["hero".to_string()],
                    },
                ))
                .id();
            world.spawn((Name::new("Sword"), ChildOf(player)));
            world.spawn((Name::new("Mutable"), Health(1), Stats::default()));
            world.spawn((Name::new("Watched"), Health(0)));

            app.finish();
            app.cleanup();
            loop {
                app.update();
                std::thread::sleep(Duration::from_millis(5));
            }
        });
        let started = Instant::now();
        while TcpStream::connect(("127.0.0.1", port)).is_err() {
            assert!(
                started.elapsed() < Duration::from_secs(30),
                "the app never listened"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        port
    })
}

/// A home for sessions, so tests don't see the user's.
fn home() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bevy-cli-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn command(args: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_bevy"));
    command
        .args(args)
        .env("BEVY_CLI_HOME", home())
        .env_remove("BEVY_APP")
        .env_remove("BEVY_FORMAT")
        .env_remove("BEVY_TIMEOUT")
        .stdin(Stdio::null());
    command
}

/// Runs `bevy --app <app> --json <args>` and returns the envelope and the exit code.
fn bevy(args: &[&str]) -> (Value, i32) {
    let app = format!("127.0.0.1:{}", app());
    let mut all = vec!["--app", &app, "--json"];
    all.extend_from_slice(args);
    run(&all)
}

fn run(args: &[&str]) -> (Value, i32) {
    let output = command(args).output().unwrap();
    let envelope = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "`bevy {}` didn't print JSON ({error}):\n{}\n{}",
            args.join(" "),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    });
    (envelope, output.status.code().unwrap_or(-1))
}

/// Runs a command that must succeed and returns its data.
fn ok(args: &[&str]) -> Value {
    let (envelope, code) = bevy(args);
    assert!(
        envelope["success"] == true && code == 0,
        "`bevy {}` failed with {code}: {envelope:#}",
        args.join(" ")
    );
    envelope["data"].clone()
}

/// Runs a command that must fail and returns its error code and exit code.
fn fails(args: &[&str]) -> (String, i32) {
    let (envelope, code) = bevy(args);
    assert_eq!(
        envelope["success"],
        false,
        "`bevy {}` succeeded",
        args.join(" ")
    );
    let error = envelope["errors"][0]["code"].as_str().unwrap().to_string();
    (error, code)
}

#[test]
fn reports_status() {
    let data = ok(&["status"]);
    assert!(data["entities"].as_u64().unwrap() >= 4, "{data:#}");
    assert!(data["named"].as_u64().unwrap() >= 4, "{data:#}");
    assert_eq!(data["bevy_version"], env!("CARGO_PKG_VERSION"));
}

#[test]
fn shows_the_hierarchy() {
    let data = ok(&["tree"]);
    let roots = data["roots"].as_array().unwrap();
    let player = roots
        .iter()
        .find(|root| root["name"] == "Player")
        .expect("Player is a root");
    assert_eq!(player["children"][0]["name"], "Sword");
    assert!(roots.iter().all(|root| root["name"] != "Sword"));
}

#[test]
fn finds_and_reads_entities() {
    let found = ok(&["find", "play*"]);
    assert_eq!(found.as_array().unwrap().len(), 1, "{found:#}");
    let id = found[0]["entity"].as_str().unwrap().to_string();

    let data = ok(&["get", &id]);
    assert_eq!(data["name"], "Player");
    let health = data["components"]
        .as_object()
        .unwrap()
        .iter()
        .find(|(path, _)| path.ends_with("Health"))
        .map(|(_, value)| value.clone());
    assert_eq!(health, Some(json!(7)));

    let data = ok(&["get", "Player", "Stats.speed", "Stats.tags[0]"]);
    let values: Vec<&Value> = data["components"].as_object().unwrap().values().collect();
    assert!(
        values.contains(&&json!(1.5)) && values.contains(&&json!("hero")),
        "{data:#}"
    );

    let with = ok(&["find", "--with", "Health"]);
    assert!(with.as_array().unwrap().len() >= 3, "{with:#}");
}

#[test]
fn changes_components() {
    ok(&["set", "Mutable", "Health", "9"]);
    ok(&["set", "Mutable", "Stats.speed", "2.5"]);
    ok(&["set", "Mutable", "Stats.tags", "[\"a\", \"b\"]"]);
    let data = ok(&["get", "Mutable", "Health", "Stats"]);
    let components = data["components"].as_object().unwrap();
    let stats = components
        .iter()
        .find(|(path, _)| path.ends_with("Stats"))
        .unwrap()
        .1;
    assert_eq!(stats["speed"], json!(2.5));
    assert_eq!(stats["tags"], json!(["a", "b"]));
    let health = components
        .iter()
        .find(|(path, _)| path.ends_with("Health"))
        .unwrap()
        .1;
    assert_eq!(health, &json!(9));
}

#[test]
fn spawns_and_despawns() {
    let data = ok(&["spawn", "--name", "Probe", "Health=3", "--parent", "Player"]);
    let id = data["entity"]
        .as_str()
        .expect("spawn returns the entity")
        .to_string();
    let data = ok(&["get", "Probe", "Health"]);
    assert_eq!(data["entity"], id.as_str());
    ok(&["despawn", &id]);
    assert_eq!(fails(&["get", &id]), ("ENTITY_NOT_FOUND".to_string(), 4));
}

#[test]
fn queries_components() {
    let rows = ok(&["query", "Health", "--with", "Stats"]);
    let rows = rows.as_array().unwrap();
    assert_eq!(rows.len(), 2, "{rows:#?}");
    assert!(rows
        .iter()
        .all(|row| row["components"].as_object().unwrap().len() == 1));
}

#[test]
fn reads_and_changes_resources() {
    let data = ok(&["resource", "get", "Score"]);
    assert_eq!(data["value"], json!(11));
    ok(&["resource", "set", "Score", "12"]);
    assert_eq!(ok(&["resource", "get", "Score"])["value"], json!(12));
    let list = ok(&["resource", "list"]);
    assert!(list.to_string().contains("Score"), "{list:#}");
}

#[test]
fn searches_types_and_schemas() {
    let types = ok(&["types", "Stats"]);
    assert!(
        types
            .as_array()
            .unwrap()
            .iter()
            .any(|found| found["short"] == "Stats"),
        "{types:#}"
    );
    let schema = ok(&["schema", "Stats"]);
    assert!(schema.to_string().contains("speed"), "{schema:#}");
}

#[test]
fn controls_time() {
    let data = ok(&["time", "pause"]);
    assert_eq!(data["paused"], true);
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(ok(&["time"])["paused"], true);
    ok(&["time", "speed", "2"]);
    let data = ok(&["time", "resume"]);
    assert_eq!(data["paused"], false);
    assert_eq!(data["relative_speed"], json!(2.0));
    ok(&["time", "speed", "1"]);
}

#[test]
fn streams_changes() {
    let app = format!("127.0.0.1:{}", app());
    let mut watcher: Child = command(&[
        "--app",
        &app,
        "--format",
        "ndjson",
        "watch",
        "Watched",
        "Health",
        "--count",
        "1",
        "--duration",
        "20",
    ])
    .stdout(Stdio::piped())
    .spawn()
    .unwrap();
    // Keep changing the value until the watcher reports one and exits.
    let started = Instant::now();
    let mut value = 0;
    while watcher.try_wait().unwrap().is_none() {
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "the watch never ended"
        );
        value += 1;
        ok(&["set", "Watched", "Health", &value.to_string()]);
        std::thread::sleep(Duration::from_millis(200));
    }
    let lines: Vec<Value> = BufReader::new(watcher.stdout.take().unwrap())
        .lines()
        .map(|line| serde_json::from_str(&line.unwrap()).unwrap())
        .collect();
    assert_eq!(lines.len(), 2, "{lines:#?}");
    assert_eq!(lines[0]["type"], "event");
    assert_eq!(lines[1]["type"], "result");
    assert_eq!(lines[1]["data"]["stopped"], "count");
}

#[test]
fn reports_errors_with_codes() {
    assert_eq!(
        fails(&["get", "Nobody"]),
        ("ENTITY_NOT_FOUND".to_string(), 4)
    );
    assert_eq!(
        fails(&["get", "Player", "Nonexistent"]),
        ("UNKNOWN_TYPE".to_string(), 4)
    );
    assert_eq!(fails(&["set", "Player", "Stats.nope", "1"]).1, 4);
    assert_eq!(fails(&["get"]), ("MISSING_ARGUMENT".to_string(), 2));
    assert_eq!(fails(&["frobnicate"]), ("UNKNOWN_COMMAND".to_string(), 2));
    assert_eq!(
        fails(&["call", "no.such_method"]),
        ("METHOD_NOT_FOUND".to_string(), 4)
    );
}

#[test]
fn reports_unreachable_apps() {
    // A port that nothing listens on: bind one, then free it.
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let app = format!("127.0.0.1:{port}");
    let (envelope, code) = run(&["--app", &app, "--json", "--timeout", "2", "status"]);
    assert_eq!(
        envelope["errors"][0]["code"], "APP_UNREACHABLE",
        "{envelope:#}"
    );
    assert_eq!(code, 3);
}

#[test]
fn lists_commands() {
    let (envelope, code) = run(&["commands", "--json"]);
    assert_eq!(code, 0);
    let commands = envelope["data"]["commands"].as_array().unwrap();
    assert!(commands
        .iter()
        .any(|command| command["name"] == "input key"));
    assert!(!envelope["data"]["error_codes"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn serves_mcp() {
    let app = format!("127.0.0.1:{}", app());
    let mut server = command(&["mcp", "--app", &app])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = server.stdin.take().unwrap();
    let mut output = BufReader::new(server.stdout.take().unwrap());
    let mut request = |id: u32, method: &str, params: Value| -> Value {
        let message = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        writeln!(input, "{message}").unwrap();
        let mut line = String::new();
        output.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap()
    };

    let init = request(
        1,
        "initialize",
        json!({ "protocolVersion": "2025-06-18", "capabilities": {} }),
    );
    assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
    let tools = request(2, "tools/list", json!({}));
    let tools = tools["result"]["tools"].as_array().unwrap();
    assert!(tools.iter().any(|tool| tool["name"] == "get"));
    assert!(tools.iter().all(|tool| tool["name"] != "mcp"));

    let result = request(
        3,
        "tools/call",
        json!({ "name": "get", "arguments": { "entity": "Player", "components": ["Health"] } }),
    );
    assert_eq!(result["result"]["isError"], false, "{result:#}");
    let envelope: Value =
        serde_json::from_str(result["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert!(
        envelope["data"]["components"].to_string().contains('7'),
        "{envelope:#}"
    );

    let result = request(
        4,
        "tools/call",
        json!({ "name": "get", "arguments": { "entity": "Nobody" } }),
    );
    assert_eq!(result["result"]["isError"], true, "{result:#}");

    drop(input);
    assert!(server.wait().unwrap().success());
}
