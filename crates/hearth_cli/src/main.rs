//! `hearth`: build, run, inspect and drive Hearth apps from the command line.
//!
//! The commands that work on a running app talk to it over the Hearth Remote Protocol (HRP).
//! Every command prints text for people by default and a JSON envelope with `--json`, exits
//! with a documented code, and is listed by `hearth commands`, so scripts and agents can rely
//! on it. `hearth mcp` serves the same commands as Model Context Protocol tools.

extern crate alloc;

mod base64;
mod cli;
mod commands;
mod context;
mod hrp;
mod http;
mod known;
mod mcp;
mod output;
mod project;
mod registry;
mod registry_cache;
mod session;
mod skill;
mod target;
mod value;

use std::ffi::OsString;
use std::process::ExitCode;
use std::time::Duration;

use clap::error::{ContextKind, ContextValue, ErrorKind};
use clap::{CommandFactory, FromArgMatches, ValueEnum};
use serde_json::{json, Value};

use crate::cli::{group_of, Cli, GlobalArgs, GROUPS};
use crate::context::{Attachment, Ctx, McpLimits};
use crate::output::{paint, CliError, ColorChoice, Exit, Format, Output, Style};
use crate::target::Source;

/// How long to wait for the app without `--timeout` or `HEARTH_TIMEOUT`.
const DEFAULT_TIMEOUT: f64 = 10.0;

/// The output format: `--json`, `--format`, else `HEARTH_FORMAT`, else text for people.
fn format_of(global: &GlobalArgs) -> Result<Format, CliError> {
    if global.json {
        return Ok(Format::Json);
    }
    if let Some(format) = global.format {
        return Ok(format);
    }
    match std::env::var("HEARTH_FORMAT") {
        Ok(text) if !text.trim().is_empty() => Format::from_str(text.trim(), true).map_err(|_| {
            CliError::usage(format!(
                "HEARTH_FORMAT must be human, json or ndjson, not `{text}`"
            ))
        }),
        _ => Ok(Format::Human),
    }
}

/// How long to wait for the app: `--timeout`, else `HEARTH_TIMEOUT`, else 10 seconds.
fn timeout_of(global: &GlobalArgs) -> Result<Duration, CliError> {
    let seconds = match global.timeout {
        Some(seconds) => seconds,
        None => match std::env::var("HEARTH_TIMEOUT") {
            Ok(text) if !text.trim().is_empty() => text.trim().parse().map_err(|_| {
                CliError::usage(format!(
                    "HEARTH_TIMEOUT must be a number of seconds, not `{text}`"
                ))
            })?,
            _ => DEFAULT_TIMEOUT,
        },
    };
    if !seconds.is_finite() || seconds <= 0.0 {
        return Err(CliError::usage(
            "The timeout must be a positive number of seconds",
        ));
    }
    Ok(Duration::from_secs_f64(seconds))
}

/// The format asked for on the raw command line, for reporting errors in parsing it.
fn format_in(args: &[OsString]) -> Format {
    let mut words = args.iter().map(|arg| arg.to_string_lossy());
    let mut format = None;
    while let Some(word) = words.next() {
        if word == "--" {
            break;
        }
        let value = match word.as_ref() {
            "--json" => Some("json".to_string()),
            "--format" => words.next().map(|value| value.to_string()),
            other => other.strip_prefix("--format=").map(str::to_string),
        };
        if let Some(value) = value.and_then(|value| Format::from_str(&value, true).ok()) {
            format = Some(value);
        }
    }
    format
        .or_else(|| {
            std::env::var("HEARTH_FORMAT")
                .ok()
                .and_then(|text| Format::from_str(text.trim(), true).ok())
        })
        .unwrap_or(Format::Human)
}

/// Whether help should be colored, from the raw command line and the environment.
fn help_color(args: &[OsString]) -> bool {
    use std::io::IsTerminal;
    let words: Vec<String> = args
        .iter()
        .map(|arg| arg.to_string_lossy().to_string())
        .collect();
    if words
        .iter()
        .any(|word| word == "--no-color" || word == "--color=never")
        || words
            .windows(2)
            .any(|pair| pair[0] == "--color" && pair[1] == "never")
    {
        return false;
    }
    if words.iter().any(|word| word == "--color=always")
        || words
            .windows(2)
            .any(|pair| pair[0] == "--color" && pair[1] == "always")
    {
        return true;
    }
    std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none()
}

