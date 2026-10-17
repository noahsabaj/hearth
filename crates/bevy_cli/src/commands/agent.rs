//! For agents and scripts: the command manifest that `hearth commands` prints and the MCP server
//! turns into tools, and the agent skill.

use clap::{ArgAction, CommandFactory};
use serde_json::{json, Value};

use crate::cli::{group_of, Cli, CommandsArgs, SkillCommand, GROUPS};
use crate::context::Ctx;
use crate::output::{CliError, Exit, Outcome, Style};

/// The help section of the commands that talk to a running app.
const LIVE: &str = "Live app (over BRP)";

/// Commands that only read.
const READ_ONLY: &[&str] = &[
    "status",
    "tree",
    "find",
    "get",
    "query",
    "resource list",
    "resource get",
    "types",
    "schema",
    "watch",
    "observe",
    "screenshot",
    "time",
    "time status",
    "schedules",
    "diagnostics",
    "methods",
    "ps",
    "logs",
    "examples",
    "doctor",
    "commands",
    "skill show",
    "skill status",
];

/// Commands that remove or end something.
const DESTRUCTIVE: &[&str] = &[
    "remove",
    "despawn",
    "resource remove",
    "exit",
    "stop",
    "skill uninstall",
];

/// Commands that stream events until a count or duration is reached.
const STREAMING: &[&str] = &["watch", "observe", "logs", "call"];

/// Error codes and what they mean. Every code the CLI reports is listed here.
pub const ERROR_CODES: &[(&str, &str)] = &[
    ("INVALID_ARGUMENTS", "The command line is invalid"),
    ("UNKNOWN_COMMAND", "No command has this name"),
    ("UNKNOWN_ARGUMENT", "The command has no such option"),
    ("MISSING_ARGUMENT", "A required argument is missing"),
    ("INVALID_VALUE", "An argument's value is invalid"),
    ("IO_ERROR", "A file couldn't be read or written"),
    (
        "INTERNAL_ERROR",
        "Something the CLI didn't expect; please report it",
    ),
    (
        "APP_UNREACHABLE",
        "No app answered at the address; `data.candidates` lists apps found on other ports",
    ),
    ("AMBIGUOUS_APP", "Several sessions match; pass --app"),
    ("UNKNOWN_APP", "--app is neither a session nor an address"),
    (
        "TIMEOUT",
        "The app, an event or a capture didn't answer in time",
    ),
    ("PROTOCOL_ERROR", "The address answered, but not over BRP"),
    (
        "METHOD_NOT_FOUND",
        "The app doesn't support the BRP method: an older engine version, or a plugin is missing",
    ),
    (
        "INVALID_PARAMS",
        "The app rejected the request's parameters",
    ),
    ("APP_ERROR", "The app failed to handle the request"),
    (
        "BRP_ERROR",
        "The app returned an error without a known code",
    ),
    ("ENTITY_NOT_FOUND", "No entity has this id or name"),
    (
        "AMBIGUOUS_ENTITY",
        "Several entities have this name; use an id from `data.candidates`",
    ),
    (
        "COMPONENT_ERROR",
        "The component value or path is invalid for its type",
    ),
    (
        "COMPONENT_NOT_PRESENT",
        "The entity doesn't have the component",
    ),
    ("SELF_REPARENT", "An entity can't be its own parent"),
    (
        "COMPONENT_NOT_IN_METADATA",
        "The component type isn't known to the app",
    ),
    (
        "RESOURCE_ERROR",
        "The resource value or path is invalid for its type",
    ),
    ("RESOURCE_NOT_PRESENT", "The app doesn't have the resource"),
    (
        "UNKNOWN_TYPE",
        "No registered type matches the name; `data.candidates` has close matches",
    ),
    (
        "AMBIGUOUS_TYPE",
        "Several types match the name; use a full path from `data.candidates`",
    ),
    (
        "WRONG_KIND",
        "The type exists but isn't a component, resource or event as needed",
    ),
    ("UNKNOWN_KEY", "The key name isn't a `KeyCode` or alias"),
    ("NO_WINDOW", "The app has no primary window"),
    (
        "CAPTURE_FAILED",
        "The screenshot couldn't be taken or saved",
    ),
    ("NO_PROJECT", "There's no Cargo project in this directory"),
    ("CARGO_NOT_FOUND", "cargo isn't installed or isn't on PATH"),
    ("CARGO_FAILED", "cargo failed before building"),
    (
        "TOOLCHAIN_NOT_INSTALLED",
        "The rustup toolchain isn't installed",
    ),
    (
        "UNKNOWN_PACKAGE",
        "The workspace has no package with this name",
    ),
    (
        "AMBIGUOUS_PACKAGE",
        "The workspace has several packages; pass -p",
    ),
    (
        "UNKNOWN_EXAMPLE",
        "The package has no example with this name",
    ),
    ("UNKNOWN_BIN", "The package has no binary with this name"),
    (
        "NO_BINARY",
        "The package has no binary; run an example instead",
    ),
    (
        "AMBIGUOUS_TARGET",
        "The package has several binaries; pass --bin",
    ),
    (
        "BUILD_FAILED",
        "Compilation failed; `data.errors` has each error with file and line",
    ),
    (
        "DIRECTORY_NOT_EMPTY",
        "The new project's directory already has files",
    ),
    ("APP_FAILED", "The app exited with a failure"),
    (
        "APP_EXITED",
        "The app exited before it answered over BRP; `data.log_tail` has its last output",
    ),
    ("UNKNOWN_SESSION", "No session has this name"),
    (
        "NOT_A_SESSION",
        "The app wasn't started with `hearth run --detach`",
    ),
    ("NO_SESSION", "There are no sessions"),
    ("AMBIGUOUS_SESSION", "Several sessions exist; name one"),
    ("STOP_FAILED", "The session's process couldn't be ended"),
    (
        "SKILL_EXISTS",
        "A skill this CLI didn't write is installed there; pass --force to replace it",
    ),
    (
        "UNKNOWN_TOPIC",
        "The skill has no reference topic with this name",
    ),
];

