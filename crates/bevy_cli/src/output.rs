//! Output formats, the JSON envelope, errors with stable codes, exit codes and terminal styling.
//!
//! Every command produces structured data. In `human` format it is rendered as text for people,
//! in `json` format it is wrapped in an envelope, and in `ndjson` format streams are written one
//! event per line, followed by a terminal `result` frame.

use std::io::{IsTerminal, Write};

use clap::ValueEnum;
use serde_json::{json, Value};

/// How results are written to stdout.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Format {
    /// Text for people.
    Human,
    /// One JSON envelope with `success`, `data`, `errors` and `warnings`.
    Json,
    /// One JSON object per line; streams end with a `result` frame.
    Ndjson,
}

/// When to color human output.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum ColorChoice {
    /// Color when stdout is a terminal and `NO_COLOR` is unset.
    Auto,
    /// Always color.
    Always,
    /// Never color.
    Never,
}

/// Process exit codes. Scripts and agents can branch on these without parsing output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Exit {
    /// Success.
    Ok = 0,
    /// An unexpected or local error, such as a file that couldn't be written.
    Error = 1,
    /// The command line was invalid.
    Usage = 2,
    /// No app could be reached, or the target was ambiguous.
    Unreachable = 3,
    /// The app rejected the request, or a name didn't resolve to exactly one type or entity.
    Rejected = 4,
    /// Waiting for the app, an event or a capture timed out.
    Timeout = 5,
    /// The project failed to build.
    BuildFailed = 6,
}

impl Exit {
    /// The documented exit code table, used by `bevy commands` and the skill.
    pub const TABLE: &[(i32, &str)] = &[
        (0, "success"),
        (1, "error (local failure, e.g. I/O)"),
        (2, "usage (invalid arguments)"),
        (
            3,
            "unreachable (no app answered, or the target was ambiguous)",
        ),
        (
            4,
            "rejected (the app returned an error, or a name didn't resolve)",
        ),
        (5, "timeout"),
        (6, "build failed"),
    ];
}

/// An error with a stable `code` to branch on, a message for people and an optional hint.
#[derive(Debug, Clone)]
pub struct CliError {
    /// Stable, `SCREAMING_SNAKE_CASE` identifier of the error kind.
    pub code: &'static str,
    /// What went wrong.
    pub message: String,
    /// What to do about it.
    pub hint: Option<String>,
    /// Extra structured details, such as candidates for an ambiguous name.
    pub data: Option<Value>,
    /// The process exit code this error maps to.
    pub exit: Exit,
}

impl CliError {
    /// Creates an error.
    pub fn new(code: &'static str, exit: Exit, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            hint: None,
            data: None,
            exit,
        }
    }

    /// Adds a hint.
    #[must_use]
    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    /// Adds structured data.
    #[must_use]
    pub fn data(mut self, data: Value) -> Self {
        self.data = Some(data);
        self
    }

    /// Invalid arguments.
    pub fn usage(message: impl Into<String>) -> Self {
        Self::new("INVALID_ARGUMENTS", Exit::Usage, message)
    }

    /// A local I/O failure.
    pub fn io(context: impl core::fmt::Display, error: std::io::Error) -> Self {
        Self::new("IO_ERROR", Exit::Error, format!("{context}: {error}"))
    }

    /// Something the CLI doesn't expect.
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new("INTERNAL_ERROR", Exit::Error, message)
    }

    /// The JSON form of the error, as it appears in the envelope's `errors` array.
    pub fn to_json(&self) -> Value {
        let mut value = json!({ "code": self.code, "message": self.message });
        if let Some(hint) = &self.hint {
            value["hint"] = json!(hint);
        }
        if let Some(data) = &self.data {
            value["data"] = data.clone();
        }
        value
    }
}

impl core::fmt::Display for CliError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.message)
    }
}

/// The result of a command: data for machines and, optionally, text for people.
#[derive(Debug, Default)]
pub struct Outcome {
    /// The structured result. This is the envelope's `data`.
    pub data: Value,
    /// The human rendering. When absent, `data` is printed as indented JSON.
    pub human: Option<String>,
}

impl Outcome {
    /// An outcome with structured data and a human rendering.
    pub fn new(data: Value, human: impl Into<String>) -> Self {
        Self {
            data,
            human: Some(human.into()),
        }
    }

    /// An outcome rendered as JSON for people too.
    pub fn data(data: Value) -> Self {
        Self { data, human: None }
    }
}

