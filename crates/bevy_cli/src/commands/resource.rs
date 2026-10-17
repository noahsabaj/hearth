//! `hearth resource`: listing, reading and changing resources.

use serde_json::{json, Value};

use super::split_path;
use super::world::with_schema_hint;
use crate::cli::ResourceCommand;
use crate::context::Ctx;
use crate::output::{CliError, Outcome, Style};
use crate::registry::Want;
use crate::value;

/// Runs a `hearth resource` command.
pub fn run(ctx: &mut Ctx, command: ResourceCommand) -> Result<Outcome, CliError> {
    match command {
        ResourceCommand::List { grep } => list(ctx, grep.as_deref()),
        ResourceCommand::Get { resource } => get(ctx, &resource),
        ResourceCommand::Set { target, value } => set(ctx, &target, &value),
        ResourceCommand::Insert { resource, value } => insert(ctx, &resource, value.as_deref()),
        ResourceCommand::Remove { resource } => remove(ctx, &resource),
    }
}

fn list(ctx: &mut Ctx, grep: Option<&str>) -> Result<Outcome, CliError> {
    let result = ctx.call("world.list_resources", Value::Null)?;
    let mut paths: Vec<String> = result
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect();
    if let Some(grep) = grep {
        let grep = grep.to_lowercase();
        paths.retain(|path| path.to_lowercase().contains(&grep));
    }
    paths.sort_by_key(|path| crate::registry::shorten_path(path).to_lowercase());
    let rows: Vec<Vec<String>> = paths
        .iter()
        .map(|path| {
            vec![
                ctx.out
                    .paint(&crate::registry::shorten_path(path), Style::Type),
                ctx.out.paint(path, Style::Dim),
            ]
        })
        .collect();
    let human = if rows.is_empty() {
        "No resources match.".to_string()
    } else {
        crate::output::table(ctx.out.color, &["RESOURCE", "TYPE PATH"], &rows)
    };
    Ok(Outcome::new(json!(paths), human))
}

fn get(ctx: &mut Ctx, name: &str) -> Result<Outcome, CliError> {
    let resource = ctx.type_path(name, Want::Resource)?;
    let result = ctx.call("world.get_resources", json!({ "resource": resource }))?;
    let value = result.get("value").cloned().unwrap_or(Value::Null);
    let human = format!(
        "{}\n{}",
        ctx.out.paint(&ctx.short_name(&resource), Style::Bold),
        serde_json::to_string_pretty(&value).unwrap_or_default()
    );
    Ok(Outcome::new(
        json!({ "resource": resource, "value": value }),
        human,
    ))
}

fn set(ctx: &mut Ctx, target: &str, text: &str) -> Result<Outcome, CliError> {
    let (name, path) = split_path(target);
    let resource = ctx.type_path(name, Want::Resource)?;
    let value = value::parse(text)?;
    if let Err(error) = ctx.call(
        "world.mutate_resources",
        json!({ "resource": resource, "path": path, "value": value }),
    ) {
        return Err(with_schema_hint(ctx, error, &resource));
    }
    let human = format!("Set {}{path}", ctx.short_name(&resource));
    Ok(Outcome::new(
        json!({ "resource": resource, "path": path, "value": value }),
        human,
    ))
}

fn insert(ctx: &mut Ctx, name: &str, text: Option<&str>) -> Result<Outcome, CliError> {
    let resource = ctx.type_path(name, Want::Resource)?;
    let value = match text {
        Some(text) => value::parse(text)?,
        None => {
            let example = ctx.registry()?.example(&resource);
            if example.is_null() {
                return Err(CliError::usage(format!(
                    "`{}` has no default value, so pass one",
                    ctx.short_name(&resource)
                )));
            }
            example
        }
    };
    if let Err(error) = ctx.call(
        "world.insert_resources",
        json!({ "resource": resource, "value": value }),
    ) {
        return Err(with_schema_hint(ctx, error, &resource));
    }
    let human = format!("Inserted {}", ctx.short_name(&resource));
    Ok(Outcome::new(
        json!({ "resource": resource, "value": value }),
        human,
    ))
}

fn remove(ctx: &mut Ctx, name: &str) -> Result<Outcome, CliError> {
    let resource = ctx.type_path(name, Want::Resource)?;
    ctx.call("world.remove_resources", json!({ "resource": resource }))?;
    let human = format!("Removed {}", ctx.short_name(&resource));
    Ok(Outcome::new(json!({ "resource": resource }), human))
}
