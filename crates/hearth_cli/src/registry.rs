//! The app's reflected types, from `registry.schema`: resolving the short names people type to
//! full type paths, and building example values.

use std::collections::HashMap;

use serde_json::{json, Map, Value};

use crate::hrp::Client;
use crate::output::{CliError, Exit};

/// A reflected type.
#[derive(Debug, Clone)]
pub struct TypeInfo {
    /// The full type path, such as `hearth_transform::components::transform::Transform`.
    pub path: String,
    /// The short path, such as `Transform` or `Time<Virtual>`.
    pub short: String,
    /// The crate that defines it.
    pub crate_name: String,
    /// The reflected traits, such as `Component`, `Resource`, `Default` and `Serialize`.
    pub reflect: Vec<String>,
    /// The reflection kind, such as `Struct`, `Enum` or `Value`.
    pub kind: String,
    /// Its JSON schema.
    pub schema: Value,
}

impl TypeInfo {
    /// Whether the type reflects `name`, such as `Component`.
    pub fn reflects(&self, name: &str) -> bool {
        self.reflect.iter().any(|reflected| reflected == name)
    }

    /// What the type can be used as: `component`, `resource`, `event` and `message`.
    pub fn roles(&self) -> Vec<&'static str> {
        [
            ("Component", "component"),
            ("Resource", "resource"),
            ("Event", "event"),
            ("Message", "message"),
        ]
        .into_iter()
        .filter(|(name, _)| self.reflects(name))
        .map(|(_, role)| role)
        .collect()
    }
}

/// What a name must resolve to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Want {
    /// A component.
    Component,
    /// A resource.
    Resource,
    /// An event, for triggering and observing.
    Event,
    /// A message. The schema doesn't mark messages yet, so any type is accepted, preferring
    /// those marked `Message`.
    Message,
    /// Any type.
    Any,
}

impl Want {
    fn noun(self) -> &'static str {
        match self {
            Self::Component => "component",
            Self::Resource => "resource",
            Self::Event => "event",
            Self::Message => "message",
            Self::Any => "type",
        }
    }

    fn accepts(self, info: &TypeInfo) -> bool {
        match self {
            Self::Component => info.reflects("Component"),
            Self::Resource => info.reflects("Resource"),
            Self::Event => info.reflects("Event"),
            Self::Message => info.reflects("Message"),
            Self::Any => true,
        }
    }
}

/// The app's type registry.
pub struct Registry {
    /// Every type, sorted by path.
    pub types: Vec<TypeInfo>,
    index: HashMap<String, usize>,
}

impl Registry {
    /// How long to wait for `registry.schema`, which takes a large app a while to answer.
    pub fn fetch_timeout(client: &Client) -> core::time::Duration {
        client.timeout.max(core::time::Duration::from_secs(30))
    }

    /// Builds the registry from a `registry.schema` result.
    pub fn from_schema(schema: Value) -> Self {
        let Value::Object(entries) = schema else {
            return Self {
                types: Vec::new(),
                index: HashMap::new(),
            };
        };
        let mut types: Vec<TypeInfo> = entries
            .into_iter()
            .map(|(path, schema)| {
                let text = |key: &str| schema.get(key).and_then(Value::as_str).map(str::to_string);
                TypeInfo {
                    short: text("shortPath").unwrap_or_else(|| shorten_path(&path)),
                    crate_name: text("crateName")
                        .unwrap_or_else(|| path.split("::").next().unwrap_or_default().to_string()),
                    kind: text("kind").unwrap_or_default(),
                    reflect: schema
                        .get("reflectTypes")
                        .and_then(Value::as_array)
                        .map(|names| {
                            names
                                .iter()
                                .filter_map(Value::as_str)
                                .map(str::to_string)
                                .collect()
                        })
                        .unwrap_or_default(),
                    path,
                    schema,
                }
            })
            .collect();
        types.sort_by(|a, b| a.path.cmp(&b.path));
        let index = types
            .iter()
            .enumerate()
            .map(|(i, info)| (info.path.clone(), i))
            .collect();
        Self { types, index }
    }

    /// The type with this exact path.
    pub fn get(&self, path: &str) -> Option<&TypeInfo> {
        self.index.get(path).map(|&i| &self.types[i])
    }

