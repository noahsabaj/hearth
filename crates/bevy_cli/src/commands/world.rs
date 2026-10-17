//! Entity commands: the hierarchy, finding entities, reading and changing their components,
//! and triggering events and messages.

use std::collections::{HashMap, HashSet};

use serde_json::{json, Map, Value};

use super::{entity_order, label_of, matches_pattern, split_path};
use crate::cli::{
    DespawnArgs, FindArgs, GetArgs, InsertArgs, QueryArgs, RemoveArgs, ReparentArgs, SendArgs,
    SetArgs, SpawnArgs, TreeArgs, TriggerArgs,
};
use crate::context::{entity_text, name_text, Ctx};
use crate::known;
use crate::output::{CliError, Exit, Outcome, Style};
use crate::registry::Want;
use crate::value;

/// How many characters of a value human output shows on one line.
pub const VALUE_WIDTH: usize = 100;

/// An entity returned by `world.query`.
pub struct Row {
    /// The entity id.
    pub entity: Value,
    /// Component values, by type path.
    pub components: Map<String, Value>,
    /// Whether each `has` component is present, by type path.
    pub has: Map<String, Value>,
}

impl Row {
    /// The entity's name, when the query fetched it.
    pub fn name(&self) -> Option<String> {
        self.components.get(known::NAME).and_then(name_text)
    }
}

/// Runs `world.query`.
pub fn run_query(ctx: &mut Ctx, params: Value) -> Result<Vec<Row>, CliError> {
    let Value::Array(rows) = ctx.call("world.query", params)? else {
        return Ok(Vec::new());
    };
    let take = |row: &mut Map<String, Value>, key: &str| match row.remove(key) {
        Some(Value::Object(map)) => map,
        _ => Map::new(),
    };
    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let Value::Object(mut row) = row else {
                return None;
            };
            let entity = row.remove("entity")?;
            Some(Row {
                entity,
                components: take(&mut row, "components"),
                has: take(&mut row, "has"),
            })
        })
        .collect())
}

/// Resolves several type names.
pub fn resolve_all(ctx: &mut Ctx, names: &[String], want: Want) -> Result<Vec<String>, CliError> {
    names.iter().map(|name| ctx.type_path(name, want)).collect()
}

/// How many entities hold resources, or 0 when the app doesn't store resources as entities.
fn count_resources(ctx: &mut Ctx) -> usize {
    ctx.call(
        "world.query",
        json!({ "data": {}, "filter": { "with": [known::IS_RESOURCE] }, "strict": true }),
    )
    .ok()
    .and_then(|rows| rows.as_array().map(Vec::len))
    .unwrap_or(0)
}

/// Adds the type's schema to the hint of a value the app rejected.
pub fn with_schema_hint(ctx: &mut Ctx, error: CliError, type_path: &str) -> CliError {
    if !matches!(
        error.code,
        "COMPONENT_ERROR" | "RESOURCE_ERROR" | "INVALID_PARAMS"
    ) {
        return error;
    }
    let example = ctx
        .registry()
        .map(|registry| registry.example(type_path))
        .unwrap_or(Value::Null);
    let short = ctx.short_name(type_path);
    let hint = if example.is_null() {
        format!("`hearth schema {short}` shows its fields.")
    } else {
        format!(
            "`hearth schema {short}` shows its fields. A whole value looks like: {}",
            value::compact(&example, 300)
        )
    };
    error.hint(hint)
}

/// The default value of a type, for commands that take an optional value.
fn default_value(ctx: &mut Ctx, type_path: &str) -> Result<Value, CliError> {
    let example = ctx.registry()?.example(type_path);
    if example.is_null() {
        let short = ctx.short_name(type_path);
        return Err(
            CliError::usage(format!("`{short}` has no default value, so pass one"))
                .hint(format!("`hearth schema {short}` shows an example value.")),
        );
    }
    Ok(example)
}

/// `Cube (494v0)`, or just the id when the name isn't known.
fn describe(ctx: &Ctx, entity: &Value) -> String {
    match ctx.name_of(entity) {
        Some(name) => format!("{name} ({})", entity_text(entity)),
        None => entity_text(entity),
    }
}

