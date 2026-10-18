//! `hearth input`: keyboard and mouse input for the app's window.
//!
//! Input is written as `WindowEvent` messages, which Hearth splits into the typed messages that
//! `ButtonInput`, picking and UI read, and which also move the window's cursor. Hearth 0.19 and
//! older don't split them, so for those the typed messages are written as well. The window
//! doesn't need focus.

use std::time::Duration;

use serde_json::{json, Value};

use super::app::primary_window;
use crate::cli::{Button, InputCommand};
use crate::context::{entity_text, Ctx};
use crate::known;
use crate::output::{CliError, Exit, Outcome};
use crate::registry::edit_distance;

/// A key as Hearth's `KeyboardInput` describes it.
#[derive(Clone, Debug, PartialEq)]
struct Key {
    /// The physical key, a `KeyCode`.
    code: Value,
    /// The logical key, a `Key`.
    logical: Value,
    /// The text the key types, if any.
    text: Option<String>,
}

/// Named keys whose `KeyCode` and logical `Key` share a name.
const NAMED: &[&str] = &[
    "Tab",
    "Escape",
    "Backspace",
    "Delete",
    "Insert",
    "Home",
    "End",
    "PageUp",
    "PageDown",
    "ArrowUp",
    "ArrowDown",
    "ArrowLeft",
    "ArrowRight",
    "CapsLock",
    "NumLock",
    "ScrollLock",
    "PrintScreen",
    "Pause",
    "ContextMenu",
];

/// The typed character of punctuation keys.
const PUNCTUATION: &[(&str, char)] = &[
    ("Minus", '-'),
    ("Equal", '='),
    ("Comma", ','),
    ("Period", '.'),
    ("Slash", '/'),
    ("Semicolon", ';'),
    ("Quote", '\''),
    ("BracketLeft", '['),
    ("BracketRight", ']'),
    ("Backslash", '\\'),
    ("Backquote", '`'),
];

fn character(c: char) -> (Value, Option<String>) {
    (json!({ "Character": c.to_string() }), Some(c.to_string()))
}

/// The logical key and text of a key code, as a US keyboard layout types them.
fn key_for_code(code: &str) -> Key {
    let single = |rest: &str| {
        let mut chars = rest.chars();
        match (chars.next(), chars.next()) {
            (Some(c), None) => Some(c),
            _ => None,
        }
    };
    let (logical, text) = if let Some(letter) = code.strip_prefix("Key").and_then(single) {
        character(letter.to_ascii_lowercase())
    } else if let Some(digit) = code
        .strip_prefix("Digit")
        .or_else(|| code.strip_prefix("Numpad"))
        .and_then(single)
        .filter(char::is_ascii_digit)
    {
        character(digit)
    } else if let Some((_, c)) = PUNCTUATION.iter().find(|(name, _)| *name == code) {
        character(*c)
    } else {
        let named = match code {
            "Space" => {
                return Key {
                    code: json!(code),
                    logical: json!("Space"),
                    text: Some(" ".into()),
                }
            }
            "ShiftLeft" | "ShiftRight" => "Shift",
            "ControlLeft" | "ControlRight" => "Control",
            "AltLeft" | "AltRight" => "Alt",
            "SuperLeft" | "SuperRight" => "Super",
            "Enter" | "NumpadEnter" => "Enter",
            other if NAMED.contains(&other) => other,
            other
                if other
                    .strip_prefix('F')
                    .is_some_and(|n| n.parse::<u8>().is_ok_and(|n| (1..=35).contains(&n))) =>
            {
                other
            }
            _ => {
                return Key {
                    code: json!(code),
                    logical: json!({ "Unidentified": "Unidentified" }),
                    text: None,
                }
            }
        };
        (json!(named), None)
    };
    Key {
        code: json!(code),
        logical,
        text,
    }
}

/// The key that types `c` on a US keyboard layout.
fn key_for_char(c: char) -> Key {
    let code = match c {
        'a'..='z' | 'A'..='Z' => format!("Key{}", c.to_ascii_uppercase()),
        '0'..='9' => format!("Digit{c}"),
        ' ' => "Space".to_string(),
        '\n' | '\r' => "Enter".to_string(),
        '\t' => "Tab".to_string(),
        _ => match PUNCTUATION.iter().find(|(_, typed)| *typed == c) {
            Some((name, _)) => name.to_string(),
            None => {
                let (logical, text) = character(c);
                return Key {
                    code: json!({ "Unidentified": "Unidentified" }),
                    logical,
                    text,
                };
            }
        },
    };
    let mut key = key_for_code(&code);
    if c.is_alphabetic() || c.is_ascii_digit() || PUNCTUATION.iter().any(|(_, typed)| *typed == c) {
        key.logical = json!({ "Character": c.to_string() });
        key.text = Some(c.to_string());
    }
    key
}