/// Writes results in the selected format and collects warnings.
pub struct Output {
    /// The selected format.
    pub format: Format,
    /// Whether human output is colored.
    pub color: bool,
    /// Whether informational output on stderr is suppressed.
    pub quiet: bool,
    /// Warnings gathered while running, reported in the envelope.
    pub warnings: Vec<String>,
    /// Streamed events, collected instead of printed in `json` format and in MCP mode.
    pub collected: Vec<Value>,
    /// When set, nothing is printed: results are only collected. Used by the MCP server.
    pub capture: bool,
    /// The command path, such as `resource get`, reported in the envelope.
    pub command: String,
}

impl Output {
    /// Creates an output for the given format.
    pub fn new(format: Format, color: ColorChoice, quiet: bool) -> Self {
        let color = match color {
            ColorChoice::Always => true,
            ColorChoice::Never => false,
            ColorChoice::Auto => {
                std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none()
            }
        };
        Self {
            format,
            color: color && format == Format::Human,
            quiet,
            warnings: Vec::new(),
            collected: Vec::new(),
            capture: false,
            command: String::new(),
        }
    }

    /// Records a warning. Human output prints it to stderr right away.
    pub fn warn(&mut self, message: impl Into<String>) {
        let message = message.into();
        if self.format == Format::Human && !self.capture {
            let mut stderr = std::io::stderr().lock();
            let _ = writeln!(stderr, "{} {message}", self.paint("warning:", Style::Warn));
        }
        self.warnings.push(message);
    }

    /// Prints progress information to stderr, unless quiet, captured or machine-readable.
    pub fn info(&self, message: impl AsRef<str>) {
        if self.quiet || self.capture || self.format != Format::Human {
            return;
        }
        let mut stderr = std::io::stderr().lock();
        let _ = writeln!(stderr, "{}", message.as_ref());
    }

    /// Emits one event of a stream: a line of text for people, a line of JSON for `ndjson`,
    /// and a collected item for `json` and the MCP server.
    pub fn event(&mut self, data: Value, human: impl FnOnce() -> String) {
        if self.capture || self.format == Format::Json {
            self.collected.push(data);
            return;
        }
        let mut stdout = std::io::stdout().lock();
        let _ = match self.format {
            Format::Human => writeln!(stdout, "{}", human()),
            _ => writeln!(stdout, "{}", json!({ "type": "event", "data": data })),
        };
        let _ = stdout.flush();
    }

    /// Prints the final result of a command.
    pub fn finish(&mut self, result: &Result<Outcome, CliError>) {
        if self.capture {
            return;
        }
        let mut stdout = std::io::stdout().lock();
        match self.format {
            Format::Human => match result {
                Ok(outcome) => {
                    let text = match &outcome.human {
                        Some(text) => text.clone(),
                        None if outcome.data.is_null() => String::new(),
                        None => serde_json::to_string_pretty(&outcome.data).unwrap_or_default(),
                    };
                    if !text.is_empty() {
                        let _ = writeln!(stdout, "{}", text.trim_end_matches('\n'));
                    }
                }
                Err(error) => {
                    let mut stderr = std::io::stderr().lock();
                    let _ = writeln!(
                        stderr,
                        "{} {}",
                        self.paint("error:", Style::Error),
                        error.message
                    );
                    if let Some(hint) = &error.hint {
                        let _ = writeln!(stderr, "{} {hint}", self.paint("hint:", Style::Dim));
                    }
                    if let Some(candidates) = error
                        .data
                        .as_ref()
                        .and_then(|data| data.get("candidates"))
                        .and_then(Value::as_array)
                    {
                        for candidate in candidates.iter().take(20) {
                            let text = match candidate {
                                Value::String(text) => text.clone(),
                                other => other.to_string(),
                            };
                            let _ = writeln!(stderr, "  {text}");
                        }
                    }
                }
            },
            Format::Json => {
                let envelope = self.envelope(result);
                let _ = writeln!(
                    stdout,
                    "{}",
                    serde_json::to_string_pretty(&envelope).unwrap_or_default()
                );
            }
            Format::Ndjson => {
                if let Ok(outcome) = result
                    && let Value::Array(items) = &outcome.data
                {
                    for item in items {
                        let _ = writeln!(stdout, "{}", json!({ "type": "item", "data": item }));
                    }
                }
                let mut envelope = self.envelope(result);
                envelope["type"] = json!("result");
                if let Ok(outcome) = result
                    && let Value::Array(items) = &outcome.data
                {
                    envelope["data"] = json!({ "count": items.len() });
                }
                let _ = writeln!(stdout, "{envelope}");
            }
        }
        let _ = stdout.flush();
    }

