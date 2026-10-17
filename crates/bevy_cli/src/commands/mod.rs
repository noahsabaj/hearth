//! The commands. Each takes the context and its arguments and returns an [`Outcome`].

mod agent;
mod app;
mod input;
mod project;
mod resource;
mod screenshot;
mod stream;
mod types;
mod world;

use serde_json::{Map, Value};

use crate::cli::Command;
use crate::context::Ctx;
use crate::known;
use crate::output::{CliError, Outcome};
use crate::registry::shorten_path;

#[cfg(test)]
pub use agent::ERROR_CODES;
pub use agent::{manifest, ArgInfo, CommandInfo};
pub use screenshot::display_path;

/// Runs a command.
pub fn dispatch(ctx: &mut Ctx, command: Command) -> Result<Outcome, CliError> {
    match command {
        Command::New(args) => project::new(ctx, args),
        Command::Build(args) => project::build(ctx, args, false),
        Command::Check(args) => project::build(ctx, args, true),
        Command::Run(args) => project::run(ctx, args),
        Command::Ps(args) => project::ps(ctx, args),
        Command::Logs(args) => project::logs(ctx, args),
        Command::Stop(args) => project::stop(ctx, args),
        Command::Examples(args) => project::examples(ctx, args),
        Command::Doctor(args) => project::doctor(ctx, args),
        Command::Status => app::status(ctx),
        Command::Tree(args) => world::tree(ctx, args),
        Command::Find(args) => world::find(ctx, args),
        Command::Get(args) => world::get(ctx, args),
        Command::Set(args) => world::set(ctx, args),
        Command::Insert(args) => world::insert(ctx, args),
        Command::Remove(args) => world::remove(ctx, args),
        Command::Spawn(args) => world::spawn(ctx, args),
        Command::Despawn(args) => world::despawn(ctx, args),
        Command::Reparent(args) => world::reparent(ctx, args),
        Command::Query(args) => world::query(ctx, args),
        Command::Resource(command) => resource::run(ctx, command),
        Command::Types(args) => types::types(ctx, args),
        Command::Schema(args) => types::schema(ctx, args),
        Command::Watch(args) => stream::watch(ctx, args),
        Command::Observe(args) => stream::observe(ctx, args),
        Command::Trigger(args) => world::trigger(ctx, args),
        Command::Send(args) => world::send(ctx, args),
        Command::Screenshot(args) => screenshot::screenshot(ctx, args),
        Command::Input(command) => input::run(ctx, command),
        Command::Time(args) => app::time(ctx, args),
        Command::Schedules(args) => app::schedules(ctx, args),
        Command::Diagnostics(args) => app::diagnostics(ctx, args),
        Command::Methods => app::methods(ctx),
        Command::Call(args) => app::call(ctx, args),
        Command::Exit(args) => app::exit(ctx, args),
        Command::Commands(args) => agent::commands(ctx, args),
        Command::Skill(command) => agent::skill(ctx, command),
        Command::Mcp(args) => crate::mcp::serve(ctx, args),
    }
}

/// Splits `Transform.translation.x` into the type, `Transform`, and the reflection path,
/// `.translation.x`. Dots inside generics such as `Time<Virtual>` don't split.
pub fn split_path(target: &str) -> (&str, String) {
    let mut depth = 0usize;
    for (i, c) in target.char_indices() {
        match c {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            '.' | '[' if depth == 0 && i > 0 => {
                let path = &target[i..];
                return (&target[..i], path.to_string());
            }
            _ => {}
        }
    }
    (target, String::new())
}

/// What to call an entity: its name, or the most telling of its components in parentheses.
pub fn label_of(components: &Map<String, Value>) -> String {
    if let Some(name) = components
        .get(known::NAME)
        .and_then(crate::context::name_text)
    {
        return name;
    }
    let shorts: Vec<String> = components.keys().map(|path| shorten_path(path)).collect();
    if let Some(label) = known::LABELS
        .iter()
        .find(|label| shorts.iter().any(|short| short == *label))
    {
        return format!("({label})");
    }
    match shorts
        .iter()
        .filter(|short| !known::GENERIC.contains(&short.as_str()))
        .min()
    {
        Some(short) => format!("({short})"),
        None => "(entity)".to_string(),
    }
}

/// Orders entity ids by index, then generation.
pub fn entity_order(entity: &Value) -> (u64, u64) {
    match entity {
        Value::String(text) => {
            let (index, generation) = text.split_once('v').unwrap_or((text, "0"));
            (
                index.parse().unwrap_or(u64::MAX),
                generation.parse().unwrap_or(0),
            )
        }
        Value::Number(bits) => {
            let bits = bits.as_u64().unwrap_or(u64::MAX);
            (bits & 0xFFFF_FFFF, bits >> 32)
        }
        _ => (u64::MAX, 0),
    }
}

/// Matches `text` against a pattern, ignoring case: `*` matches any text and `?` one
/// character. A pattern without either matches when `text` contains it.
pub fn matches_pattern(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.to_lowercase().chars().collect();
    let text: Vec<char> = text.to_lowercase().chars().collect();
    if !pattern.iter().any(|&c| c == '*' || c == '?') {
        let needle: String = pattern.iter().collect();
        let haystack: String = text.iter().collect();
        return haystack.contains(&needle);
    }
    // Iterative wildcard matching with backtracking to the last `*`.
    let (mut p, mut t) = (0, 0);
    let (mut star, mut mark) = (None, 0);
    while t < text.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == text[t]) {
            p += 1;
            t += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = Some(p);
            mark = t;
            p += 1;
        } else if let Some(star) = star {
            p = star + 1;
            mark += 1;
            t = mark;
        } else {
            return false;
        }
    }
    pattern[p..].iter().all(|&c| c == '*')
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn splits_paths() {
        assert_eq!(split_path("Transform"), ("Transform", String::new()));
        assert_eq!(
            split_path("Transform.translation.x"),
            ("Transform", ".translation.x".to_string())
        );
        assert_eq!(
            split_path("Time<Virtual>.context.paused"),
            ("Time<Virtual>", ".context.paused".to_string())
        );
        assert_eq!(split_path("my::Grid[3]"), ("my::Grid", "[3]".to_string()));
    }

    #[test]
    fn matches_patterns() {
        assert!(matches_pattern("cube", "Big Cube"));
        assert!(matches_pattern("enemy*", "Enemy 12"));
        assert!(!matches_pattern("enemy*", "Boss Enemy"));
        assert!(matches_pattern("*enemy*", "Boss Enemy"));
        assert!(matches_pattern("e?emy", "enemy"));
        assert!(!matches_pattern("e?emy", "eenemy"));
    }

    #[test]
    fn labels_entities() {
        let named = json!({ "bevy_ecs::name::Name": "Cube" });
        assert_eq!(label_of(named.as_object().unwrap()), "Cube");
        let camera = json!({
            "bevy_transform::components::transform::Transform": {},
            "bevy_camera::components::Camera3d": {}
        });
        assert_eq!(label_of(camera.as_object().unwrap()), "(Camera3d)");
        let other =
            json!({ "bevy_transform::components::transform::Transform": {}, "my::Health": 1 });
        assert_eq!(label_of(other.as_object().unwrap()), "(Health)");
        assert_eq!(label_of(&Map::new()), "(entity)");
    }

    #[test]
    fn orders_entities() {
        let mut ids = vec![json!("10v0"), json!("2v1"), json!("2v0")];
        ids.sort_by_key(entity_order);
        assert_eq!(ids, vec![json!("2v0"), json!("2v1"), json!("10v0")]);
    }
}