/// Splits `ctrl+shift+s` into its keys. A lone `+` is a key itself.
fn split_chord(text: &str) -> Vec<&str> {
    if text.len() > 1 && text.contains('+') {
        let mut keys: Vec<&str> = text.split('+').filter(|key| !key.is_empty()).collect();
        // `ctrl++` ends with the `+` key.
        if text.ends_with("++") {
            keys.push("+");
        }
        keys
    } else {
        vec![text]
    }
}

/// Resolves a key argument: an alias, a letter or digit, or a `KeyCode` name in any case.
fn parse_key(text: &str, codes: &[String]) -> Result<Key, CliError> {
    let lower = text.to_lowercase();
    let alias = match lower.as_str() {
        "space" | " " => Some("Space"),
        "enter" | "return" => Some("Enter"),
        "esc" | "escape" => Some("Escape"),
        "tab" => Some("Tab"),
        "backspace" => Some("Backspace"),
        "delete" | "del" => Some("Delete"),
        "up" => Some("ArrowUp"),
        "down" => Some("ArrowDown"),
        "left" => Some("ArrowLeft"),
        "right" => Some("ArrowRight"),
        "shift" => Some("ShiftLeft"),
        "ctrl" | "control" => Some("ControlLeft"),
        "alt" | "option" => Some("AltLeft"),
        "super" | "cmd" | "command" | "win" | "meta" => Some("SuperLeft"),
        "pageup" => Some("PageUp"),
        "pagedown" => Some("PageDown"),
        _ => None,
    };
    let mut chars = text.chars();
    let single = match (chars.next(), chars.next()) {
        (Some(c), None) => Some(c),
        _ => None,
    };
    let code = if let Some(alias) = alias {
        alias.to_string()
    } else if let Some(c) = single.filter(char::is_ascii_alphanumeric) {
        return Ok(key_for_char(c.to_ascii_lowercase()));
    } else if let Some(code) = codes.iter().find(|code| code.to_lowercase() == lower) {
        code.clone()
    } else if codes.is_empty() {
        text.to_string()
    } else {
        let mut close: Vec<&String> = codes
            .iter()
            .filter(|code| {
                code.to_lowercase().contains(&lower)
                    || edit_distance(&code.to_lowercase(), &lower) <= 2
            })
            .collect();
        close.truncate(10);
        return Err(CliError::new(
            "UNKNOWN_KEY",
            Exit::Usage,
            format!("`{text}` isn't a key"),
        )
        .hint("Use a `KeyCode` name such as KeyA, Digit1, ArrowUp or F5, or an alias: a-z, 0-9, space, enter, esc, tab, backspace, up, down, left, right, shift, ctrl, alt, super.")
        .data(json!({ "candidates": close })));
    };
    Ok(key_for_code(&code))
}

/// Writes input messages for one window.
struct Sender {
    window: Value,
    scale: f64,
    /// Whether the app splits `WindowEvent`s itself.
    modern: bool,
    /// Whether `WindowEvent::CursorMoved` carries a `RawCursorMoved` in physical pixels.
    raw_cursor: bool,
    sent: Vec<Value>,
}

impl Sender {
    fn write(&mut self, ctx: &mut Ctx, message: &str, value: Value) -> Result<(), CliError> {
        ctx.call(
            "world.write_message",
            json!({ "message": message, "value": value }),
        )?;
        self.sent
            .push(json!({ "message": message, "value": value }));
        Ok(())
    }

    /// Writes an input as a `WindowEvent`, and as its typed message for older versions.
    fn input(
        &mut self,
        ctx: &mut Ctx,
        variant: &str,
        typed: &str,
        value: Value,
    ) -> Result<(), CliError> {
        self.write(ctx, known::WINDOW_EVENT, json!({ variant: value }))?;
        if !self.modern {
            self.write(ctx, typed, value)?;
        }
        Ok(())
    }

