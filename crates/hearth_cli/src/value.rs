//! Values on the command line: JSON, bare words, number lists, `@file` and `-` for stdin.

use std::io::Read;

use serde_json::Value;

use crate::output::CliError;

/// Reads the text of an argument: `@path` reads a file, `-` and `@-` read stdin, and anything
/// else is the text itself.
pub fn read_text(text: &str) -> Result<String, CliError> {
    if text == "-" || text == "@-" {
        let mut input = String::new();
        std::io::stdin()
            .read_to_string(&mut input)
            .map_err(|error| CliError::io("could not read stdin", error))?;
        return Ok(input);
    }
    if let Some(path) = text.strip_prefix('@') {
        return std::fs::read_to_string(path)
            .map_err(|error| CliError::io(format!("could not read `{path}`"), error));
    }
    Ok(text.to_string())
}

/// Parses a value argument.
///
/// JSON is used as is. Text that isn't JSON becomes a string, so `Bob` means `"Bob"`, and
/// numbers separated by commas become an array, so `1,2,3` means `[1, 2, 3]`.
pub fn parse(text: &str) -> Result<Value, CliError> {
    let from_source = text.starts_with('@') || text == "-";
    let text = read_text(text)?;
    let trimmed = text.trim();
    match serde_json::from_str::<Value>(trimmed) {
        Ok(value) => Ok(value),
        Err(error) => {
            if from_source || trimmed.starts_with(['{', '[', '"']) {
                return Err(invalid_json(trimmed, &error));
            }
            if let Some(numbers) = number_list(trimmed) {
                return Ok(Value::Array(numbers));
            }
            Ok(Value::String(text))
        }
    }
}

/// Parses an argument that must be JSON, such as raw HRP parameters.
pub fn parse_json(text: &str) -> Result<Value, CliError> {
    let text = read_text(text)?;
    serde_json::from_str(text.trim()).map_err(|error| invalid_json(text.trim(), &error))
}

fn invalid_json(text: &str, error: &serde_json::Error) -> CliError {
    let shown: String = text.chars().take(80).collect();
    CliError::usage(format!("`{shown}` isn't valid JSON: {error}")).hint(
        "Quote the whole value for your shell, such as '{\"x\": 1}'. Windows PowerShell 5 strips inner double quotes: escape them as \\\", or pass the value with @file or - (stdin).",
    )
}

/// Parses `1, 2.5, -3` as numbers.
fn number_list(text: &str) -> Option<Vec<Value>> {
    if !text.contains(',') {
        return None;
    }
    text.split(',')
        .map(|part| {
            let part = part.trim();
            part.parse::<i64>().map(Value::from).ok().or_else(|| {
                part.parse::<f64>()
                    .ok()
                    .and_then(|number| serde_json::Number::from_f64(number).map(Value::Number))
            })
        })
        .collect()
}

/// Renders a value on one line, shortened to about `width` characters unless `width` is 0.
pub fn compact(value: &Value, width: usize) -> String {
    let text = match value {
        Value::String(text) => serde_json::to_string(text).unwrap_or_default(),
        other => other.to_string(),
    };
    shorten(&text, width)
}

/// Cuts `text` to `width` characters with an ellipsis, unless `width` is 0.
pub fn shorten(text: &str, width: usize) -> String {
    if width == 0 || text.chars().count() <= width {
        return text.to_string();
    }
    let mut cut: String = text.chars().take(width.saturating_sub(1)).collect();
    cut.push('…');
    cut
}

/// Reads the field at a reflection path such as `.translation.x` or `.items[2]` from a
/// value as HRP serializes it. Vectors are arrays, so `.x` to `.w` index them, and an enum's
/// fields are read through its variant, as reflection paths do.
pub fn field<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let mut value = value;
    let mut rest = path;
    while !rest.is_empty() {
        let (segment, index) = if let Some(after) = rest.strip_prefix('[') {
            let end = after.find(']')?;
            let segment = &after[..end];
            rest = &after[end + 1..];
            (segment, true)
        } else {
            let after = rest.strip_prefix('.')?;
            let end = after.find(['.', '[']).unwrap_or(after.len());
            rest = &after[end..];
            (&after[..end], false)
        };
        value = step(value, segment, index)?;
    }
    Some(value)
}

fn step<'a>(value: &'a Value, segment: &str, index: bool) -> Option<&'a Value> {
    match value {
        Value::Array(items) => {
            let position = match segment {
                "x" | "r" if !index => 0,
                "y" | "g" if !index => 1,
                "z" | "b" if !index => 2,
                "w" | "a" if !index => 3,
                _ => segment.parse().ok()?,
            };
            items.get(position)
        }
        Value::Object(fields) => match fields.get(segment) {
            Some(found) => Some(found),
            // `{"Variant": {...}}`: the path names a field of the variant.
            None if fields.len() == 1 => step(fields.values().next()?, segment, index),
            None => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_values() {
        assert_eq!(parse("2.5").unwrap(), json!(2.5));
        assert_eq!(parse("true").unwrap(), json!(true));
        assert_eq!(parse("[1, 2]").unwrap(), json!([1, 2]));
        assert_eq!(parse("{\"a\": 1}").unwrap(), json!({ "a": 1 }));
        assert_eq!(parse("\"quoted\"").unwrap(), json!("quoted"));
        assert_eq!(parse("Hidden").unwrap(), json!("Hidden"));
        assert_eq!(parse("two words").unwrap(), json!("two words"));
        assert_eq!(parse("1, 2.5,-3").unwrap(), json!([1, 2.5, -3]));
        assert_eq!(parse("a,b").unwrap(), json!("a,b"));
        assert!(parse("{\"a\": ").is_err());
        assert!(parse("[1, 2").is_err());
    }

    #[test]
    fn shortens_values() {
        assert_eq!(compact(&json!("abc"), 0), "\"abc\"");
        assert_eq!(compact(&json!([1, 2, 3, 4]), 6), "[1,2,…");
        assert_eq!(shorten("short", 10), "short");
    }

    #[test]
    fn reads_fields() {
        let transform = json!({ "translation": [1.0, 2.0, 3.0], "scale": [1.0, 1.0, 1.0] });
        assert_eq!(
            field(&transform, ".translation"),
            Some(&json!([1.0, 2.0, 3.0]))
        );
        assert_eq!(field(&transform, ".translation.y"), Some(&json!(2.0)));
        assert_eq!(field(&transform, ".translation[2]"), Some(&json!(3.0)));
        assert_eq!(field(&transform, ".rotation"), None);
        let projection = json!({ "Perspective": { "fov": 0.78 } });
        assert_eq!(field(&projection, ".fov"), Some(&json!(0.78)));
        assert_eq!(field(&json!([[1, 2]]), ".0.1"), Some(&json!(2)));
    }
}