/// The hierarchy, linked by `ChildOf` and ordered by `Children`.
struct Forest {
    rows: Vec<Row>,
    index: HashMap<String, usize>,
    children: HashMap<String, Vec<usize>>,
    with_components: bool,
}

impl Forest {
    fn new(rows: Vec<Row>, with_components: bool) -> Self {
        let index: HashMap<String, usize> = rows
            .iter()
            .enumerate()
            .map(|(i, row)| (entity_text(&row.entity), i))
            .collect();
        let mut children: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, row) in rows.iter().enumerate() {
            if let Some(parent) = row.components.get(known::CHILD_OF).map(entity_text)
                && index.contains_key(&parent)
            {
                children.entry(parent).or_default().push(i);
            }
        }
        for (parent, kids) in &mut children {
            let order: Vec<String> = index
                .get(parent)
                .and_then(|&i| rows[i].components.get(known::CHILDREN))
                .and_then(Value::as_array)
                .map(|list| list.iter().map(entity_text).collect())
                .unwrap_or_default();
            kids.sort_by_key(|&i| {
                let id = entity_text(&rows[i].entity);
                (
                    order
                        .iter()
                        .position(|child| *child == id)
                        .unwrap_or(usize::MAX),
                    entity_order(&rows[i].entity),
                )
            });
        }
        Self {
            rows,
            index,
            children,
            with_components,
        }
    }

    /// Whether the entity's parent is in the forest.
    fn has_parent(&self, i: usize) -> bool {
        self.rows[i]
            .components
            .get(known::CHILD_OF)
            .map(entity_text)
            .is_some_and(|parent| self.index.contains_key(&parent))
    }

    fn kids(&self, i: usize) -> &[usize] {
        self.children
            .get(&entity_text(&self.rows[i].entity))
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    fn component_names(&self, i: usize) -> Vec<String> {
        let mut names: Vec<String> = self.rows[i]
            .components
            .keys()
            .map(|path| crate::registry::shorten_path(path))
            .collect();
        names.sort();
        names
    }

    fn json(&self, i: usize, depth: usize, max: Option<usize>, seen: &mut HashSet<usize>) -> Value {
        seen.insert(i);
        let row = &self.rows[i];
        let mut node = json!({
            "entity": row.entity,
            "name": row.name(),
            "label": label_of(&row.components),
        });
        if self.with_components {
            node["components"] = json!(self.component_names(i));
        }
        let kids = self.kids(i);
        if max.is_some_and(|max| depth >= max) {
            if !kids.is_empty() {
                node["children_hidden"] = json!(kids.len());
            }
        } else {
            let mut children = Vec::new();
            for &kid in kids {
                if !seen.contains(&kid) {
                    children.push(self.json(kid, depth + 1, max, seen));
                }
            }
            node["children"] = Value::Array(children);
        }
        node
    }

    fn render(
        &self,
        ctx: &Ctx,
        i: usize,
        prefix: &str,
        branch: &str,
        depth: usize,
        max: Option<usize>,
        lines: &mut Vec<String>,
        seen: &mut HashSet<usize>,
    ) {
        seen.insert(i);
        let row = &self.rows[i];
        let label = match row.name() {
            Some(name) => ctx.out.paint(&name, Style::Bold),
            None => ctx.out.paint(&label_of(&row.components), Style::Dim),
        };
        let mut line = format!(
            "{prefix}{branch}{label}  {}",
            ctx.out.paint(&entity_text(&row.entity), Style::Id)
        );
        if self.with_components {
            let names = self.component_names(i).join(", ");
            line.push_str(&format!(
                "  {}",
                ctx.out.paint(&format!("[{names}]"), Style::Dim)
            ));
        }
        lines.push(line);
        let child_prefix = format!(
            "{prefix}{}",
            match branch {
                "├── " => "│   ",
                "└── " => "    ",
                _ => "",
            }
        );
        let kids: Vec<usize> = self
            .kids(i)
            .iter()
            .copied()
            .filter(|kid| !seen.contains(kid))
            .collect();
        if max.is_some_and(|max| depth >= max) {
            if !kids.is_empty() {
                lines.push(format!(
                    "{child_prefix}└── {}",
                    ctx.out.paint(&format!("… {} more", kids.len()), Style::Dim)
                ));
            }
            return;
        }
        for (n, &kid) in kids.iter().enumerate() {
            let branch = if n + 1 == kids.len() {
                "└── "
            } else {
                "├── "
            };
            self.render(ctx, kid, &child_prefix, branch, depth + 1, max, lines, seen);
        }
    }
}