/// Environment variables the CLI reads, or sets for the apps it runs.
pub const ENVIRONMENT: &[(&str, &str)] = &[
    ("HEARTH_APP", "The app to talk to, like --app"),
    ("HEARTH_FORMAT", "The output format, like --format"),
    (
        "HEARTH_TIMEOUT",
        "Seconds to wait for the app, like --timeout",
    ),
    (
        "HEARTH_TOOLCHAIN",
        "The rustup toolchain to build with, like --toolchain",
    ),
    ("HEARTH_CLI_HOME", "Where sessions and their logs are kept"),
    ("NO_COLOR", "Disables colors"),
    (
        "HEARTH_REMOTE_PORT",
        "Set by `hearth run` for the app: the port to answer BRP on",
    ),
];

/// An argument of a command.
#[derive(Clone, Debug)]
pub struct ArgInfo {
    /// Its identifier, which is also its MCP property name.
    pub id: String,
    /// Its long flag, without `--`.
    pub long: Option<String>,
    /// Its short flag.
    pub short: Option<char>,
    /// What its value is called.
    pub value_name: String,
    /// What it does.
    pub help: String,
    /// Whether it's positional.
    pub positional: bool,
    /// Whether it takes a value, rather than being a flag.
    pub takes_value: bool,
    /// Whether it takes several values.
    pub multiple: bool,
    /// Whether it's required.
    pub required: bool,
    /// Its default value.
    pub default: Option<String>,
    /// The values it accepts, when limited.
    pub choices: Vec<String>,
    /// Whether it comes after `--`.
    pub last: bool,
}

impl ArgInfo {
    /// The JSON description.
    pub fn to_json(&self) -> Value {
        json!({
            "name": self.id,
            "kind": if self.positional { "positional" } else if self.takes_value { "option" } else { "flag" },
            "long": self.long.as_ref().map(|long| format!("--{long}")),
            "short": self.short.map(|short| format!("-{short}")),
            "value_name": self.takes_value.then_some(&self.value_name),
            "help": self.help,
            "required": self.required,
            "multiple": self.multiple,
            "default": self.default,
            "choices": (!self.choices.is_empty()).then_some(&self.choices),
            "after_double_dash": self.last,
        })
    }
}