    /// The short name of a type path, from the registry when known.
    pub fn short_name(&self, path: &str) -> String {
        self.get(path)
            .map(|info| info.short.clone())
            .unwrap_or_else(|| shorten_path(path))
    }

    /// Resolves what someone typed to one type.
    ///
    /// Tried in order, the first that matches exactly one type wins: the full path, the short
    /// name, the short name ignoring case, a path suffix such as `transform::Transform`, and the
    /// short form of generic types such as `Time<Virtual>`.
    pub fn resolve(&self, name: &str, want: Want) -> Result<&TypeInfo, CliError> {
        let name = name.trim();
        if let Some(info) = self.get(name) {
            return Ok(info);
        }
        let mut pool: Vec<&TypeInfo> = self
            .types
            .iter()
            .filter(|info| want.accepts(info))
            .collect();
        if want == Want::Message && pool.is_empty() {
            pool = self.types.iter().collect();
        }
        if let Some(found) = Self::match_in(&pool, name, want)? {
            return Ok(found);
        }
        if want == Want::Message {
            let everything: Vec<&TypeInfo> = self.types.iter().collect();
            if let Some(found) = Self::match_in(&everything, name, want)? {
                return Ok(found);
            }
        }

        // Explain a type that exists but isn't what the command needs.
        let everything: Vec<&TypeInfo> = self.types.iter().collect();
        if want != Want::Any
            && let Ok(Some(found)) = Self::match_in(&everything, name, Want::Any)
        {
            return Err(CliError::new(
                "WRONG_KIND",
                Exit::Rejected,
                format!(
                    "`{}` is registered, but not as a {} (it reflects {})",
                    found.path,
                    want.noun(),
                    if found.reflect.is_empty() {
                        "nothing".to_string()
                    } else {
                        found.reflect.join(", ")
                    }
                ),
            )
            .hint(format!(
                "A type is usable as a {0} when it derives `{1}` and has `#[reflect({1})]`.",
                want.noun(),
                match want {
                    Want::Component => "Component",
                    Want::Resource => "Resource",
                    Want::Event => "Event",
                    _ => "Message",
                }
            )));
        }

        let lower = name.to_lowercase();
        let mut candidates: Vec<&TypeInfo> = pool
            .iter()
            .copied()
            .filter(|info| info.path.to_lowercase().contains(&lower))
            .collect();
        if candidates.is_empty() {
            candidates = pool
                .iter()
                .copied()
                .filter(|info| edit_distance(&info.short.to_lowercase(), &lower) <= 2)
                .collect();
        }
        candidates.truncate(12);
        let hint = if candidates.is_empty() {
            format!(
                "`hearth types <text>` searches the registered types. A {} must derive `Reflect` and be registered, which most types are automatically.",
                want.noun()
            )
        } else {
            "Did you mean one of these?".to_string()
        };
        Err(CliError::new(
            "UNKNOWN_TYPE",
            Exit::Rejected,
            format!("No {} named `{name}` is registered in the app", want.noun()),
        )
        .hint(hint)
        .data(json!({
            "candidates": candidates.iter().map(|info| info.path.clone()).collect::<Vec<_>>(),
        })))
    }