    fn key(&mut self, ctx: &mut Ctx, key: &Key, pressed: bool) -> Result<(), CliError> {
        let value = json!({
            "key_code": key.code,
            "logical_key": key.logical,
            "state": if pressed { "Pressed" } else { "Released" },
            "text": if pressed { key.text.clone() } else { None },
            "repeat": false,
            "window": self.window,
        });
        self.input(ctx, "KeyboardInput", known::KEYBOARD_INPUT, value)
    }

    fn button(&mut self, ctx: &mut Ctx, button: Button, pressed: bool) -> Result<(), CliError> {
        let value = json!({
            "button": match button {
                Button::Left => "Left",
                Button::Right => "Right",
                Button::Middle => "Middle",
                Button::Back => "Back",
                Button::Forward => "Forward",
            },
            "state": if pressed { "Pressed" } else { "Released" },
            "window": self.window,
        });
        self.input(ctx, "MouseButtonInput", known::MOUSE_BUTTON_INPUT, value)
    }

    fn cursor(&mut self, ctx: &mut Ctx, x: f64, y: f64) -> Result<(), CliError> {
        if self.raw_cursor {
            let value = json!({ "window": self.window, "physical_position": [x * self.scale, y * self.scale] });
            return self.write(ctx, known::WINDOW_EVENT, json!({ "CursorMoved": value }));
        }
        let value = json!({ "window": self.window, "position": [x, y], "delta": null });
        self.input(ctx, "CursorMoved", known::CURSOR_MOVED, value)?;
        // Older versions track the cursor in the window itself, set by the windowing backend.
        let _ = ctx.call(
            "world.mutate_components",
            json!({
                "entity": self.window,
                "component": known::WINDOW,
                "path": ".internal.physical_cursor_position",
                "value": [x * self.scale, y * self.scale],
            }),
        );
        Ok(())
    }

    fn wheel(&mut self, ctx: &mut Ctx, x: f32, y: f32, pixels: bool) -> Result<(), CliError> {
        let value = json!({
            "unit": if pixels { "Pixel" } else { "Line" },
            "x": x,
            "y": y,
            "window": self.window,
            "phase": "Moved",
        });
        self.input(ctx, "MouseWheel", known::MOUSE_WHEEL, value)
    }
}