/// A command, as `hearth commands` and the MCP server describe it.
#[derive(Clone, Debug)]
pub struct CommandInfo {
    /// The words that name it, such as `["resource", "get"]`.
    pub path: Vec<String>,
    /// Its help section.
    pub group: &'static str,
    /// Its one-line description.
    pub about: String,
    /// Its full description, when longer.
    pub long_about: Option<String>,
    /// Example command lines.
    pub examples: Vec<String>,
    /// Help text after the examples.
    pub notes: Option<String>,
    /// Other names it answers to.
    pub aliases: Vec<String>,
    /// Its arguments, positional ones in order.
    pub args: Vec<ArgInfo>,
}

impl CommandInfo {
    /// Its name, such as `resource get`.
    pub fn name(&self) -> String {
        self.path.join(" ")
    }

    /// Whether it talks to a running app.
    pub fn live(&self) -> bool {
        self.group == LIVE
    }

    /// Whether it accepts `--app`.
    pub fn takes_app(&self) -> bool {
        self.live() || matches!(self.path[0].as_str(), "logs" | "stop" | "doctor")
    }

    /// Whether it only reads.
    pub fn read_only(&self) -> bool {
        READ_ONLY.contains(&self.name().as_str())
    }

    /// Whether it removes or ends something.
    pub fn destructive(&self) -> bool {
        DESTRUCTIVE.contains(&self.name().as_str())
    }

    /// Whether it streams events.
    pub fn streams(&self) -> bool {
        STREAMING.contains(&self.name().as_str())
    }

    /// A usage line such as `get <ENTITY> [COMPONENTS]...`.
    pub fn usage(&self) -> String {
        let mut usage = self.name();
        for arg in self.args.iter().filter(|arg| arg.positional) {
            let mut word = if arg.required {
                format!("<{}>", arg.value_name)
            } else {
                format!("[{}]", arg.value_name)
            };
            if arg.multiple {
                word.push_str("...");
            }
            if arg.last {
                word = format!("[-- {word}]");
            }
            usage.push(' ');
            usage.push_str(&word);
        }
        usage
    }

    /// The JSON description.
    pub fn to_json(&self) -> Value {
        json!({
            "name": self.name(),
            "usage": format!("hearth {}", self.usage()),
            "group": self.group,
            "about": self.about,
            "long_about": self.long_about,
            "notes": self.notes,
            "aliases": self.aliases,
            "examples": self.examples,
            "arguments": self.args.iter().map(ArgInfo::to_json).collect::<Vec<_>>(),
            "live": self.live(),
            "read_only": self.read_only(),
            "destructive": self.destructive(),
            "streams": self.streams(),
        })
    }

    fn matches(&self, needle: &str) -> bool {
        let haystack = format!(
            "{} {} {} {} {}",
            self.name(),
            self.about,
            self.long_about.as_deref().unwrap_or_default(),
            self.examples.join(" "),
            self.args
                .iter()
                .map(|arg| format!("{} {}", arg.id, arg.help))
                .collect::<Vec<_>>()
                .join(" ")
        );
        haystack.to_lowercase().contains(needle)
    }
}

fn arg_info(arg: &clap::Arg) -> ArgInfo {
    let action = arg.get_action();
    let id = arg.get_id().to_string();
    ArgInfo {
        long: arg.get_long().map(str::to_string),
        short: arg.get_short(),
        value_name: arg
            .get_value_names()
            .and_then(|names| names.first())
            .map(ToString::to_string)
            .unwrap_or_else(|| id.to_uppercase()),
        help: arg.get_help().map(ToString::to_string).unwrap_or_default(),
        positional: arg.is_positional(),
        takes_value: action.takes_values(),
        multiple: matches!(action, ArgAction::Append),
        required: arg.is_required_set(),
        default: arg
            .get_default_values()
            .first()
            .map(|value| value.to_string_lossy().to_string()),
        choices: arg
            .get_possible_values()
            .iter()
            .filter(|value| !value.is_hide_set())
            .map(|value| value.get_name().to_string())
            .collect(),
        last: arg.is_last_set(),
        id,
    }
}