/// `hearth tree`.
pub fn tree(ctx: &mut Ctx, args: TreeArgs) -> Result<Outcome, CliError> {
    let root = match &args.root {
        Some(text) => Some(ctx.entity(text)?),
        None => None,
    };
    let filter = if args.all {
        json!({})
    } else {
        json!({ "without": [known::IS_RESOURCE] })
    };
    let rows = run_query(
        ctx,
        json!({ "data": { "option": "all" }, "filter": filter }),
    )?;
    let resources = if args.all { 0 } else { count_resources(ctx) };
    let forest = Forest::new(rows, args.components);

    let mut roots: Vec<usize> = match &root {
        Some(root) => {
            let found = forest.rows.iter().position(|row| &row.entity == root);
            let Some(found) = found else {
                return Err(CliError::new(
                    "ENTITY_NOT_FOUND",
                    Exit::Rejected,
                    format!("Entity {} isn't in the world", entity_text(root)),
                ));
            };
            vec![found]
        }
        None => (0..forest.rows.len())
            .filter(|&i| !forest.has_parent(i))
            .collect(),
    };
    // Observers and systems have no reflected components; resources are filtered out above.
    let mut internal = 0;
    if root.is_none() && !args.all {
        roots.retain(|&i| {
            let keep = !forest.rows[i].components.is_empty();
            if !keep {
                internal += 1;
            }
            keep
        });
    }
    roots.sort_by_key(|&i| entity_order(&forest.rows[i].entity));

    let mut seen = HashSet::new();
    let nodes: Vec<Value> = roots
        .iter()
        .map(|&i| forest.json(i, 0, args.depth, &mut seen))
        .collect();
    let shown = seen.len();

    let mut lines = Vec::new();
    let mut seen = HashSet::new();
    for &i in &roots {
        forest.render(ctx, i, "", "", 0, args.depth, &mut lines, &mut seen);
    }
    if lines.is_empty() {
        lines.push("No entities.".to_string());
    }
    if internal > 0 || resources > 0 {
        lines.push(ctx.out.paint(
            &format!(
                "({internal} internal entities and {resources} resource entities hidden; --all shows them)"
            ),
            Style::Dim,
        ));
    }
    Ok(Outcome::new(
        json!({
            "roots": nodes,
            "count": shown,
            "hidden": { "internal": internal, "resources": resources },
        }),
        lines.join("\n"),
    ))
}