/// The command line definition, with the commands grouped in the help.
fn cli_command(color: bool) -> clap::Command {
    // `hearth`, not the file name, which is `hearth.exe` on Windows.
    let command = Cli::command().bin_name("hearth").color(if color {
        clap::ColorChoice::Always
    } else {
        clap::ColorChoice::Never
    });
    let heading = |text: &str| {
        if color {
            format!("\x1b[1m\x1b[4m{text}:\x1b[0m")
        } else {
            format!("{text}:")
        }
    };
    let width = command
        .get_subcommands()
        .map(|sub| sub.get_name().len())
        .max()
        .unwrap_or(0);
    let mut sections = String::new();
    for group in GROUPS {
        sections.push_str(&heading(group));
        sections.push('\n');
        for sub in command
            .get_subcommands()
            .filter(|sub| group_of(sub.get_name()) == *group && !sub.is_hide_set())
        {
            let name = if color {
                format!("\x1b[1m{:<width$}\x1b[0m", sub.get_name())
            } else {
                format!("{:<width$}", sub.get_name())
            };
            let about = sub.get_about().map(ToString::to_string).unwrap_or_default();
            sections.push_str(&format!("  {name}  {about}\n"));
        }
        sections.push('\n');
    }
    let template = format!(
        "{{about-with-newline}}\n{{usage-heading}} {{usage}}\n\n{sections}{}\n{{options}}{{after-help}}",
        heading("Options")
    );
    command.help_template(template)
}

/// The words naming the command that matched, such as `resource get`.
fn command_path(matches: &clap::ArgMatches) -> String {
    let mut words = Vec::new();
    let mut current = matches;
    while let Some((name, sub)) = current.subcommand() {
        words.push(name.to_string());
        current = sub;
    }
    words.join(" ")
}

/// The subcommand words of a command line clap rejected, as far as they can be told.
fn command_in<T: AsRef<std::ffi::OsStr>>(command: &clap::Command, args: &[T]) -> String {
    let mut words = Vec::new();
    let mut current = command;
    let mut args = args
        .iter()
        .skip(1)
        .map(|arg| arg.as_ref().to_string_lossy());
    while let Some(arg) = args.next() {
        if arg == "--" {
            break;
        }
        if let Some(option) = arg.strip_prefix('-') {
            // Skip the value of an option that takes one, unless it's attached.
            let takes_value = |found: Option<&clap::Arg>| {
                found.is_some_and(|found| found.get_action().takes_values())
            };
            let found = match option.strip_prefix('-') {
                Some(long) if !long.contains('=') => takes_value(
                    current
                        .get_arguments()
                        .find(|known| known.get_long() == Some(long)),
                ),
                None if option.chars().count() == 1 => {
                    let short = option.chars().next();
                    takes_value(
                        current
                            .get_arguments()
                            .find(|known| known.get_short() == short),
                    )
                }
                _ => false,
            };
            if found {
                args.next();
            }
            continue;
        }
        match current.find_subcommand(arg.as_ref()) {
            Some(sub) => {
                words.push(sub.get_name().to_string());
                current = sub;
            }
            None if current.has_subcommands() => break,
            None => {}
        }
    }
    words.join(" ")
}

/// The error for a command line clap rejected.
fn parse_error(error: &clap::Error) -> CliError {
    let code = match error.kind() {
        ErrorKind::InvalidSubcommand => "UNKNOWN_COMMAND",
        ErrorKind::UnknownArgument => "UNKNOWN_ARGUMENT",
        ErrorKind::MissingRequiredArgument
        | ErrorKind::MissingSubcommand
        | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand => "MISSING_ARGUMENT",
        ErrorKind::InvalidValue | ErrorKind::ValueValidation | ErrorKind::InvalidUtf8 => {
            "INVALID_VALUE"
        }
        _ => "INVALID_ARGUMENTS",
    };
    let text = error.render().to_string();
    let mut lines = text.lines().filter(|line| !line.trim().is_empty());
    let message = lines
        .next()
        .unwrap_or("The command line is invalid")
        .trim_start_matches("error: ")
        .to_string();
    let mut rest: Vec<&str> = lines.collect();
    let mut message = message;
    let mut missing = Vec::new();
    if let Some(ContextValue::Strings(arguments)) = error.get(ContextKind::InvalidArg)
        && code == "MISSING_ARGUMENT"
    {
        message = format!("Missing required argument: {}", arguments.join(", "));
        rest.retain(|line| !arguments.iter().any(|argument| line.trim() == argument));
        missing.clone_from(arguments);
    }
    let mut candidates = Vec::new();
    for kind in [
        ContextKind::SuggestedSubcommand,
        ContextKind::SuggestedArg,
        ContextKind::ValidValue,
    ] {
        match error.get(kind) {
            Some(ContextValue::String(value)) => candidates.push(value.clone()),
            Some(ContextValue::Strings(values)) => candidates.extend(values.iter().cloned()),
            _ => {}
        }
    }
    let error = CliError::new(code, Exit::Usage, message).hint(if rest.is_empty() {
        "`hearth commands` lists the commands; `hearth <command> --help` explains one.".to_string()
    } else {
        rest.join("\n").trim().to_string()
    });
    if !missing.is_empty() {
        error.data(json!({ "missing": missing }))
    } else if candidates.is_empty() {
        error
    } else {
        error.data(json!({ "candidates": candidates }))
    }
}

