//! `hearth mcp`: the commands as tools of a Model Context Protocol server, over stdio.
//!
//! Every command becomes a tool whose properties mirror its arguments, and a call runs the
//! command as `hearth <command> --json` would, returning its JSON envelope. Screenshots also
//! return the image. Streaming tools stop after `--stream-duration` when the call sets no limit.

use std::io::{BufRead, Write};
use std::time::Duration;

use serde_json::{json, Map, Value};

use crate::cli::McpArgs;
use crate::commands::{manifest, CommandInfo};
use crate::context::{Attachment, Ctx, McpLimits};
use crate::output::{CliError, Outcome};

/// The protocol versions this server speaks, newest first.
const PROTOCOL_VERSIONS: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

/// Commands that aren't tools: the server itself, the command list that `tools/list` replaces,
/// and changes to the user's skill folders.
const EXCLUDED: &[&str] = &[
    "mcp",
    "commands",
    "skill install",
    "skill uninstall",
    "skill status",
];

/// Arguments that take numbers.
const NUMERIC: &[&str] = &[
    "x",
    "y",
    "speed",
    "count",
    "duration",
    "hold",
    "limit",
    "depth",
    "tail",
    "since",
    "port",
    "max_size",
    "code",
    "wait_timeout",
];

/// Arguments that take any JSON value.
const JSON_VALUES: &[&str] = &["value", "params"];

/// What the server tells clients about using its tools.
const INSTRUCTIONS: &str = "Tools to build, run, inspect and drive Hearth apps. Live tools talk to a running app over the Hearth Remote Protocol: start one with `run` (it runs detached as a session and returns once the app answers; `editor` starts the project's editor the same way), then use `status`, `tree`, `find`, `get`, `set`, `screenshot`, `input_*` and the rest. Entities are names or ids such as \"495v0\"; types are short names such as \"Transform\" or full type paths; values are JSON. Every result is a JSON envelope: on failure `errors[0]` has a stable `code`, a `hint` and often `data.candidates` to retry with. Pass `count` or `duration` to streaming tools. `skill_show` has the full guide; `skill_show` with topic \"recipes\" has step-by-step recipes.";

/// A tool's name: the command's words joined by `_`, such as `resource_get`.
fn tool_name(command: &CommandInfo) -> String {
    command.path.join("_")
}

fn property_schema(arg: &crate::commands::ArgInfo) -> Value {
    let mut description = arg.help.trim().to_string();
    if let Some(default) = &arg.default {
        description.push_str(&format!(" Default: {default}."));
    }
    let item = if !arg.choices.is_empty() {
        json!({ "type": "string", "enum": arg.choices })
    } else if NUMERIC.contains(&arg.id.as_str()) {
        json!({ "type": "number" })
    } else if JSON_VALUES.contains(&arg.id.as_str()) {
        description.push_str(" Any JSON value; a string that isn't JSON is taken as text.");
        json!({})
    } else {
        json!({ "type": "string" })
    };
    let mut schema = if !arg.takes_value {
        json!({ "type": "boolean" })
    } else if arg.multiple {
        json!({ "type": "array", "items": item })
    } else {
        item
    };
    schema["description"] = json!(description);
    schema
}

fn tool_definition(command: &CommandInfo, limits: McpLimits) -> Value {
    let mut properties = Map::new();
    let mut required = Vec::new();
    for arg in &command.args {
        properties.insert(arg.id.clone(), property_schema(arg));
        if arg.required {
            required.push(arg.id.clone());
        }
    }
    if command.takes_app() {
        properties.insert(
            "app".to_string(),
            json!({ "type": "string", "description": "The app: a session name, a port, host:port or a URL. Defaults to the server's --app, else the current project's session, else 127.0.0.1:15702." }),
        );
    }
    if command.live() {
        properties.insert(
            "timeout".to_string(),
            json!({ "type": "number", "description": "Seconds to wait for the app to answer. Default: 10." }),
        );
    }
    let mut description = command.about.clone();
    if let Some(long) = &command.long_about {
        description = long.clone();
    }
    if let Some(notes) = &command.notes {
        description.push_str("\n\n");
        description.push_str(notes);
    }
    if command.streams() {
        description.push_str(&format!(
            "\n\nWithout `count` or `duration` this stops after {:.0} seconds.",
            limits.stream_duration.as_secs_f64()
        ));
    }
    if !command.examples.is_empty() {
        description.push_str("\n\nThe same as these command lines:\n");
        for example in &command.examples {
            description.push_str(&format!("  {example}\n"));
        }
    }
    json!({
        "name": tool_name(command),
        "title": command.about.trim_end_matches('.'),
        "description": description.trim_end(),
        "inputSchema": {
            "type": "object",
            "properties": properties,
            "required": required,
            "additionalProperties": false,
        },
        "annotations": {
            "readOnlyHint": command.read_only(),
            "destructiveHint": command.destructive(),
            "openWorldHint": false,
        },
    })
}