    fn match_in<'a>(
        pool: &[&'a TypeInfo],
        name: &str,
        want: Want,
    ) -> Result<Option<&'a TypeInfo>, CliError> {
        let lower = name.to_lowercase();
        let suffix = format!("::{name}");
        let tiers: [&dyn Fn(&TypeInfo) -> bool; 4] = [
            &|info| info.short == name,
            &|info| info.short.to_lowercase() == lower,
            &|info| info.path.ends_with(&suffix),
            &|info| shorten_path(&info.path).to_lowercase() == lower,
        ];
        for tier in tiers {
            let found: Vec<&TypeInfo> = pool.iter().copied().filter(|info| tier(info)).collect();
            match found.len() {
                0 => {}
                1 => return Ok(Some(found[0])),
                _ => {
                    return Err(CliError::new(
                        "AMBIGUOUS_TYPE",
                        Exit::Rejected,
                        format!("`{name}` matches {} {}s", found.len(), want.noun()),
                    )
                    .hint("Use the full type path.")
                    .data(json!({
                        "candidates": found.iter().map(|info| info.path.clone()).collect::<Vec<_>>(),
                    })));
                }
            }
        }
        Ok(None)
    }

    /// An example value for a type, to start editing from: its default when it has one, else
    /// a value built from its schema.
    pub fn example(&self, path: &str) -> Value {
        self.example_at(path, 0)
    }

    fn example_at(&self, path: &str, depth: usize) -> Value {
        let Some(info) = self.get(path) else {
            return Value::Null;
        };
        if let Some(default) = info.schema.get("default") {
            return default.clone();
        }
        if depth > 6 {
            return Value::Null;
        }
        let schema = &info.schema;
        let of = |field: &Value| {
            type_ref(field)
                .map(|path| self.example_at(path, depth + 1))
                .unwrap_or(Value::Null)
        };
        match info.kind.as_str() {
            "Struct" => {
                let mut fields: Vec<(&String, &Value)> = schema
                    .get("properties")
                    .and_then(Value::as_object)
                    .map(|properties| properties.iter().collect())
                    .unwrap_or_default();
                fields.sort_by_key(|(_, field)| field.get("fieldIndex").and_then(Value::as_u64));
                Value::Object(
                    fields
                        .into_iter()
                        .map(|(name, field)| (name.clone(), of(field)))
                        .collect::<Map<_, _>>(),
                )
            }
            "TupleStruct" | "Tuple" => {
                let items: Vec<Value> = schema
                    .get("prefixItems")
                    .and_then(Value::as_array)
                    .map(|items| items.iter().map(of).collect())
                    .unwrap_or_default();
                // Single-field tuple structs serialize as their field.
                if info.kind == "TupleStruct" && items.len() == 1 {
                    items.into_iter().next().unwrap_or(Value::Null)
                } else {
                    Value::Array(items)
                }
            }
            "Enum" => {
                let variants = schema.get("oneOf").and_then(Value::as_array);
                let variants = variants.map(Vec::as_slice).unwrap_or_default();
                if let Some(unit) = variants.iter().find_map(Value::as_str) {
                    return json!(unit);
                }
                let Some(variant) = variants.first() else {
                    return Value::Null;
                };
                let name = variant
                    .get("shortPath")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let inner =
                    if let Some(items) = variant.get("prefixItems").and_then(Value::as_array) {
                        if items.len() == 1 {
                            of(&items[0])
                        } else {
                            Value::Array(items.iter().map(of).collect())
                        }
                    } else if let Some(properties) =
                        variant.get("properties").and_then(Value::as_object)
                    {
                        Value::Object(
                            properties
                                .iter()
                                .map(|(name, field)| (name.clone(), of(field)))
                                .collect(),
                        )
                    } else {
                        return json!(name);
                    };
                json!({ name: inner })
            }
            "List" | "Set" | "Array" => json!([]),
            "Map" => json!({}),
            _ => match schema.get("type").and_then(Value::as_str) {
                Some("string") => json!(""),
                Some("float") | Some("number") => json!(0.0),
                Some("int") | Some("uint") | Some("integer") => json!(0),
                Some("boolean") | Some("bool") => json!(false),
                Some("object") => json!({}),
                Some("array") => json!([]),
                _ => Value::Null,
            },
        }
    }
}

/// The type path a schema field refers to, from `{"type": {"$ref": "#/$defs/<path>"}}`.
pub fn type_ref(field: &Value) -> Option<&str> {
    field
        .get("type")
        .and_then(|ty| ty.get("$ref"))
        .and_then(Value::as_str)
        .and_then(|reference| reference.strip_prefix("#/$defs/"))
}

/// Drops the module paths in a type path: `hearth_time::time::Time<hearth_time::virt::Virtual>`
/// becomes `Time<Virtual>`.
pub fn shorten_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    let mut segment = String::new();
    let flush = |segment: &mut String, out: &mut String| {
        out.push_str(segment.rsplit("::").next().unwrap_or(segment));
        segment.clear();
    };
    for c in path.chars() {
        if c.is_alphanumeric() || c == '_' || c == ':' {
            segment.push(c);
        } else {
            flush(&mut segment, &mut out);
            out.push(c);
        }
    }
    flush(&mut segment, &mut out);
    out
}