/// Adds the apps that are running to an error about the target app not answering.
fn with_running_apps(ctx: &mut Ctx, error: CliError) -> CliError {
    if error.code != "APP_UNREACHABLE" {
        return error;
    }
    let Ok(target) = ctx.target() else {
        return error;
    };
    if !hrp::is_loopback(&target.endpoint.host)
        || target.source == Source::Flag && target.session.is_some()
    {
        return error;
    }
    let port = target.endpoint.port;
    let found: Vec<String> = target::scan(&target::scan_ports())
        .into_iter()
        .filter(|found| {
            found.endpoint.port != port
                && found
                    .info
                    .as_ref()
                    .is_none_or(|info| info["sub_app"] != "render")
        })
        .map(|found| found.describe())
        .collect();
    if found.is_empty() {
        return error;
    }
    let mut data = error.data.clone().unwrap_or_else(|| json!({}));
    data["candidates"] = json!(found);
    error
        .hint("Other apps answer HRP on this machine: pass --app with one of their ports.")
        .data(data)
}

/// The context for a parsed command line.
fn context(global: &GlobalArgs, format: Format) -> Result<Ctx, CliError> {
    let color = if global.no_color {
        ColorChoice::Never
    } else {
        global.color
    };
    let out = Output::new(format, color, global.quiet);
    Ok(Ctx::new(out, global.app.clone(), timeout_of(global)?))
}

/// Runs a command line as the MCP server does: nothing is printed, and the JSON envelope and
/// any images are returned.
pub fn run_captured(
    argv: Vec<String>,
    app: Option<String>,
    timeout: Duration,
    limits: McpLimits,
) -> (Value, Vec<Attachment>) {
    let mut out = Output::new(Format::Json, ColorChoice::Never, true);
    out.capture = true;
    let failed = |out: &Output, error: CliError| (out.envelope(&Err(error)), Vec::new());
    let matches = match cli_command(false).try_get_matches_from(&argv) {
        Ok(matches) => matches,
        Err(error) => {
            out.command = command_in(&cli_command(false), &argv);
            return failed(&out, parse_error(&error));
        }
    };
    out.command = command_path(&matches);
    let cli = match Cli::from_arg_matches(&matches) {
        Ok(cli) => cli,
        Err(error) => return failed(&out, parse_error(&error)),
    };
    let timeout = match cli.global.timeout {
        Some(_) => match timeout_of(&cli.global) {
            Ok(timeout) => timeout,
            Err(error) => return failed(&out, error),
        },
        None => timeout,
    };
    let mut ctx = Ctx::new(out, cli.global.app.clone().or(app), timeout);
    ctx.mcp = Some(limits);
    let result = commands::dispatch(&mut ctx, cli.command);
    let result = result.map_err(|error| with_running_apps(&mut ctx, error));
    (
        ctx.out.envelope(&result),
        core::mem::take(&mut ctx.attachments),
    )
}

fn exit_code(exit: Exit) -> ExitCode {
    ExitCode::from(exit as u8)
}