/// Splits a command's text after its options into examples and notes.
fn examples_and_notes(command: &clap::Command) -> (Vec<String>, Option<String>) {
    let Some(text) = command.get_after_help().map(ToString::to_string) else {
        return (Vec::new(), None);
    };
    let mut lines = text.lines();
    let mut examples = Vec::new();
    let mut notes = Vec::new();
    while let Some(line) = lines.next() {
        if line.trim_end() == "Examples:" {
            for line in lines.by_ref() {
                if line.trim().is_empty() {
                    break;
                }
                examples.push(line.trim().to_string());
            }
        } else {
            notes.push(line);
        }
    }
    let notes = notes.join("\n").trim().to_string();
    (examples, (!notes.is_empty()).then_some(notes))
}

fn collect(command: &clap::Command, parents: &[String], out: &mut Vec<CommandInfo>) {
    if command.get_name() == "help" || command.is_hide_set() {
        return;
    }
    let mut path = parents.to_vec();
    path.push(command.get_name().to_string());
    let has_subcommands = command
        .get_subcommands()
        .any(|sub| sub.get_name() != "help");
    if !has_subcommands || !command.is_subcommand_required_set() {
        let (examples, notes) = examples_and_notes(command);
        let about = command
            .get_about()
            .map(ToString::to_string)
            .unwrap_or_default();
        let long_about = command
            .get_long_about()
            .map(ToString::to_string)
            .filter(|long| long.trim() != about.trim());
        out.push(CommandInfo {
            group: group_of(&path[0]),
            about,
            long_about,
            examples,
            notes,
            aliases: command.get_visible_aliases().map(str::to_string).collect(),
            args: command
                .get_arguments()
                .filter(|arg| {
                    !arg.is_global_set()
                        && !arg.is_hide_set()
                        && !matches!(
                            arg.get_action(),
                            ArgAction::Help
                                | ArgAction::HelpShort
                                | ArgAction::HelpLong
                                | ArgAction::Version
                        )
                })
                .map(arg_info)
                .collect(),
            path: path.clone(),
        });
    }
    for sub in command.get_subcommands() {
        collect(sub, &path, out);
    }
}

/// Every command, in the order of the help.
pub fn manifest() -> Vec<CommandInfo> {
    let mut command = Cli::command();
    command.build();
    let mut out = Vec::new();
    for sub in command.get_subcommands() {
        collect(sub, &[], &mut out);
    }
    out
}

/// The options every command accepts.
fn global_options() -> Vec<ArgInfo> {
    let mut command = Cli::command();
    command.build();
    command
        .get_arguments()
        .filter(|arg| arg.is_global_set())
        .map(arg_info)
        .collect()
}

/// `hearth commands`.
pub fn commands(ctx: &mut Ctx, args: CommandsArgs) -> Result<Outcome, CliError> {
    let mut commands = manifest();
    if let Some(grep) = &args.grep {
        let needle = grep.to_lowercase();
        commands.retain(|command| command.matches(&needle));
    }
    let width = commands
        .iter()
        .map(|command| command.usage().chars().count())
        .max()
        .unwrap_or(0)
        .min(44);
    let mut lines = Vec::new();
    for group in GROUPS {
        let in_group: Vec<&CommandInfo> = commands
            .iter()
            .filter(|command| command.group == *group)
            .collect();
        if in_group.is_empty() {
            continue;
        }
        if !lines.is_empty() {
            lines.push(String::new());
        }
        lines.push(ctx.out.paint(group, Style::Bold));
        for command in in_group {
            let usage = command.usage();
            let pad = width.saturating_sub(usage.chars().count());
            lines.push(format!("  {usage}{}  {}", " ".repeat(pad), command.about));
        }
    }
    if lines.is_empty() {
        lines.push("No commands match.".to_string());
    }
    lines.push(String::new());
    lines.push(ctx.out.paint(
        "`hearth <command> --help` explains a command; `hearth commands --json` lists every argument, exit code and error code.",
        Style::Dim,
    ));
    let data = json!({
        "name": "hearth",
        "version": env!("CARGO_PKG_VERSION"),
        "about": "Build, run, inspect and drive Hearth apps. Live-app commands use the Bevy Remote Protocol.",
        "usage": "hearth [OPTIONS] <COMMAND>",
        "global_options": global_options().iter().map(ArgInfo::to_json).collect::<Vec<_>>(),
        "commands": commands.iter().map(CommandInfo::to_json).collect::<Vec<_>>(),
        "output": {
            "formats": ["human", "json", "ndjson"],
            "envelope": "With --json, stdout is one object: {success, command, data, errors: [{code, message, hint?, data?}], warnings}",
            "ndjson": "With --format ndjson, streams print {type: \"event\", data} lines, lists print {type: \"item\", data} lines, and a final {type: \"result\", ...envelope} line",
        },
        "exit_codes": Exit::TABLE
            .iter()
            .map(|(code, meaning)| json!({ "code": code, "meaning": meaning }))
            .collect::<Vec<_>>(),
        "error_codes": ERROR_CODES
            .iter()
            .map(|(code, meaning)| json!({ "code": code, "meaning": meaning }))
            .collect::<Vec<_>>(),
        "environment": ENVIRONMENT
            .iter()
            .map(|(name, meaning)| json!({ "name": name, "meaning": meaning }))
            .collect::<Vec<_>>(),
    });
    Ok(Outcome::new(data, lines.join("\n")))
}

