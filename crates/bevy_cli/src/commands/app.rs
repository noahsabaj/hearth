//! Commands about the app as a whole: status, time, schedules, diagnostics, raw calls and exit.

use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::stream::{drive, seconds};
use crate::cli::{CallArgs, DiagnosticsArgs, ExitArgs, SchedulesArgs, TimeArgs, TimeCommand};
use crate::context::Ctx;
use crate::known;
use crate::output::{table, CliError, Exit, Outcome, Style};
use crate::registry::shorten_path;
use crate::value;

/// `bevy status`.
pub fn status(ctx: &mut Ctx) -> Result<Outcome, CliError> {
    let target = ctx.target()?.to_json();
    let session = ctx.target()?.session.clone();
    let endpoint = ctx.client()?.endpoint.to_string();
    // The first call shows whether the app is there at all.
    let discover = ctx.call("rpc.discover", Value::Null)?;
    let methods = discover
        .get("methods")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let brp_version = discover
        .pointer("/info/version")
        .and_then(Value::as_str)
        .map(str::to_string);
    let info = ctx.call("app.info", Value::Null).ok();

    let entities = ctx
        .call("world.query", json!({ "data": {} }))
        .ok()
        .and_then(|rows| rows.as_array().map(Vec::len));
    let resources = ctx
        .call(
            "world.query",
            json!({ "data": {}, "filter": { "with": [known::IS_RESOURCE] }, "strict": true }),
        )
        .ok()
        .and_then(|rows| rows.as_array().map(Vec::len));
    let named = ctx.names().ok().map(<[_]>::len);

    let time = ctx
        .call(
            "world.get_resources",
            json!({ "resource": known::TIME_VIRTUAL }),
        )
        .ok()
        .and_then(|result| result.get("value").cloned())
        .map(|time| time_summary(&time));
    let window = primary_window(ctx).ok().flatten();
    let fps = ctx
        .call("diagnostics.get", json!({ "paths": ["fps"] }))
        .ok()
        .and_then(|result| {
            result
                .get("diagnostics")?
                .as_array()?
                .first()?
                .get("smoothed")?
                .as_f64()
        });

    let bevy_version = info
        .as_ref()
        .and_then(|info| info.get("bevy_version"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| brp_version.clone());
    let mut lines = Vec::new();
    let mut row = |label: &str, text: String| lines.push(format!("{label:<10}{text}"));
    let app_name = info
        .as_ref()
        .and_then(|info| info.get("app_name"))
        .and_then(Value::as_str)
        .unwrap_or("Bevy app");
    let sub_app = info
        .as_ref()
        .and_then(|info| info.get("sub_app"))
        .and_then(Value::as_str)
        .map(|sub_app| format!(" ({sub_app} sub-app)"))
        .unwrap_or_default();
    row(
        "App",
        format!(
            "{}{sub_app} at {}{}",
            ctx.out.paint(app_name, Style::Bold),
            ctx.out.paint(&endpoint, Style::Id),
            session
                .as_ref()
                .map(|session| format!(", session {} (pid {})", session.name, session.pid))
                .unwrap_or_default()
        ),
    );
    if let Some(version) = &bevy_version {
        row("Bevy", version.clone());
    }
    if let Some(entities) = entities {
        let mut details = Vec::new();
        if let Some(resources) = resources {
            details.push(format!("{resources} resources"));
        }
        if let Some(named) = named {
            details.push(format!("{named} named"));
        }
        let details = if details.is_empty() {
            String::new()
        } else {
            format!(" ({})", details.join(", "))
        };
        row("Entities", format!("{entities}{details}"));
    }
    if let Some(time) = &time {
        row("Time", describe_time(time));
    }
    if let Some(window) = &window {
        row("Window", describe_window(window));
    }
    if let Some(fps) = fps {
        row("FPS", format!("{fps:.1}"));
    }
    row(
        "BRP",
        format!("{methods} methods (`bevy methods` lists them)"),
    );

    Ok(Outcome::new(
        json!({
            "target": target,
            "app": info,
            "bevy_version": bevy_version,
            "brp_version": brp_version,
            "methods": methods,
            "entities": entities,
            "resources": resources,
            "named": named,
            "time": time,
            "window": window,
            "fps": fps,
            "session": session.map(|session| json!({
                "name": session.name,
                "pid": session.pid,
                "log": session.log,
            })),
        }),
        lines.join("\n"),
    ))
}

/// The primary window: its entity and the parts of `Window` worth showing.
pub fn primary_window(ctx: &mut Ctx) -> Result<Option<Value>, CliError> {
    let rows = ctx.call(
        "world.query",
        json!({
            "data": { "components": [known::WINDOW] },
            "filter": { "with": [known::PRIMARY_WINDOW] },
        }),
    )?;
    let Some(row) = rows.as_array().and_then(|rows| rows.first()) else {
        return Ok(None);
    };
    let window = &row["components"][known::WINDOW];
    let resolution = &window["resolution"];
    let scale = resolution["scale_factor_override"]
        .as_f64()
        .or_else(|| resolution["scale_factor"].as_f64())
        .unwrap_or(1.0);
    let physical_width = resolution["physical_width"].as_f64().unwrap_or(0.0);
    let physical_height = resolution["physical_height"].as_f64().unwrap_or(0.0);
    Ok(Some(json!({
        "entity": row["entity"],
        "title": window["title"],
        "width": (physical_width / scale).round(),
        "height": (physical_height / scale).round(),
        "physical_width": physical_width,
        "physical_height": physical_height,
        "scale_factor": scale,
        "mode": window["mode"],
        "focused": window["focused"],
    })))
}

fn describe_window(window: &Value) -> String {
    format!(
        "\"{}\" {}x{} ({}x{} physical, scale {}){}",
        window["title"].as_str().unwrap_or_default(),
        window["width"],
        window["height"],
        window["physical_width"],
        window["physical_height"],
        window["scale_factor"],
        if window["focused"].as_bool() == Some(true) {
            ", focused"
        } else {
            ", not focused"
        }
    )
}

/// The parts of `Time<Virtual>` worth showing.
fn time_summary(time: &Value) -> Value {
    json!({
        "paused": time.pointer("/context/paused"),
        "relative_speed": time.pointer("/context/relative_speed"),
        "effective_speed": time.pointer("/context/effective_speed"),
        "elapsed_secs": time.get("elapsed_secs_f64").or_else(|| time.get("elapsed_secs")),
        "delta_secs": time.get("delta_secs_f64").or_else(|| time.get("delta_secs")),
    })
}

fn describe_time(time: &Value) -> String {
    let paused = time["paused"].as_bool() == Some(true);
    let speed = time["relative_speed"].as_f64().unwrap_or(1.0);
    let elapsed = time["elapsed_secs"].as_f64().unwrap_or(0.0);
    format!(
        "{} at {speed}x, {elapsed:.1}s elapsed",
        if paused { "paused" } else { "running" }
    )
}

/// `bevy time`.
pub fn time(ctx: &mut Ctx, args: TimeArgs) -> Result<Outcome, CliError> {
    let action = args.action.unwrap_or(TimeCommand::Status);
    let change = match action {
        TimeCommand::Status => None,
        TimeCommand::Pause => Some((".context.paused", json!(true))),
        TimeCommand::Resume => Some((".context.paused", json!(false))),
        TimeCommand::Speed { speed } => {
            if !speed.is_finite() || speed < 0.0 {
                return Err(CliError::usage("The speed must be a number of at least 0"));
            }
            Some((".context.relative_speed", json!(speed)))
        }
    };
    if let Some((path, value)) = change {
        ctx.call(
            "world.mutate_resources",
            json!({ "resource": known::TIME_VIRTUAL, "path": path, "value": value }),
        )?;
    }
    let time = ctx.call(
        "world.get_resources",
        json!({ "resource": known::TIME_VIRTUAL }),
    )?;
    let mut summary = time_summary(time.get("value").unwrap_or(&Value::Null));
    // A change applies when the app next updates its clock, so report what was asked for.
    match action {
        TimeCommand::Pause => summary["paused"] = json!(true),
        TimeCommand::Resume => summary["paused"] = json!(false),
        TimeCommand::Speed { speed } => summary["relative_speed"] = json!(speed),
        TimeCommand::Status => {}
    }
    let human = format!("Virtual time is {}", describe_time(&summary));
    Ok(Outcome::new(summary, human))
}

/// `bevy schedules`.
pub fn schedules(ctx: &mut Ctx, args: SchedulesArgs) -> Result<Outcome, CliError> {
    let Some(label) = args.schedule else {
        let list = ctx.call("schedule.list", Value::Null)?;
        let names = |key: &str| -> Vec<String> {
            let mut names: Vec<String> = list
                .get(key)
                .and_then(Value::as_array)
                .map(|labels| {
                    labels
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            names.sort();
            names
        };
        let (available, empty, unavailable) = (
            names("schedule_labels"),
            names("empty_schedule_labels"),
            names("unavailable_schedule_labels"),
        );
        let mut lines = vec![available.join("\n")];
        if !empty.is_empty() {
            lines.push(
                ctx.out
                    .paint(&format!("Empty: {}", empty.join(", ")), Style::Dim),
            );
        }
        if !unavailable.is_empty() {
            lines.push(ctx.out.paint(
                &format!(
                    "Running now, so not inspectable: {}",
                    unavailable.join(", ")
                ),
                Style::Dim,
            ));
        }
        return Ok(Outcome::new(
            json!({ "schedules": available, "empty": empty, "unavailable": unavailable }),
            lines.join("\n"),
        ));
    };
    let graph = ctx.call("schedule.graph", json!({ "schedule_label": label }))?;
    let data = graph.get("schedule_data").cloned().unwrap_or(graph);
    let systems: Vec<String> = data
        .get("systems")
        .and_then(Value::as_array)
        .map(|systems| {
            systems
                .iter()
                .filter_map(|system| system.get("name").and_then(Value::as_str))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let sets = data
        .get("system_sets")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let mut lines = vec![format!(
        "{}: {} systems, {sets} sets",
        ctx.out.paint(&label, Style::Bold),
        systems.len()
    )];
    let mut sorted = systems.clone();
    sorted.sort_by_key(|system| shorten_path(system).to_lowercase());
    let width = sorted
        .iter()
        .map(|system| shorten_path(system).chars().count())
        .max()
        .unwrap_or(0);
    for system in &sorted {
        let short = shorten_path(system);
        let pad = " ".repeat(width - short.chars().count());
        lines.push(format!(
            "  {short}{pad}  {}",
            ctx.out.paint(system, Style::Dim)
        ));
    }
    Ok(Outcome::new(
        json!({ "schedule": label, "systems": systems, "graph": data }),
        lines.join("\n"),
    ))
}

/// `bevy diagnostics`.
pub fn diagnostics(ctx: &mut Ctx, args: DiagnosticsArgs) -> Result<Outcome, CliError> {
    let result = ctx.call("diagnostics.get", Value::Null)?;
    let mut diagnostics: Vec<Value> = result
        .get("diagnostics")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if let Some(pattern) = &args.pattern {
        let pattern = pattern.to_lowercase();
        diagnostics.retain(|diagnostic| {
            diagnostic["path"]
                .as_str()
                .is_some_and(|path| path.to_lowercase().contains(&pattern))
        });
    }
    let number = |value: &Value, suffix: &str| match value.as_f64() {
        Some(number) => format!("{number:.2}{suffix}"),
        None => "-".to_string(),
    };
    let rows: Vec<Vec<String>> = diagnostics
        .iter()
        .map(|diagnostic| {
            let suffix = diagnostic["suffix"].as_str().unwrap_or_default();
            vec![
                diagnostic["path"].as_str().unwrap_or_default().to_string(),
                number(&diagnostic["value"], suffix),
                number(&diagnostic["average"], suffix),
                number(&diagnostic["smoothed"], suffix),
            ]
        })
        .collect();
    let human = if rows.is_empty() {
        "No diagnostics. Add plugins such as `FrameTimeDiagnosticsPlugin` to record them."
            .to_string()
    } else {
        table(
            ctx.out.color,
            &["PATH", "VALUE", "AVERAGE", "SMOOTHED"],
            &rows,
        )
    };
    Ok(Outcome::new(Value::Array(diagnostics), human))
}

/// `bevy methods`.
pub fn methods(ctx: &mut Ctx) -> Result<Outcome, CliError> {
    let mut methods = ctx.client()?.methods()?.to_vec();
    methods.sort();
    let human = methods.join("\n");
    Ok(Outcome::new(json!(methods), human))
}

/// `bevy call`.
pub fn call(ctx: &mut Ctx, args: CallArgs) -> Result<Outcome, CliError> {
    let params = args.params.as_deref().map(value::parse_json).transpose()?;
    if !args.method.contains("+watch") {
        let result = ctx.client()?.call(&args.method, params)?;
        return Ok(Outcome::data(result));
    }
    let (count, duration) = ctx.stream_limits(args.stream.count, args.stream.duration);
    let mut watch = ctx.client()?.watch(&args.method, params)?;
    let (events, stop) = drive(ctx, &mut watch, count, duration, |ctx, frame, t, _| {
        let human = frame.to_string();
        ctx.out
            .event(json!({ "t": seconds(t), "result": frame }), || human);
        1
    })?;
    let human = ctx
        .out
        .paint(&format!("{events} results ({})", stop.name()), Style::Dim);
    Ok(Outcome::new(
        json!({ "method": args.method, "count": events, "stopped": stop.name() }),
        human,
    ))
}

/// `bevy exit`.
pub fn exit(ctx: &mut Ctx, args: ExitArgs) -> Result<Outcome, CliError> {
    let value = match args.code {
        Some(code) => json!({ "Error": code }),
        None => json!("Success"),
    };
    ctx.call(
        "world.write_message",
        json!({ "message": known::APP_EXIT, "value": value }),
    )?;
    if args.no_wait {
        return Ok(Outcome::new(
            json!({ "exited": null }),
            "Asked the app to exit",
        ));
    }
    let timeout = ctx.timeout.max(Duration::from_secs(5));
    let exited = wait_for_exit(ctx, timeout)?;
    if !exited {
        return Err(CliError::new(
            "TIMEOUT",
            Exit::Timeout,
            "The app was asked to exit, but it's still answering",
        )
        .hint("Something may keep it alive, such as a system that ignores `AppExit`. `bevy stop` ends a session's process."));
    }
    Ok(Outcome::new(json!({ "exited": true }), "The app exited"))
}

/// Waits until the app stops answering. Returns whether it did within `timeout`.
pub fn wait_for_exit(ctx: &mut Ctx, timeout: Duration) -> Result<bool, CliError> {
    let client = ctx.client()?;
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        match client.call_with_timeout("rpc.discover", None, Duration::from_secs(1)) {
            Err(error) if error.code == "APP_UNREACHABLE" || error.code == "PROTOCOL_ERROR" => {
                return Ok(true);
            }
            _ => std::thread::sleep(Duration::from_millis(200)),
        }
    }
    Ok(false)
}