fn main() -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().collect();
    let color = help_color(&args);
    let matches = match cli_command(color).try_get_matches_from(&args) {
        Ok(matches) => matches,
        Err(error) => {
            if matches!(
                error.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) {
                let _ = error.print();
                return ExitCode::SUCCESS;
            }
            let format = format_in(&args);
            if format == Format::Human {
                let _ = error.print();
                return exit_code(Exit::Usage);
            }
            let mut out = Output::new(format, ColorChoice::Never, false);
            out.command = command_in(&cli_command(false), &args);
            out.finish(&Err(parse_error(&error)));
            return exit_code(Exit::Usage);
        }
    };
    let cli = match Cli::from_arg_matches(&matches) {
        Ok(cli) => cli,
        Err(error) => {
            let _ = error.print();
            return exit_code(Exit::Usage);
        }
    };
    let format = match format_of(&cli.global) {
        Ok(format) => format,
        Err(error) => {
            Output::new(Format::Human, ColorChoice::Never, false).finish(&Err(error));
            return exit_code(Exit::Usage);
        }
    };
    let mut ctx = match context(&cli.global, format) {
        Ok(ctx) => ctx,
        Err(error) => {
            Output::new(format, ColorChoice::Never, false).finish(&Err(error));
            return exit_code(Exit::Usage);
        }
    };
    ctx.out.command = command_path(&matches);
    let result = commands::dispatch(&mut ctx, cli.command);
    let result = result.map_err(|error| with_running_apps(&mut ctx, error));
    ctx.out.finish(&result);
    match result {
        Ok(_) => exit_code(Exit::Ok),
        Err(error) => {
            if error.code == "INTERNAL_ERROR" && format == Format::Human {
                let note = paint(
                    ctx.out.color,
                    "This is a bug in the CLI; please report it.",
                    Style::Dim,
                );
                ctx.out.info(note);
            }
            exit_code(error.exit)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(line: &str) -> Vec<OsString> {
        line.split_whitespace().map(OsString::from).collect()
    }

    #[test]
    fn parses_every_example() {
        let command = cli_command(false);
        for info in commands::manifest() {
            for example in &info.examples {
                let line = example.split('#').next().unwrap_or_default();
                // Quoted arguments contain spaces; parse them as one word.
                let mut argv = Vec::new();
                let mut quoted: Option<String> = None;
                for word in line.split_whitespace() {
                    match &mut quoted {
                        Some(text) => {
                            text.push(' ');
                            text.push_str(word);
                            if word.ends_with('\'') || word.ends_with('"') {
                                argv.push(quoted.take().unwrap());
                            }
                        }
                        None if (word.starts_with('\'') || word.starts_with('"'))
                            && !(word.len() > 1
                                && (word.ends_with('\'') || word.ends_with('"'))) =>
                        {
                            quoted = Some(word.to_string());
                        }
                        None => argv.push(word.to_string()),
                    }
                }
                if argv.first().map(String::as_str) != Some("hearth") {
                    continue;
                }
                let result = command.clone().try_get_matches_from(&argv);
                assert!(
                    result.is_ok(),
                    "`{example}` doesn't parse: {:?}",
                    result.err().map(|error| error.to_string())
                );
            }
        }
    }

    #[test]
    fn reads_the_format_from_raw_arguments() {
        assert_eq!(format_in(&words("hearth get x --json")), Format::Json);
        assert_eq!(
            format_in(&words("hearth get x --format ndjson")),
            Format::Ndjson
        );
        assert_eq!(
            format_in(&words("hearth get x --format=json")),
            Format::Json
        );
        assert_eq!(format_in(&words("hearth run -- --json")), Format::Human);
    }

    #[test]
    fn reports_parse_errors_with_codes() {
        let error = cli_command(false)
            .try_get_matches_from(words("hearth tre"))
            .unwrap_err();
        let error = parse_error(&error);
        assert_eq!(error.code, "UNKNOWN_COMMAND");
        assert!(error.data.unwrap()["candidates"]
            .as_array()
            .unwrap()
            .contains(&json!("tree")));
        let error = cli_command(false)
            .try_get_matches_from(words("hearth get"))
            .unwrap_err();
        let error = parse_error(&error);
        assert_eq!(error.code, "MISSING_ARGUMENT");
        assert_eq!(error.message, "Missing required argument: <ENTITY>");
    }

    #[test]
    fn no_option_shadows_a_global_one() {
        fn check(command: &clap::Command, globals: &[&str]) {
            for arg in command.get_arguments() {
                if let Some(long) = arg.get_long() {
                    assert!(
                        arg.is_global_set() || !globals.contains(&long),
                        "`{}` has its own `--{long}`, which hides the global one",
                        command.get_name()
                    );
                }
            }
            for sub in command.get_subcommands() {
                check(sub, globals);
            }
        }
        let command = Cli::command();
        let globals: Vec<&str> = command
            .get_arguments()
            .filter(|arg| arg.is_global_set())
            .filter_map(clap::Arg::get_long)
            .collect();
        assert!(globals.contains(&"json"));
        check(&command, &globals);
    }

    #[test]
    fn finds_the_command_of_rejected_lines() {
        let command = cli_command(false);
        assert_eq!(command_in(&command, &words("hearth get")), "get");
        assert_eq!(
            command_in(&command, &words("hearth --app tree resource get")),
            "resource get"
        );
        assert_eq!(
            command_in(&command, &words("hearth -q input key --hold 5")),
            "input key"
        );
        assert_eq!(command_in(&command, &words("hearth frobnicate")), "");
        assert_eq!(command_in(&command, &words("hearth run -- tree")), "run");
    }

    #[test]
    fn groups_the_help() {
        let help = cli_command(false).render_help().to_string();
        let project = help.find("Project:").unwrap();
        let live = help.find("Live app (over HRP):").unwrap();
        let agents = help.find("Agents and scripting:").unwrap();
        assert!(project < live && live < agents);
        assert!(help.contains("  screenshot"));
        assert!(help.contains("Options:"));
    }
}