/// `hearth skill`.
pub fn skill(ctx: &mut Ctx, command: SkillCommand) -> Result<Outcome, CliError> {
    match command {
        SkillCommand::Show { topic, list } => crate::skill::show(ctx, topic.as_deref(), list),
        SkillCommand::Install {
            location,
            full,
            force,
        } => crate::skill::install(&location, full, force),
        SkillCommand::Uninstall { location } => crate::skill::uninstall(&location),
        SkillCommand::Status { location } => crate::skill::status(&location),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_every_command() {
        let commands = manifest();
        let names: Vec<String> = commands.iter().map(CommandInfo::name).collect();
        for expected in [
            "new",
            "run",
            "get",
            "resource get",
            "input key",
            "time",
            "time pause",
            "skill show",
            "mcp",
        ] {
            assert!(
                names.contains(&expected.to_string()),
                "{expected} is missing from {names:?}"
            );
        }
        assert!(
            !names.contains(&"resource".to_string()),
            "a command that needs a subcommand isn't runnable"
        );
        assert!(!names.iter().any(|name| name.ends_with("help")));
        let get = commands
            .iter()
            .find(|command| command.name() == "get")
            .unwrap();
        assert_eq!(get.usage(), "get <ENTITY> [COMPONENTS]...");
        assert!(get.live() && get.read_only() && !get.examples.is_empty());
        assert!(
            get.args.iter().all(|arg| arg.id != "app"),
            "global options are listed once"
        );
        let key = commands
            .iter()
            .find(|command| command.name() == "input key")
            .unwrap();
        assert!(key
            .notes
            .as_deref()
            .is_some_and(|notes| notes.contains("KeyCode")));
        let run = commands
            .iter()
            .find(|command| command.name() == "run")
            .unwrap();
        assert!(run.args.iter().any(|arg| arg.last));
    }

    #[test]
    fn documents_every_error_code() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut pending = vec![dir];
        let mut missing = Vec::new();
        // Split so this test doesn't find itself.
        let needle = concat!("CliError", "::new(");
        while let Some(dir) = pending.pop() {
            for entry in std::fs::read_dir(&dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    pending.push(path);
                    continue;
                }
                let text = std::fs::read_to_string(&path).unwrap();
                for (i, _) in text.match_indices(needle) {
                    let rest = text[i + needle.len()..].trim_start();
                    let Some(rest) = rest.strip_prefix('"') else {
                        continue;
                    };
                    let code = &rest[..rest.find('"').unwrap()];
                    if !ERROR_CODES.iter().any(|(known, _)| *known == code) {
                        missing.push(format!("{code} ({})", path.display()));
                    }
                }
            }
        }
        assert!(missing.is_empty(), "undocumented error codes: {missing:?}");
    }
}