/// The `KeyCode` names the app knows, from its type registry.
fn key_codes(ctx: &mut Ctx) -> Vec<String> {
    let Ok(registry) = ctx.registry() else {
        return Vec::new();
    };
    registry
        .get(known::KEY_CODE)
        .and_then(|info| info.schema.get("oneOf"))
        .and_then(Value::as_array)
        .map(|variants| {
            variants
                .iter()
                .filter_map(|variant| match variant {
                    Value::String(name) => Some(name.clone()),
                    other => other
                        .get("shortPath")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Whether the app's `WindowEvent::CursorMoved` carries a `RawCursorMoved`.
fn uses_raw_cursor(ctx: &mut Ctx) -> bool {
    let Ok(registry) = ctx.registry() else {
        return false;
    };
    registry
        .get(known::WINDOW_EVENT)
        .and_then(|info| info.schema.get("oneOf"))
        .and_then(Value::as_array)
        .and_then(|variants| {
            variants.iter().find(|variant| {
                variant.get("shortPath").and_then(Value::as_str) == Some("CursorMoved")
            })
        })
        .and_then(|variant| variant.pointer("/prefixItems/0/type/$ref"))
        .and_then(Value::as_str)
        .is_some_and(|reference| reference.ends_with("RawCursorMoved"))
}

fn sleep_ms(ms: u64) {
    std::thread::sleep(Duration::from_millis(ms));
}

/// Runs a `hearth input` command.
pub fn run(ctx: &mut Ctx, command: InputCommand) -> Result<Outcome, CliError> {
    let Some(window) = primary_window(ctx)? else {
        return Err(CliError::new(
            "NO_WINDOW",
            Exit::Rejected,
            "The app has no primary window to send input to",
        )
        .hint("Input goes to the window marked `PrimaryWindow`; headless apps have none."));
    };
    let mut sender = Sender {
        window: window["entity"].clone(),
        scale: window["scale_factor"].as_f64().unwrap_or(1.0),
        modern: ctx.client()?.is_modern()?,
        raw_cursor: uses_raw_cursor(ctx),
        sent: Vec::new(),
    };
    let human = match command {
        InputCommand::Key {
            keys,
            hold,
            down,
            up,
        } => {
            let codes = key_codes(ctx);
            // `ctrl+s` is the same chord as `ctrl s`.
            let parsed: Vec<Key> = keys
                .iter()
                .flat_map(|key| split_chord(key))
                .map(|key| parse_key(key, &codes))
                .collect::<Result<_, _>>()?;
            if !up {
                for key in &parsed {
                    sender.key(ctx, key, true)?;
                }
            }
            if !down && !up {
                sleep_ms(hold);
            }
            if !down {
                for key in parsed.iter().rev() {
                    sender.key(ctx, key, false)?;
                }
            }
            let names: Vec<String> = parsed
                .iter()
                .map(|key| {
                    key.code
                        .as_str()
                        .map_or_else(|| key.code.to_string(), str::to_string)
                })
                .collect();
            let verb = match (down, up) {
                (true, _) => "Pressed",
                (_, true) => "Released",
                _ => "Pressed and released",
            };
            format!("{verb} {}", names.join("+"))
        }
        InputCommand::Type { text } => {
            for c in text.chars() {
                let key = key_for_char(c);
                sender.key(ctx, &key, true)?;
                sender.key(ctx, &key, false)?;
            }
            format!("Typed {} characters", text.chars().count())
        }
        InputCommand::Move { x, y } => {
            sender.cursor(ctx, x, y)?;
            format!("Moved the cursor to {x}, {y}")
        }
        InputCommand::Click {
            x,
            y,
            button,
            hold,
            double,
        } => {
            if let (Some(x), Some(y)) = (x, y) {
                sender.cursor(ctx, x, y)?;
            }
            let clicks = if double { 2 } else { 1 };
            for click in 0..clicks {
                if click > 0 {
                    sleep_ms(80);
                }
                sender.button(ctx, button, true)?;
                sleep_ms(hold);
                sender.button(ctx, button, false)?;
            }
            let at = match (x, y) {
                (Some(x), Some(y)) => format!(" at {x}, {y}"),
                _ => String::new(),
            };
            format!(
                "Clicked {button:?}{at}{}",
                if double { " twice" } else { "" }
            )
            .replace("Clicked Left", "Clicked")
        }
        InputCommand::Scroll { y, x, pixels } => {
            sender.wheel(ctx, x, y, pixels)?;
            format!("Scrolled {y} {}", if pixels { "pixels" } else { "lines" })
        }
    };
    Ok(Outcome::new(
        json!({
            "window": sender.window,
            "messages": sender.sent.len(),
            "sent": sender.sent,
        }),
        format!("{human} (window {})", entity_text(&sender.window)),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_chords() {
        assert_eq!(split_chord("ctrl+s"), vec!["ctrl", "s"]);
        assert_eq!(
            split_chord("ctrl+shift+KeyZ"),
            vec!["ctrl", "shift", "KeyZ"]
        );
        assert_eq!(split_chord("+"), vec!["+"]);
        assert_eq!(split_chord("ctrl++"), vec!["ctrl", "+"]);
        assert_eq!(split_chord("a"), vec!["a"]);
    }

    #[test]
    fn maps_keys() {
        let codes = vec![
            "KeyA".to_string(),
            "Space".to_string(),
            "F5".to_string(),
            "ArrowUp".to_string(),
        ];
        let a = parse_key("a", &codes).unwrap();
        assert_eq!(a.code, json!("KeyA"));
        assert_eq!(a.logical, json!({ "Character": "a" }));
        assert_eq!(a.text.as_deref(), Some("a"));
        assert_eq!(parse_key("space", &codes).unwrap().logical, json!("Space"));
        assert_eq!(parse_key("f5", &codes).unwrap(), key_for_code("F5"));
        assert_eq!(parse_key("F5", &codes).unwrap().logical, json!("F5"));
        assert_eq!(parse_key("up", &codes).unwrap().code, json!("ArrowUp"));
        assert_eq!(parse_key("ctrl", &codes).unwrap().logical, json!("Control"));
        assert_eq!(parse_key("Spcae", &codes).unwrap_err().code, "UNKNOWN_KEY");
    }

    #[test]
    fn maps_characters() {
        let upper = key_for_char('A');
        assert_eq!(upper.code, json!("KeyA"));
        assert_eq!(upper.text.as_deref(), Some("A"));
        assert_eq!(key_for_char('/').code, json!("Slash"));
        assert_eq!(key_for_char('é').logical, json!({ "Character": "é" }));
        assert_eq!(key_for_char('\n').logical, json!("Enter"));
    }
}
