//! `hearth types` and `hearth schema`: what the app registered for reflection.

use serde_json::{json, Value};

use crate::cli::{SchemaArgs, TypeKind, TypesArgs};
use crate::context::Ctx;
use crate::output::{table, CliError, Outcome, Style};
use crate::registry::{type_ref, TypeInfo, Want};
use crate::value;

/// How many types human output lists without `--limit`.
const HUMAN_LIMIT: usize = 200;

fn summary(info: &TypeInfo) -> Value {
    json!({
        "path": info.path,
        "short": info.short,
        "crate": info.crate_name,
        "kind": info.kind,
        "roles": info.roles(),
        "reflect": info.reflect,
    })
}

/// `hearth types`.
pub fn types(ctx: &mut Ctx, args: TypesArgs) -> Result<Outcome, CliError> {
    let color = ctx.out.color;
    let human_output = ctx.out.format == crate::output::Format::Human;
    let registry = ctx.registry()?;
    let pattern = args.pattern.as_deref().map(str::to_lowercase);
    let mut found: Vec<&TypeInfo> = registry
        .types
        .iter()
        .filter(|info| match args.kind {
            TypeKind::Component => info.reflects("Component"),
            TypeKind::Resource => info.reflects("Resource"),
            TypeKind::Event => info.reflects("Event"),
            TypeKind::All => true,
        })
        .filter(|info| {
            args.crate_name
                .as_ref()
                .is_none_or(|name| &info.crate_name == name)
        })
        .filter(|info| {
            pattern
                .as_ref()
                .is_none_or(|pattern| info.path.to_lowercase().contains(pattern))
        })
        .collect();
    found.sort_by(|a, b| {
        a.short
            .to_lowercase()
            .cmp(&b.short.to_lowercase())
            .then(a.path.cmp(&b.path))
    });
    let total = found.len();
    let limit = args.limit.or(human_output.then_some(HUMAN_LIMIT));
    let mut notes = Vec::new();
    if let Some(limit) = limit
        && limit < total
    {
        found.truncate(limit);
        notes.push(format!("Showing {limit} of {total} types"));
    }
    let data: Vec<Value> = found.iter().map(|info| summary(info)).collect();
    let rows: Vec<Vec<String>> = found
        .iter()
        .map(|info| {
            vec![
                crate::output::paint(color, &info.short, Style::Type),
                info.roles().join(","),
                crate::output::paint(color, &info.path, Style::Dim),
            ]
        })
        .collect();
    let human = if rows.is_empty() {
        "No types match.".to_string()
    } else {
        table(color, &["TYPE", "USE", "PATH"], &rows)
    };
    for note in notes {
        ctx.out.warn(note);
    }
    Ok(Outcome::new(Value::Array(data), human))
}

/// `hearth schema`.
pub fn schema(ctx: &mut Ctx, args: SchemaArgs) -> Result<Outcome, CliError> {
    let color = ctx.out.color;
    let registry = ctx.registry()?;
    let info = registry.resolve(&args.type_name, Want::Any)?;
    let example = registry.example(&info.path);
    let schema = &info.schema;

    let type_name = |field: &Value| {
        type_ref(field)
            .map(|path| registry.short_name(path))
            .unwrap_or_else(|| "?".to_string())
    };
    let mut fields: Vec<(String, String, u64)> = schema
        .get("properties")
        .and_then(Value::as_object)
        .map(|properties| {
            properties
                .iter()
                .map(|(name, field)| {
                    (
                        name.clone(),
                        type_name(field),
                        field.get("fieldIndex").and_then(Value::as_u64).unwrap_or(0),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    fields.sort_by_key(|(_, _, index)| *index);
    let items: Vec<String> = schema
        .get("prefixItems")
        .and_then(Value::as_array)
        .map(|items| items.iter().map(type_name).collect())
        .unwrap_or_default();
    let variants: Vec<String> = schema
        .get("oneOf")
        .and_then(Value::as_array)
        .map(|variants| {
            variants
                .iter()
                .map(|variant| match variant {
                    Value::String(name) => name.clone(),
                    other => {
                        let name = other
                            .get("shortPath")
                            .and_then(Value::as_str)
                            .unwrap_or("?");
                        let inner: Vec<String> = other
                            .get("prefixItems")
                            .and_then(Value::as_array)
                            .map(|items| items.iter().map(type_name).collect())
                            .unwrap_or_default();
                        let fields: Vec<String> = other
                            .get("properties")
                            .and_then(Value::as_object)
                            .map(|fields| {
                                fields
                                    .iter()
                                    .map(|(field, ty)| format!("{field}: {}", type_name(ty)))
                                    .collect()
                            })
                            .unwrap_or_default();
                        if !inner.is_empty() {
                            format!("{name}({})", inner.join(", "))
                        } else if !fields.is_empty() {
                            format!("{name} {{ {} }}", fields.join(", "))
                        } else {
                            name.to_string()
                        }
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    let element = schema
        .get("items")
        .filter(|items| items.is_object())
        .map(type_name);

    let mut lines = vec![
        format!(
            "{}  {}",
            crate::output::paint(color, &info.short, Style::Bold),
            crate::output::paint(color, &info.path, Style::Dim)
        ),
        format!(
            "{} {}{}",
            info.kind,
            if info.roles().is_empty() {
                "type".to_string()
            } else {
                info.roles().join(", ")
            },
            if info.reflect.is_empty() {
                String::new()
            } else {
                format!(" (reflects {})", info.reflect.join(", "))
            }
        ),
    ];
    if !fields.is_empty() {
        lines.push("Fields:".to_string());
        let width = fields
            .iter()
            .map(|(name, _, _)| name.len())
            .max()
            .unwrap_or(0);
        for (name, ty, _) in &fields {
            lines.push(format!(
                "  {name:width$}  {}",
                crate::output::paint(color, ty, Style::Type)
            ));
        }
    }
    if !items.is_empty() {
        lines.push(format!("Fields: ({})", items.join(", ")));
    }
    if let Some(element) = &element {
        lines.push(format!("Items: {element}"));
    }
    if !variants.is_empty() {
        lines.push("Variants:".to_string());
        lines.extend(variants.iter().map(|variant| format!("  {variant}")));
    }
    if !example.is_null() {
        lines.push(format!("Example value:\n  {}", value::compact(&example, 0)));
    }
    let data = json!({
        "path": info.path,
        "short": info.short,
        "kind": info.kind,
        "crate": info.crate_name,
        "roles": info.roles(),
        "reflect": info.reflect,
        "fields": fields.iter().map(|(name, ty, _)| json!({ "name": name, "type": ty })).collect::<Vec<_>>(),
        "items": items,
        "variants": variants,
        "example": example,
        "schema": schema,
    });
    Ok(Outcome::new(data, lines.join("\n")))
}