/// The Levenshtein distance between two strings.
pub fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut current = vec![i + 1];
        for (j, &cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            current.push(
                (previous[j] + cost)
                    .min(previous[j + 1] + 1)
                    .min(current[j] + 1),
            );
        }
        previous = current;
    }
    previous[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry() -> Registry {
        Registry::from_schema(json!({
            "hearth_transform::components::transform::Transform": {
                "shortPath": "Transform", "crateName": "hearth_transform", "kind": "Struct",
                "reflectTypes": ["Component", "Default"],
                "properties": {
                    "translation": { "fieldIndex": 0, "type": { "$ref": "#/$defs/glam::Vec3" } },
                    "scale": { "fieldIndex": 2, "type": { "$ref": "#/$defs/glam::Vec3" } },
                    "rotation": { "fieldIndex": 1, "type": { "$ref": "#/$defs/glam::Quat" } }
                }
            },
            "my_game::Transform": {
                "shortPath": "Transform", "crateName": "my_game", "kind": "Struct",
                "reflectTypes": ["Default"], "properties": {}
            },
            "glam::Vec3": { "shortPath": "Vec3", "kind": "Struct", "default": [0.0, 0.0, 0.0] },
            "glam::Quat": { "shortPath": "Quat", "kind": "Struct", "default": [0.0, 0.0, 0.0, 1.0] },
            "hearth_time::time::Time<hearth_time::virt::Virtual>": {
                "shortPath": "Time<Virtual>", "kind": "Struct", "reflectTypes": ["Resource"]
            },
            "hearth_camera::visibility::Visibility": {
                "shortPath": "Visibility", "kind": "Enum", "reflectTypes": ["Component"],
                "oneOf": ["Inherited", "Hidden", "Visible"]
            },
            "my_game::Health": {
                "shortPath": "Health", "kind": "TupleStruct", "reflectTypes": ["Component"],
                "prefixItems": [{ "type": { "$ref": "#/$defs/f32" } }]
            },
            "f32": { "shortPath": "f32", "kind": "Value", "type": "float" }
        }))
    }

    #[test]
    fn resolves_names() {
        let registry = registry();
        let path = |name, want| registry.resolve(name, want).map(|info| info.path.clone());
        let transform = "hearth_transform::components::transform::Transform";
        assert_eq!(path("Transform", Want::Component).unwrap(), transform);
        assert_eq!(path("transform", Want::Component).unwrap(), transform);
        assert_eq!(path(transform, Want::Component).unwrap(), transform);
        assert_eq!(
            path("time<virtual>", Want::Resource).unwrap(),
            "hearth_time::time::Time<hearth_time::virt::Virtual>"
        );
        assert_eq!(
            path("Time<hearth_time::virt::Virtual>", Want::Resource).unwrap(),
            "hearth_time::time::Time<hearth_time::virt::Virtual>"
        );
        assert_eq!(
            path("Transform", Want::Any).unwrap_err().code,
            "AMBIGUOUS_TYPE"
        );
        assert_eq!(
            path("Vec3", Want::Component).unwrap_err().code,
            "WRONG_KIND"
        );
        let unknown = path("Transfrom", Want::Component).unwrap_err();
        assert_eq!(unknown.code, "UNKNOWN_TYPE");
        assert_eq!(unknown.data.unwrap()["candidates"][0], json!(transform));
    }

    #[test]
    fn builds_examples() {
        let registry = registry();
        assert_eq!(
            registry.example("hearth_transform::components::transform::Transform"),
            json!({ "translation": [0.0, 0.0, 0.0], "rotation": [0.0, 0.0, 0.0, 1.0], "scale": [0.0, 0.0, 0.0] })
        );
        assert_eq!(
            registry.example("hearth_camera::visibility::Visibility"),
            json!("Inherited")
        );
        assert_eq!(registry.example("my_game::Health"), json!(0.0));
    }

    #[test]
    fn shortens_paths() {
        assert_eq!(
            shorten_path("hearth_time::time::Time<hearth_time::virt::Virtual>"),
            "Time<Virtual>"
        );
        assert_eq!(
            shorten_path("core::option::Option<(f32, alloc::string::String)>"),
            "Option<(f32, String)>"
        );
        assert_eq!(shorten_path("Plain"), "Plain");
    }

    #[test]
    fn measures_edit_distance() {
        assert_eq!(edit_distance("transfrom", "transform"), 2);
        assert_eq!(edit_distance("", "abc"), 3);
        assert_eq!(edit_distance("same", "same"), 0);
    }
}