/// `hearth find`.
pub fn find(ctx: &mut Ctx, args: FindArgs) -> Result<Outcome, CliError> {
    let with = resolve_all(ctx, &args.with, Want::Component)?;
    let mut without = resolve_all(ctx, &args.without, Want::Component)?;
    if !args.all {
        without.push(known::IS_RESOURCE.to_string());
    }
    let data = if args.pattern.is_some() {
        json!({ "components": [known::NAME] })
    } else {
        json!({ "option": "all" })
    };
    let mut rows = run_query(
        ctx,
        json!({ "data": data, "filter": { "with": with, "without": without } }),
    )?;
    match &args.pattern {
        Some(pattern) => rows.retain(|row| {
            row.name()
                .is_some_and(|name| matches_pattern(pattern, &name))
        }),
        None if !args.all && with.is_empty() => rows.retain(|row| !row.components.is_empty()),
        None => {}
    }
    rows.sort_by_key(|row| entity_order(&row.entity));
    let total = rows.len();
    if let Some(limit) = args.limit
        && limit < total
    {
        rows.truncate(limit);
        ctx.out.warn(format!("Showing {limit} of {total} entities"));
    }
    let items: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({ "entity": row.entity, "name": row.name(), "label": label_of(&row.components) })
        })
        .collect();
    let human = if rows.is_empty() {
        "No entities match.".to_string()
    } else {
        rows.iter()
            .map(|row| {
                format!(
                    "{}  {}",
                    ctx.out.paint(&entity_text(&row.entity), Style::Id),
                    match row.name() {
                        Some(name) => name,
                        None => ctx.out.paint(&label_of(&row.components), Style::Dim),
                    }
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    Ok(Outcome::new(Value::Array(items), human))
}

/// `hearth get`.
pub fn get(ctx: &mut Ctx, args: GetArgs) -> Result<Outcome, CliError> {
    let entity = ctx.entity(&args.entity)?;
    let mut components = Map::new();
    let mut unserializable = Map::new();
    let mut errors = Map::new();
    // Fields picked with a path such as `Transform.translation`, shown under that name.
    let mut fields: Vec<(String, Value)> = Vec::new();
    let mut picked: HashSet<String> = HashSet::new();
    let mut name = None;
    if args.components.is_empty() {
        if ctx.client()?.supports("world.inspect")? {
            let result = ctx.call("world.inspect", json!({ "entity": entity }))?;
            for component in result
                .get("components")
                .and_then(Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or_default()
            {
                let Some(path) = component
                    .get("type_path")
                    .or_else(|| component.get("name"))
                    .and_then(Value::as_str)
                else {
                    continue;
                };
                match component.get("serialized_value") {
                    Some(value) if !value.is_null() => {
                        components.insert(path.to_string(), value.clone());
                    }
                    _ => {
                        let debug = component.get("value").cloned().unwrap_or(Value::Null);
                        unserializable.insert(path.to_string(), debug);
                    }
                }
            }
        } else {
            let names = ctx.call("world.list_components", json!({ "entity": entity }))?;
            let result = ctx.call(
                "world.get_components",
                json!({ "entity": entity, "components": names }),
            )?;
            take_components(result, &mut components, &mut errors);
            for (path, error) in core::mem::take(&mut errors) {
                unserializable.insert(path, error);
            }
        }
    } else {
        let mut targets = Vec::new();
        for target in &args.components {
            let (component, path) = split_path(target);
            targets.push((ctx.type_path(component, Want::Component)?, path));
        }
        let mut paths: Vec<&str> = targets
            .iter()
            .map(|(component, _)| component.as_str())
            .collect();
        // The name too, for the heading. An entity without one reports an error for it.
        let asked_name = paths.contains(&known::NAME);
        paths.push(known::NAME);
        paths.sort_unstable();
        paths.dedup();
        let result = ctx.call(
            "world.get_components",
            json!({ "entity": entity, "components": paths }),
        )?;
        let mut values = Map::new();
        take_components(result, &mut values, &mut errors);
        if !asked_name {
            errors.remove(known::NAME);
            name = values.remove(known::NAME).as_ref().and_then(name_text);
        }
        for (component, path) in &targets {
            let Some(value) = values.get(component) else {
                continue;
            };
            if path.is_empty() {
                components.insert(component.clone(), value.clone());
                continue;
            }
            let found = value::field(value, path).ok_or_else(|| {
                let short = ctx.short_name(component);
                CliError::new(
                    "COMPONENT_ERROR",
                    Exit::Rejected,
                    format!("{short} has no field at `{path}`"),
                )
                .hint(format!("`hearth schema {short}` shows its fields."))
            })?;
            fields.push((
                format!("{}{path}", ctx.short_name(component)),
                found.clone(),
            ));
            picked.insert(format!("{component}{path}"));
            components.insert(format!("{component}{path}"), found.clone());
        }
    }

    if let Some(found) = components.get(known::NAME).and_then(name_text) {
        name = Some(found);
    }
    let width = if args.full { 0 } else { VALUE_WIDTH };
    let mut rows: Vec<(String, String)> = Vec::new();
    let shown = components
        .iter()
        .filter(|(path, _)| !picked.contains(path.as_str()))
        .map(|(path, value)| (ctx.short_name(path), value))
        .collect::<Vec<_>>();
    for (short, value) in shown
        .into_iter()
        .chain(fields.iter().map(|(name, value)| (name.clone(), value)))
    {
        let text = if args.full {
            serde_json::to_string_pretty(value).unwrap_or_default()
        } else {
            value::compact(value, width)
        };
        rows.push((short, text));
    }
    for (path, debug) in &unserializable {
        let debug = debug
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| debug.to_string());
        let debug = debug.split_whitespace().collect::<Vec<_>>().join(" ");
        // Debug text that only repeats the type or the reason says nothing new.
        let text = if debug.starts_with("<Unreflectable") || debug.starts_with("Reflect(") {
            "(not serializable)".to_string()
        } else {
            format!("(not serializable) {}", value::shorten(&debug, width))
        };
        rows.push((ctx.short_name(path), ctx.out.paint(&text, Style::Dim)));
    }
    for (path, error) in &errors {
        let error = error
            .get("message")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| error.to_string());
        rows.push((
            ctx.short_name(path),
            ctx.out.paint(&format!("error: {error}"), Style::Error),
        ));
    }
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    let name_width = rows
        .iter()
        .map(|(name, _)| name.chars().count())
        .max()
        .unwrap_or(0);
    let mut lines = vec![format!(
        "{}  {}",
        ctx.out
            .paint(name.as_deref().unwrap_or("Entity"), Style::Bold),
        ctx.out.paint(&entity_text(&entity), Style::Id)
    )];
    for (short, text) in rows {
        if args.full && text.contains('\n') {
            lines.push(format!("  {}", ctx.out.paint(&short, Style::Type)));
            lines.extend(text.lines().map(|line| format!("    {line}")));
        } else {
            let pad = " ".repeat(name_width - short.chars().count());
            lines.push(format!(
                "  {}{pad}  {text}",
                ctx.out.paint(&short, Style::Type)
            ));
        }
    }
    Ok(Outcome::new(
        json!({
            "entity": entity,
            "name": name,
            "components": components,
            "unserializable": unserializable,
            "errors": errors,
        }),
        lines.join("\n"),
    ))
}

/// Splits a `world.get_components` result into values and errors.
fn take_components(
    result: Value,
    components: &mut Map<String, Value>,
    errors: &mut Map<String, Value>,
) {
    let Value::Object(mut result) = result else {
        return;
    };
    match (result.remove("components"), result.remove("errors")) {
        (Some(Value::Object(values)), found_errors) => {
            components.extend(values);
            if let Some(Value::Object(found_errors)) = found_errors {
                errors.extend(found_errors);
            }
        }
        // A strict response is just the map of values.
        _ => components.extend(result),
    }
}

/// `hearth set`.
pub fn set(ctx: &mut Ctx, args: SetArgs) -> Result<Outcome, CliError> {
    let entity = ctx.entity(&args.entity)?;
    let (component, path) = split_path(&args.target);
    let component = ctx.type_path(component, Want::Component)?;
    let value = value::parse(&args.value)?;
    if let Err(error) = ctx.call(
        "world.mutate_components",
        json!({ "entity": entity, "component": component, "path": path, "value": value }),
    ) {
        return Err(with_schema_hint(ctx, error, &component));
    }
    let human = format!(
        "Set {}{path} on {}",
        ctx.short_name(&component),
        describe(ctx, &entity)
    );
    Ok(Outcome::new(
        json!({ "entity": entity, "component": component, "path": path, "value": value }),
        human,
    ))
}

/// `hearth insert`.
pub fn insert(ctx: &mut Ctx, args: InsertArgs) -> Result<Outcome, CliError> {
    let entity = ctx.entity(&args.entity)?;
    let component = ctx.type_path(&args.component, Want::Component)?;
    let value = match &args.value {
        Some(text) => value::parse(text)?,
        None => default_value(ctx, &component)?,
    };
    if let Err(error) = ctx.call(
        "world.insert_components",
        json!({ "entity": entity, "components": { component.clone(): value } }),
    ) {
        return Err(with_schema_hint(ctx, error, &component));
    }
    let human = format!(
        "Inserted {} on {}",
        ctx.short_name(&component),
        describe(ctx, &entity)
    );
    Ok(Outcome::new(
        json!({ "entity": entity, "component": component, "value": value }),
        human,
    ))
}

/// `hearth remove`.
pub fn remove(ctx: &mut Ctx, args: RemoveArgs) -> Result<Outcome, CliError> {
    let entity = ctx.entity(&args.entity)?;
    let components = resolve_all(ctx, &args.components, Want::Component)?;
    ctx.call(
        "world.remove_components",
        json!({ "entity": entity, "components": components }),
    )?;
    let shorts: Vec<String> = components.iter().map(|path| ctx.short_name(path)).collect();
    let human = format!(
        "Removed {} from {}",
        shorts.join(", "),
        describe(ctx, &entity)
    );
    Ok(Outcome::new(
        json!({ "entity": entity, "components": components }),
        human,
    ))
}

/// `hearth spawn`.
pub fn spawn(ctx: &mut Ctx, args: SpawnArgs) -> Result<Outcome, CliError> {
    let mut components = Map::new();
    if let Some(text) = &args.bundle {
        let Value::Object(given) = value::parse_json(text)? else {
            return Err(CliError::usage(
                "--bundle takes an object of type paths to values",
            ));
        };
        for (name, value) in given {
            let path = ctx.type_path(&name, Want::Component)?;
            components.insert(path, value);
        }
    }
    for spec in &args.components {
        let (name, value) = match spec.split_once('=') {
            Some((name, value)) => (name, Some(value)),
            None => (spec.as_str(), None),
        };
        let path = ctx.type_path(name, Want::Component)?;
        let value = match value {
            Some(text) => value::parse(text)?,
            None => default_value(ctx, &path)?,
        };
        components.insert(path, value);
    }
    if let Some(name) = &args.name {
        components.insert(known::NAME.to_string(), json!(name));
    }
    if let Some(parent) = &args.parent {
        let parent = ctx.entity(parent)?;
        components.insert(known::CHILD_OF.to_string(), parent);
    }
    let result = ctx.call("world.spawn_entity", json!({ "components": components }))?;
    let entity = result.get("entity").cloned().unwrap_or(Value::Null);
    let label = args.name.clone().unwrap_or_else(|| "entity".to_string());
    let human = format!(
        "Spawned {label} {}",
        ctx.out.paint(&entity_text(&entity), Style::Id)
    );
    Ok(Outcome::new(
        json!({ "entity": entity, "components": components.keys().collect::<Vec<_>>() }),
        human,
    ))
}

/// `hearth despawn`.
pub fn despawn(ctx: &mut Ctx, args: DespawnArgs) -> Result<Outcome, CliError> {
    let entities: Vec<Value> = args
        .entities
        .iter()
        .map(|text| ctx.entity(text))
        .collect::<Result<_, _>>()?;
    let mut lines = Vec::new();
    for entity in &entities {
        ctx.call("world.despawn_entity", json!({ "entity": entity }))?;
        lines.push(format!("Despawned {}", describe(ctx, entity)));
    }
    Ok(Outcome::new(
        json!({ "despawned": entities }),
        lines.join("\n"),
    ))
}

/// `hearth reparent`.
pub fn reparent(ctx: &mut Ctx, args: ReparentArgs) -> Result<Outcome, CliError> {
    let entities: Vec<Value> = args
        .entities
        .iter()
        .map(|text| ctx.entity(text))
        .collect::<Result<_, _>>()?;
    let parent = match &args.parent {
        Some(text) => ctx.entity(text)?,
        None => Value::Null,
    };
    ctx.call(
        "world.reparent_entities",
        json!({ "entities": entities, "parent": parent }),
    )?;
    let moved: Vec<String> = entities
        .iter()
        .map(|entity| describe(ctx, entity))
        .collect();
    let human = if parent.is_null() {
        format!("Made {} roots", moved.join(", "))
    } else {
        format!(
            "Moved {} under {}",
            moved.join(", "),
            describe(ctx, &parent)
        )
    };
    Ok(Outcome::new(
        json!({ "entities": entities, "parent": parent }),
        human,
    ))
}

/// `hearth query`.
pub fn query(ctx: &mut Ctx, args: QueryArgs) -> Result<Outcome, CliError> {
    let components = resolve_all(ctx, &args.components, Want::Component)?;
    let option = resolve_all(ctx, &args.option, Want::Component)?;
    let has = resolve_all(ctx, &args.has, Want::Component)?;
    let with = resolve_all(ctx, &args.with, Want::Component)?;
    let mut without = resolve_all(ctx, &args.without, Want::Component)?;
    if !args.all {
        without.push(known::IS_RESOURCE.to_string());
    }
    let mut rows = run_query(
        ctx,
        json!({
            "data": { "components": components, "option": option, "has": has },
            "filter": { "with": with, "without": without },
        }),
    )?;
    rows.sort_by_key(|row| entity_order(&row.entity));
    let total = rows.len();
    if let Some(limit) = args.limit
        && limit < total
    {
        rows.truncate(limit);
        ctx.out.warn(format!("Showing {limit} of {total} entities"));
    }

    let items: Vec<Value> = rows
        .iter()
        .map(|row| json!({ "entity": row.entity, "components": row.components, "has": row.has }))
        .collect();
    // Names make the human output readable; fetching them is one more request.
    if ctx.out.format == crate::output::Format::Human && !rows.is_empty() {
        let _ = ctx.names();
    }
    let width = if args.full { 0 } else { VALUE_WIDTH };
    let mut lines = Vec::new();
    for row in &rows {
        let id = ctx.out.paint(&entity_text(&row.entity), Style::Id);
        lines.push(match ctx.name_of(&row.entity) {
            Some(name) => format!("{id}  {}", ctx.out.paint(name, Style::Bold)),
            None => id,
        });
        let mut fields: Vec<(String, String)> = row
            .components
            .iter()
            .map(|(path, value)| (ctx.short_name(path), value::compact(value, width)))
            .chain(row.has.iter().map(|(path, present)| {
                (format!("has {}", ctx.short_name(path)), present.to_string())
            }))
            .collect();
        fields.sort();
        let name_width = fields
            .iter()
            .map(|(name, _)| name.chars().count())
            .max()
            .unwrap_or(0);
        for (name, text) in fields {
            let pad = " ".repeat(name_width - name.chars().count());
            lines.push(format!(
                "  {}{pad}  {text}",
                ctx.out.paint(&name, Style::Type)
            ));
        }
    }
    if lines.is_empty() {
        lines.push("No entities match.".to_string());
    }
    Ok(Outcome::new(Value::Array(items), lines.join("\n")))
}

/// `hearth trigger`.
pub fn trigger(ctx: &mut Ctx, args: TriggerArgs) -> Result<Outcome, CliError> {
    let event = ctx.type_path(&args.event, Want::Event)?;
    let mut params = json!({ "event": event });
    if let Some(text) = &args.value {
        params["value"] = value::parse(text)?;
    }
    if let Err(error) = ctx.call("world.trigger_event", params.clone()) {
        return Err(with_schema_hint(ctx, error, &event));
    }
    let human = format!("Triggered {}", ctx.short_name(&event));
    Ok(Outcome::new(params, human))
}

/// `hearth send`.
pub fn send(ctx: &mut Ctx, args: SendArgs) -> Result<Outcome, CliError> {
    let message = ctx.type_path(&args.message, Want::Message)?;
    let mut params = json!({ "message": message });
    if let Some(text) = &args.value {
        params["value"] = value::parse(text)?;
    }
    if let Err(error) = ctx.call("world.write_message", params.clone()) {
        return Err(with_schema_hint(ctx, error, &message));
    }
    let human = format!("Sent {}", ctx.short_name(&message));
    Ok(Outcome::new(params, human))
}