/// A JSON value as a command line argument.
fn argument_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

/// The command line for a tool call.
fn command_line(
    command: &CommandInfo,
    arguments: &Map<String, Value>,
) -> Result<Vec<String>, String> {
    for key in arguments.keys() {
        let known = command.args.iter().any(|arg| &arg.id == key)
            || (key == "app" && command.takes_app())
            || (key == "timeout" && command.live());
        if !known {
            return Err(format!("`{}` has no argument `{key}`", tool_name(command)));
        }
    }
    let mut argv = vec!["hearth".to_string()];
    argv.extend(command.path.iter().cloned());
    for global in ["app", "timeout"] {
        if let Some(value) = arguments.get(global).filter(|value| !value.is_null()) {
            argv.push(format!("--{global}={}", argument_text(value)));
        }
    }
    let mut positional = Vec::new();
    for arg in &command.args {
        let Some(value) = arguments.get(&arg.id).filter(|value| !value.is_null()) else {
            continue;
        };
        let values: Vec<String> = match value {
            Value::Array(items) if arg.multiple => items.iter().map(argument_text).collect(),
            other => vec![argument_text(other)],
        };
        if arg.positional {
            positional.extend(values);
        } else if !arg.takes_value {
            if value.as_bool().unwrap_or(false) || value.as_str() == Some("true") {
                argv.push(format!("--{}", arg.long.as_deref().unwrap_or(&arg.id)));
            }
        } else {
            let long = arg.long.as_deref().unwrap_or(&arg.id);
            argv.extend(values.into_iter().map(|value| format!("--{long}={value}")));
        }
    }
    // After `--`, values that start with `-` aren't taken for options.
    if !positional.is_empty() {
        argv.push("--".to_string());
        argv.extend(positional);
    }
    Ok(argv)
}

/// Base64, for images in tool results.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(chunk.get(1).copied().unwrap_or(0)) << 8)
            | u32::from(chunk.get(2).copied().unwrap_or(0));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn tool_result(envelope: &Value, attachments: Vec<Attachment>) -> Value {
    let mut content = vec![json!({ "type": "text", "text": envelope.to_string() })];
    for attachment in attachments {
        content.push(json!({
            "type": "image",
            "data": base64(&attachment.data),
            "mimeType": attachment.mime,
        }));
    }
    json!({
        "content": content,
        "isError": envelope["success"] != true,
    })
}

/// A JSON-RPC error.
struct RpcError {
    code: i64,
    message: String,
}

impl RpcError {
    fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

struct Server {
    tools: Vec<CommandInfo>,
    limits: McpLimits,
    app: Option<String>,
    timeout: Duration,
}

impl Server {
    fn handle(&self, method: &str, params: &Value) -> Result<Value, RpcError> {
        match method {
            "initialize" => {
                let requested = params["protocolVersion"].as_str().unwrap_or_default();
                let version = PROTOCOL_VERSIONS
                    .iter()
                    .find(|version| **version == requested)
                    .unwrap_or(&PROTOCOL_VERSIONS[0]);
                Ok(json!({
                    "protocolVersion": version,
                    "capabilities": { "tools": { "listChanged": false } },
                    "serverInfo": {
                        "name": "hearth",
                        "title": "Hearth CLI",
                        "version": env!("CARGO_PKG_VERSION"),
                    },
                    "instructions": INSTRUCTIONS,
                }))
            }
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({
                "tools": self
                    .tools
                    .iter()
                    .map(|tool| tool_definition(tool, self.limits))
                    .collect::<Vec<_>>(),
            })),
            "tools/call" => {
                let name = params["name"]
                    .as_str()
                    .ok_or_else(|| RpcError::new(-32602, "tools/call needs a tool `name`"))?;
                let tool = self
                    .tools
                    .iter()
                    .find(|tool| tool_name(tool) == name)
                    .ok_or_else(|| {
                        RpcError::new(-32602, format!("There's no tool named `{name}`"))
                    })?;
                let arguments = params["arguments"].as_object().cloned().unwrap_or_default();
                let (envelope, attachments) = match command_line(tool, &arguments) {
                    Ok(argv) => {
                        crate::run_captured(argv, self.app.clone(), self.timeout, self.limits)
                    }
                    Err(message) => (
                        json!({
                            "success": false,
                            "command": tool.name(),
                            "data": null,
                            "errors": [{ "code": "INVALID_ARGUMENTS", "message": message }],
                            "warnings": [],
                        }),
                        Vec::new(),
                    ),
                };
                Ok(tool_result(&envelope, attachments))
            }
            "resources/list" => Ok(json!({ "resources": [] })),
            "prompts/list" => Ok(json!({ "prompts": [] })),
            _ => Err(RpcError::new(-32601, format!("Method not found: {method}"))),
        }
    }

