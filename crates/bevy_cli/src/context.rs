//! What a command works with: the output, the target app, and caches of what it fetched.

use std::time::Duration;

use serde_json::{json, Value};

use crate::brp::Client;
use crate::known;
use crate::output::{CliError, Exit, Output};
use crate::registry::{Registry, Want};
use crate::target::{self, Target};

/// Limits that apply when commands run as tools of the MCP server.
#[derive(Clone, Copy, Debug)]
pub struct McpLimits {
    /// How long a stream without `--count` or `--duration` runs.
    pub stream_duration: Duration,
    /// The largest side of images returned to the client.
    pub image_size: u32,
}

/// Content a command produced for the MCP client, such as a screenshot.
#[derive(Clone, Debug)]
pub struct Attachment {
    /// The bytes.
    pub data: Vec<u8>,
    /// Their media type.
    pub mime: &'static str,
}

/// An entity with a `Name`.
#[derive(Clone, Debug)]
pub struct Named {
    /// The entity id, as the app writes it.
    pub entity: Value,
    /// The name.
    pub name: String,
}

/// What a command works with.
pub struct Ctx {
    /// Where results go.
    pub out: Output,
    /// The `--app` flag.
    pub app: Option<String>,
    /// How long to wait for each answer.
    pub timeout: Duration,
    /// Set when running as an MCP tool.
    pub mcp: Option<McpLimits>,
    /// Files to return to the MCP client.
    pub attachments: Vec<Attachment>,
    target: Option<Target>,
    client: Option<Client>,
    registry: Option<Registry>,
    names: Option<Vec<Named>>,
}

impl Ctx {
    /// Creates a context. Nothing is resolved or fetched until a command needs it.
    pub fn new(out: Output, app: Option<String>, timeout: Duration) -> Self {
        Self {
            out,
            app,
            timeout,
            mcp: None,
            attachments: Vec::new(),
            target: None,
            client: None,
            registry: None,
            names: None,
        }
    }

    /// The app this command talks to.
    pub fn target(&mut self) -> Result<&Target, CliError> {
        if self.target.is_none() {
            self.target = Some(target::resolve(self.app.as_deref())?);
        }
        Ok(self.target.as_ref().expect("resolved above"))
    }

    /// The client for the target app.
    pub fn client(&mut self) -> Result<&Client, CliError> {
        if self.client.is_none() {
            let timeout = self.timeout;
            let target = self.target()?;
            let mut client = Client::new(target.endpoint.clone(), timeout);
            client.unreachable_hint = target.unreachable_hint();
            self.client = Some(client);
        }
        Ok(self.client.as_ref().expect("created above"))
    }

    /// Calls a BRP method on the target app.
    pub fn call(&mut self, method: &str, params: Value) -> Result<Value, CliError> {
        let params = (!params.is_null()).then_some(params);
        self.client()?.call(method, params)
    }

    /// The app's type registry, fetched once.
    pub fn registry(&mut self) -> Result<&Registry, CliError> {
        if self.registry.is_none() {
            let registry = Registry::fetch(self.client()?)?;
            self.registry = Some(registry);
        }
        Ok(self.registry.as_ref().expect("fetched above"))
    }

    /// Resolves a type name to its full path.
    pub fn type_path(&mut self, name: &str, want: Want) -> Result<String, CliError> {
        Ok(self.registry()?.resolve(name, want)?.path.clone())
    }

    /// The short name of a type path.
    pub fn short_name(&self, path: &str) -> String {
        match &self.registry {
            Some(registry) => registry.short_name(path),
            None => crate::registry::shorten_path(path),
        }
    }

    /// Resolves an entity argument: an id such as `495v0`, a bare index such as `495` (meaning
    /// generation 0), or a name. Prefix a name with `name:` when it looks like an id.
    pub fn entity(&mut self, text: &str) -> Result<Value, CliError> {
        let text = text.trim();
        if let Some(name) = text.strip_prefix("name:") {
            return self.entity_named(name);
        }
        if is_entity_id(text) {
            return Ok(json!(text));
        }
        if let Ok(index) = text.parse::<u64>() {
            return Ok(if self.client()?.is_modern()? {
                json!(format!("{index}v0"))
            } else {
                json!(index)
            });
        }
        self.entity_named(text)
    }