    /// Builds the JSON envelope for a result.
    pub fn envelope(&self, result: &Result<Outcome, CliError>) -> Value {
        let (success, data, errors) = match result {
            Ok(outcome) => {
                let mut data = outcome.data.clone();
                if !self.collected.is_empty() {
                    data = merge_events(data, &self.collected);
                }
                (true, data, Vec::new())
            }
            Err(error) => (false, Value::Null, vec![error.to_json()]),
        };
        json!({
            "success": success,
            "command": self.command,
            "data": data,
            "errors": errors,
            "warnings": self.warnings,
        })
    }

    /// Applies a style when color is enabled.
    pub fn paint(&self, text: &str, style: Style) -> String {
        paint(self.color, text, style)
    }
}

/// Puts collected stream events into a command's data as its `events`.
fn merge_events(data: Value, events: &[Value]) -> Value {
    match data {
        Value::Object(mut map) => {
            map.insert("events".to_string(), Value::Array(events.to_vec()));
            Value::Object(map)
        }
        Value::Null => json!({ "events": events }),
        other => json!({ "result": other, "events": events }),
    }
}

/// Text styles for human output.
#[derive(Clone, Copy, Debug)]
pub enum Style {
    /// Bold, for headings.
    Bold,
    /// Dimmed, for secondary information.
    Dim,
    /// Cyan, for entity ids and addresses.
    Id,
    /// Green, for type names and success.
    Type,
    /// Yellow, for warnings.
    Warn,
    /// Red, for errors.
    Error,
}

/// Wraps `text` in ANSI escapes for `style` when `enabled`.
pub fn paint(enabled: bool, text: &str, style: Style) -> String {
    if !enabled || text.is_empty() {
        return text.to_string();
    }
    let code = match style {
        Style::Bold => "1",
        Style::Dim => "2",
        Style::Id => "36",
        Style::Type => "32",
        Style::Warn => "33",
        Style::Error => "31;1",
    };
    format!("\x1b[{code}m{text}\x1b[0m")
}

/// Removes ANSI escape sequences from `text`.
pub fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for c in chars.by_ref() {
                    if c.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

/// Renders rows as a left-aligned table with a header, sized to the widest cell of each column.
pub fn table(color: bool, header: &[&str], rows: &[Vec<String>]) -> String {
    let columns = header.len();
    let mut widths: Vec<usize> = header.iter().map(|h| h.chars().count()).collect();
    for row in rows {
        for (i, cell) in row.iter().enumerate().take(columns) {
            widths[i] = widths[i].max(strip_ansi(cell).chars().count());
        }
    }
    let mut out = String::new();
    let render = |out: &mut String, cells: Vec<String>| {
        let mut line = String::new();
        for (i, cell) in cells.iter().enumerate() {
            let width = strip_ansi(cell).chars().count();
            line.push_str(cell);
            if i + 1 < cells.len() {
                line.push_str(&" ".repeat(widths[i].saturating_sub(width) + 2));
            }
        }
        out.push_str(line.trim_end());
        out.push('\n');
    };
    render(
        &mut out,
        header
            .iter()
            .map(|h| paint(color, h, Style::Bold))
            .collect(),
    );
    for row in rows {
        render(&mut out, row.clone());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_ansi_sequences() {
        assert_eq!(strip_ansi("\x1b[31;1merror\x1b[0m: x"), "error: x");
        assert_eq!(strip_ansi("plain"), "plain");
    }

    #[test]
    fn tables_align_columns() {
        let text = table(
            false,
            &["A", "LONGER"],
            &[
                vec!["xyz".into(), "1".into()],
                vec!["q".into(), "22".into()],
            ],
        );
        assert_eq!(text, "A    LONGER\nxyz  1\nq    22\n");
    }

    #[test]
    fn envelope_reports_errors() {
        let mut output = Output::new(Format::Json, ColorChoice::Never, false);
        output.command = "get".into();
        let error = CliError::new("ENTITY_NOT_FOUND", Exit::Rejected, "gone").hint("look");
        let envelope = output.envelope(&Err(error));
        assert_eq!(envelope["success"], json!(false));
        assert_eq!(envelope["errors"][0]["code"], json!("ENTITY_NOT_FOUND"));
        assert_eq!(envelope["errors"][0]["hint"], json!("look"));
        assert_eq!(envelope["command"], json!("get"));
    }

    #[test]
    fn collected_events_join_the_data() {
        let mut output = Output::new(Format::Json, ColorChoice::Never, false);
        output.event(json!(1), String::new);
        output.event(json!(2), String::new);
        let envelope = output.envelope(&Ok(Outcome::data(json!({ "count": 2 }))));
        assert_eq!(envelope["data"]["events"], json!([1, 2]));
        assert_eq!(envelope["data"]["count"], json!(2));
    }
}
