//! `hearth watch` and `hearth observe`: streaming changes and events from the app.
//!
//! Each event is printed as it arrives: a line of text, or with `--format ndjson` a JSON line,
//! ending with a `result` frame. With `--format json` the events are collected into the result.

use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::world::resolve_all;
use crate::brp::{Next, Watch};
use crate::cli::{ObserveArgs, WatchArgs};
use crate::context::{entity_text, Ctx};
use crate::output::{CliError, Outcome, Style};
use crate::registry::Want;
use crate::value;

/// Why a stream stopped.
#[derive(Clone, Copy, Debug)]
pub enum Stop {
    /// `--count` events arrived.
    Count,
    /// `--duration` passed.
    Duration,
    /// The app ended the stream.
    Closed,
}

impl Stop {
    /// The JSON name.
    pub fn name(self) -> &'static str {
        match self {
            Self::Count => "count",
            Self::Duration => "duration",
            Self::Closed => "closed",
        }
    }
}

/// Reads `watch` until `count` events were emitted or `duration` passed.
///
/// `emit` handles one frame of results: it gets the seconds since the start and how many more
/// events may be emitted, and returns how many it emitted.
pub fn drive(
    ctx: &mut Ctx,
    watch: &mut Watch,
    count: Option<u64>,
    duration: Option<Duration>,
    mut emit: impl FnMut(&mut Ctx, Value, f64, u64) -> u64,
) -> Result<(u64, Stop), CliError> {
    let start = Instant::now();
    let deadline = duration.map(|duration| start + duration);
    let mut emitted = 0u64;
    loop {
        if count.is_some_and(|count| emitted >= count) {
            return Ok((emitted, Stop::Count));
        }
        let wait = match deadline {
            Some(deadline) => {
                let left = deadline.saturating_duration_since(Instant::now());
                if left.is_zero() {
                    return Ok((emitted, Stop::Duration));
                }
                Some(left)
            }
            None => None,
        };
        match watch.next(wait)? {
            Next::Event(frame) => {
                let remaining = count.map_or(u64::MAX, |count| count - emitted);
                emitted += emit(ctx, frame, start.elapsed().as_secs_f64(), remaining);
            }
            Next::Idle => {}
            Next::End => return Ok((emitted, Stop::Closed)),
        }
    }
}

/// Seconds rounded to milliseconds.
pub fn seconds(t: f64) -> f64 {
    (t * 1000.0).round() / 1000.0
}

/// `hearth watch`.
pub fn watch(ctx: &mut Ctx, args: WatchArgs) -> Result<Outcome, CliError> {
    let entity = ctx.entity(&args.entity)?;
    let components = resolve_all(ctx, &args.components, Want::Component)?;
    let (count, duration) = ctx.stream_limits(args.stream.count, args.stream.duration);
    let (method, params) = if components.is_empty() {
        ("world.list_components+watch", json!({ "entity": entity }))
    } else {
        (
            "world.get_components+watch",
            json!({ "entity": entity, "components": components }),
        )
    };
    let mut watch = ctx.client()?.watch(method, Some(params))?;
    ctx.out.info(format!(
        "Watching {}{}",
        entity_text(&entity),
        if count.is_none() && duration.is_none() {
            " (Ctrl+C to stop)"
        } else {
            ""
        }
    ));
    let (events, stop) = drive(ctx, &mut watch, count, duration, |ctx, frame, t, _| {
        let t = seconds(t);
        let stamp = ctx.out.paint(&format!("+{t:.2}s"), Style::Dim);
        let mut lines = Vec::new();
        let list = |key: &str| -> Vec<String> {
            frame
                .get(key)
                .and_then(Value::as_array)
                .map(|paths| {
                    paths
                        .iter()
                        .filter_map(Value::as_str)
                        .map(|path| ctx.short_name(path))
                        .collect()
                })
                .unwrap_or_default()
        };
        if let Some(values) = frame.get("components").and_then(Value::as_object) {
            for (path, value) in values {
                lines.push(format!(
                    "{stamp}  {}  {}",
                    ctx.out.paint(&ctx.short_name(path), Style::Type),
                    value::compact(value, 120)
                ));
            }
        }
        let added = list("added");
        if !added.is_empty() {
            lines.push(format!("{stamp}  added {}", added.join(", ")));
        }
        let removed = list("removed");
        if !removed.is_empty() {
            lines.push(format!("{stamp}  removed {}", removed.join(", ")));
        }
        let mut data = json!({ "t": t });
        if let (Value::Object(data), Value::Object(frame)) = (&mut data, &frame) {
            data.extend(frame.clone());
        }
        let human = lines.join("\n");
        ctx.out.event(data, || human);
        1
    })?;
    drop(watch);
    let human = ctx
        .out
        .paint(&format!("{events} updates ({})", stop.name()), Style::Dim);
    Ok(Outcome::new(
        json!({ "entity": entity, "count": events, "stopped": stop.name() }),
        human,
    ))
}

/// `hearth observe`.
pub fn observe(ctx: &mut Ctx, args: ObserveArgs) -> Result<Outcome, CliError> {
    let event = ctx.type_path(&args.event, Want::Event)?;
    let entity = match &args.entity {
        Some(text) => Some(ctx.entity(text)?),
        None => None,
    };
    let (count, duration) = ctx.stream_limits(args.stream.count, args.stream.duration);
    let mut params = json!({ "event": event });
    if let Some(entity) = &entity {
        params["entity"] = entity.clone();
    }
    let mut watch = ctx.client()?.watch("world.observe+watch", Some(params))?;
    let short = ctx.short_name(&event);
    ctx.out.info(format!(
        "Observing {short}{}{}",
        entity
            .as_ref()
            .map(|entity| format!(" on {}", entity_text(entity)))
            .unwrap_or_default(),
        if count.is_none() && duration.is_none() {
            " (Ctrl+C to stop)"
        } else {
            ""
        }
    ));
    let (events, stop) = drive(
        ctx,
        &mut watch,
        count,
        duration,
        |ctx, frame, t, remaining| {
            let t = seconds(t);
            let events = match frame {
                Value::Array(events) => events,
                Value::Null => Vec::new(),
                other => vec![other],
            };
            let mut emitted = 0;
            for event in events
                .into_iter()
                .take(usize::try_from(remaining).unwrap_or(usize::MAX))
            {
                let human = format!(
                    "{}  {}  {}",
                    ctx.out.paint(&format!("+{t:.2}s"), Style::Dim),
                    ctx.out.paint(&short, Style::Type),
                    value::compact(&event, 120)
                );
                ctx.out.event(json!({ "t": t, "event": event }), || human);
                emitted += 1;
            }
            emitted
        },
    )?;
    drop(watch);
    let human = ctx
        .out
        .paint(&format!("{events} events ({})", stop.name()), Style::Dim);
    Ok(Outcome::new(
        json!({ "event": event, "entity": entity, "count": events, "stopped": stop.name() }),
        human,
    ))
}