    /// Handles one message, returning the response to send, if any.
    fn respond(&self, message: &Value) -> Option<Value> {
        let id = message.get("id").cloned();
        let Some(method) = message["method"].as_str() else {
            // A response to a request of ours; this server sends none.
            return None;
        };
        // Notifications get no response.
        let id = id?;
        Some(match self.handle(method, &message["params"]) {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err(error) => json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": error.code, "message": error.message },
            }),
        })
    }
}

fn send(message: &Value) {
    let mut stdout = std::io::stdout().lock();
    let _ = writeln!(stdout, "{message}");
    let _ = stdout.flush();
}

/// `hearth mcp`.
pub fn serve(ctx: &mut Ctx, args: McpArgs) -> Result<Outcome, CliError> {
    // stdout carries the protocol, so nothing else may print there.
    ctx.out.capture = true;
    let server = Server {
        tools: manifest()
            .into_iter()
            .filter(|command| !EXCLUDED.contains(&command.name().as_str()))
            .collect(),
        limits: McpLimits {
            stream_duration: Duration::from_secs_f64(args.stream_duration.max(0.1)),
            image_size: args.image_size,
        },
        app: ctx.app.clone(),
        timeout: ctx.timeout,
    };
    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let message: Value = match serde_json::from_str(&line) {
            Ok(message) => message,
            Err(error) => {
                send(&json!({
                    "jsonrpc": "2.0",
                    "id": null,
                    "error": { "code": -32700, "message": format!("Parse error: {error}") },
                }));
                continue;
            }
        };
        match &message {
            Value::Array(batch) => {
                let responses: Vec<Value> = batch
                    .iter()
                    .filter_map(|message| server.respond(message))
                    .collect();
                if !responses.is_empty() {
                    send(&Value::Array(responses));
                }
            }
            message => {
                if let Some(response) = server.respond(message) {
                    send(&response);
                }
            }
        }
    }
    Ok(Outcome::data(Value::Null))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool(name: &str) -> CommandInfo {
        manifest()
            .into_iter()
            .find(|command| command.name() == name)
            .unwrap()
    }

    #[test]
    fn encodes_base64() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn builds_command_lines() {
        let set = tool("set");
        let arguments = json!({ "entity": "Player", "target": "Transform.translation", "value": [0, -2, 0], "app": "game" });
        assert_eq!(
            command_line(&set, arguments.as_object().unwrap()).unwrap(),
            [
                "hearth",
                "set",
                "--app=game",
                "--",
                "Player",
                "Transform.translation",
                "[0,-2,0]"
            ]
        );
        let watch = tool("watch");
        let arguments =
            json!({ "entity": "Player", "components": ["Transform", "Name"], "count": 3 });
        assert_eq!(
            command_line(&watch, arguments.as_object().unwrap()).unwrap(),
            [
                "hearth",
                "watch",
                "--count=3",
                "--",
                "Player",
                "Transform",
                "Name"
            ]
        );
        let tree = tool("tree");
        let arguments = json!({ "all": true, "components": false });
        assert_eq!(
            command_line(&tree, arguments.as_object().unwrap()).unwrap(),
            ["hearth", "tree", "--all"]
        );
        let unknown = json!({ "nope": 1 });
        assert!(command_line(&tree, unknown.as_object().unwrap()).is_err());
    }

    #[test]
    fn describes_tools() {
        let limits = McpLimits {
            stream_duration: Duration::from_secs(5),
            image_size: 1280,
        };
        let get = tool_definition(&tool("get"), limits);
        assert_eq!(get["name"], "get");
        assert_eq!(get["inputSchema"]["required"], json!(["entity"]));
        assert_eq!(
            get["inputSchema"]["properties"]["components"]["type"],
            "array"
        );
        assert_eq!(get["annotations"]["readOnlyHint"], true);
        let key = tool_definition(&tool("input key"), limits);
        assert_eq!(key["name"], "input_key");
        assert_eq!(key["inputSchema"]["properties"]["hold"]["type"], "number");
        let observe = tool_definition(&tool("observe"), limits);
        assert!(observe["description"]
            .as_str()
            .unwrap()
            .contains("5 seconds"));
    }
}