    /// The entities with a `Name`, fetched once.
    pub fn names(&mut self) -> Result<&[Named], CliError> {
        if self.names.is_none() {
            let rows = self.call(
                "world.query",
                json!({ "data": { "components": [known::NAME] } }),
            )?;
            let names = rows
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or_default()
                .iter()
                .filter_map(|row| {
                    Some(Named {
                        entity: row.get("entity")?.clone(),
                        name: name_text(row.get("components")?.get(known::NAME)?)?,
                    })
                })
                .collect();
            self.names = Some(names);
        }
        Ok(self.names.as_deref().unwrap_or_default())
    }

    /// The name of an entity, if it has one and the names were fetched.
    pub fn name_of(&self, entity: &Value) -> Option<&str> {
        self.names
            .as_ref()?
            .iter()
            .find(|named| &named.entity == entity)
            .map(|named| named.name.as_str())
    }

    fn entity_named(&mut self, name: &str) -> Result<Value, CliError> {
        let names = self.names()?;
        let exact: Vec<&Named> = names.iter().filter(|named| named.name == name).collect();
        let matches = if exact.is_empty() {
            let lower = name.to_lowercase();
            names
                .iter()
                .filter(|named| named.name.to_lowercase() == lower)
                .collect()
        } else {
            exact
        };
        match matches.as_slice() {
            [one] => Ok(one.entity.clone()),
            [] => {
                let lower = name.to_lowercase();
                let similar: Vec<String> = names
                    .iter()
                    .filter(|named| named.name.to_lowercase().contains(&lower))
                    .take(12)
                    .map(|named| format!("{} {}", entity_text(&named.entity), named.name))
                    .collect();
                Err(CliError::new(
                    "ENTITY_NOT_FOUND",
                    Exit::Rejected,
                    format!("No entity is named `{name}`"),
                )
                .hint(if similar.is_empty() {
                    "Pass an entity id such as 495v0, or a name from `hearth tree`."
                } else {
                    "Did you mean one of these?"
                })
                .data(json!({ "candidates": similar })))
            }
            many => Err(CliError::new(
                "AMBIGUOUS_ENTITY",
                Exit::Rejected,
                format!("{} entities are named `{name}`", many.len()),
            )
            .hint("Pass the entity id instead.")
            .data(json!({
                "candidates": many
                    .iter()
                    .map(|named| format!("{} {}", entity_text(&named.entity), named.name))
                    .collect::<Vec<_>>(),
            }))),
        }
    }

    /// The limit on a stream: what the command asked for, else the MCP default.
    pub fn stream_limits(
        &mut self,
        count: Option<u64>,
        duration: Option<f64>,
    ) -> (Option<u64>, Option<Duration>) {
        let duration = duration.map(Duration::from_secs_f64);
        match (self.mcp, count, duration) {
            (Some(limits), None, None) => {
                self.out.warn(format!(
                    "No --count or --duration was given, so this stream stops after {:.0}s",
                    limits.stream_duration.as_secs_f64()
                ));
                (None, Some(limits.stream_duration))
            }
            _ => (count, duration),
        }
    }
}

/// Whether `text` is an entity id written as `<index>v<generation>`.
pub fn is_entity_id(text: &str) -> bool {
    text.split_once('v').is_some_and(|(index, generation)| {
        !index.is_empty()
            && !generation.is_empty()
            && index.bytes().all(|b| b.is_ascii_digit())
            && generation.bytes().all(|b| b.is_ascii_digit())
    })
}

/// An entity id as text: `495v0`, or the number older Bevy versions use.
pub fn entity_text(entity: &Value) -> String {
    match entity {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

/// The text of a serialized `Name`.
pub fn name_text(value: &Value) -> Option<String> {
    match value {
        Value::String(name) => Some(name.clone()),
        // Older versions serialize `Name` as a struct.
        Value::Object(map) => map.get("name").and_then(Value::as_str).map(str::to_string),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_entity_ids() {
        assert!(is_entity_id("495v0"));
        assert!(is_entity_id("12v34"));
        assert!(!is_entity_id("v1"));
        assert!(!is_entity_id("12v"));
        assert!(!is_entity_id("Player"));
        assert!(!is_entity_id("revive"));
        assert_eq!(entity_text(&json!("1v0")), "1v0");
        assert_eq!(entity_text(&json!(4294967296_u64)), "4294967296");
    }
}
