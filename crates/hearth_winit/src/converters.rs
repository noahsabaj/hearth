//! Helpers for mapping between winit and hearth types
use hearth_ecs::entity::Entity;
use hearth_input::{
    keyboard::{KeyCode, KeyboardInput, NativeKeyCode},
    mouse::MouseButton,
    touch::{ForceTouch, TouchInput, TouchPhase},
    ButtonState,
};
use hearth_math::{CompassOctant, Vec2};
use hearth_window::SystemCursorIcon;
use hearth_window::{EnabledButtons, WindowLevel, WindowTheme};
use winit::keyboard::{Key, NamedKey, NativeKey};

#[cfg(target_os = "ios")]
use hearth_window::ScreenEdge;

/// Converts a [`winit::event::KeyEvent`] and a window [`Entity`] to a Hearth [`KeyboardInput`]
pub fn convert_keyboard_input(
    keyboard_input: &winit::event::KeyEvent,
    window: Entity,
) -> KeyboardInput {
    KeyboardInput {
        state: convert_element_state(keyboard_input.state),
        key_code: convert_physical_key_code(keyboard_input.physical_key),
        logical_key: convert_logical_key(&keyboard_input.logical_key),
        text: keyboard_input.text.clone(),
        repeat: keyboard_input.repeat,
        window,
    }
}

/// Converts a [`winit::event::ElementState`] to a Hearth [`ButtonState`]
pub fn convert_element_state(element_state: winit::event::ElementState) -> ButtonState {
    match element_state {
        winit::event::ElementState::Pressed => ButtonState::Pressed,
        winit::event::ElementState::Released => ButtonState::Released,
    }
}

/// Converts a [`winit::event::MouseButton`] to a Hearth [`MouseButton`]
pub fn convert_mouse_button(mouse_button: winit::event::MouseButton) -> MouseButton {
    match mouse_button {
        winit::event::MouseButton::Left => MouseButton::Left,
        winit::event::MouseButton::Right => MouseButton::Right,
        winit::event::MouseButton::Middle => MouseButton::Middle,
        winit::event::MouseButton::Back => MouseButton::Back,
        winit::event::MouseButton::Forward => MouseButton::Forward,
        winit::event::MouseButton::Other(val) => MouseButton::Other(val),
    }
}

/// Converts a [`winit::event::TouchPhase`] to a Hearth [`TouchPhase`].
pub fn convert_touch_phase(phase: winit::event::TouchPhase) -> TouchPhase {
    match phase {
        winit::event::TouchPhase::Started => TouchPhase::Started,
        winit::event::TouchPhase::Moved => TouchPhase::Moved,
        winit::event::TouchPhase::Ended => TouchPhase::Ended,
        winit::event::TouchPhase::Cancelled => TouchPhase::Canceled,
    }
}

/// Converts a [`winit::event::Touch`], [`winit::dpi::LogicalPosition<f64>`] and window [`Entity`] to a Hearth [`TouchInput`]
pub fn convert_touch_input(
    touch_input: winit::event::Touch,
    location: winit::dpi::LogicalPosition<f64>,
    window_entity: Entity,
) -> TouchInput {
    TouchInput {
        phase: convert_touch_phase(touch_input.phase),
        position: Vec2::new(location.x as f32, location.y as f32),
        window: window_entity,
        force: touch_input.force.map(|f| match f {
            winit::event::Force::Calibrated {
                force,
                max_possible_force,
                altitude_angle,
            } => ForceTouch::Calibrated {
                force,
                max_possible_force,
                altitude_angle,
            },
            winit::event::Force::Normalized(x) => ForceTouch::Normalized(x),
        }),
        id: touch_input.id,
    }
}

/// Converts a [`winit::keyboard::NativeKeyCode`] to a Hearth [`NativeKeyCode`]
pub fn convert_physical_native_key_code(
    native_key_code: winit::keyboard::NativeKeyCode,
) -> NativeKeyCode {
    match native_key_code {
        winit::keyboard::NativeKeyCode::Unidentified => NativeKeyCode::Unidentified,
        winit::keyboard::NativeKeyCode::Android(scan_code) => NativeKeyCode::Android(scan_code),
        winit::keyboard::NativeKeyCode::MacOS(scan_code) => NativeKeyCode::MacOS(scan_code),
        winit::keyboard::NativeKeyCode::Windows(scan_code) => NativeKeyCode::Windows(scan_code),
        winit::keyboard::NativeKeyCode::Xkb(key_code) => NativeKeyCode::Xkb(key_code),
    }
}
/// Converts a [`winit::keyboard::PhysicalKey`] to a Hearth [`KeyCode`]
pub fn convert_physical_key_code(virtual_key_code: winit::keyboard::PhysicalKey) -> KeyCode {
    match virtual_key_code {
        winit::keyboard::PhysicalKey::Unidentified(native_key_code) => {
            KeyCode::Unidentified(convert_physical_native_key_code(native_key_code))
        }
        winit::keyboard::PhysicalKey::Code(code) => match code {
            winit::keyboard::KeyCode::Backquote => KeyCode::Backquote,
            winit::keyboard::KeyCode::Backslash => KeyCode::Backslash,
            winit::keyboard::KeyCode::BracketLeft => KeyCode::BracketLeft,
            winit::keyboard::KeyCode::BracketRight => KeyCode::BracketRight,
            winit::keyboard::KeyCode::Comma => KeyCode::Comma,
            winit::keyboard::KeyCode::Digit0 => KeyCode::Digit0,
            winit::keyboard::KeyCode::Digit1 => KeyCode::Digit1,
            winit::keyboard::KeyCode::Digit2 => KeyCode::Digit2,
            winit::keyboard::KeyCode::Digit3 => KeyCode::Digit3,
            winit::keyboard::KeyCode::Digit4 => KeyCode::Digit4,
            winit::keyboard::KeyCode::Digit5 => KeyCode::Digit5,
            winit::keyboard::KeyCode::Digit6 => KeyCode::Digit6,
            winit::keyboard::KeyCode::Digit7 => KeyCode::Digit7,
            winit::keyboard::KeyCode::Digit8 => KeyCode::Digit8,
            winit::keyboard::KeyCode::Digit9 => KeyCode::Digit9,
            winit::keyboard::KeyCode::Equal => KeyCode::Equal,
            winit::keyboard::KeyCode::IntlBackslash => KeyCode::IntlBackslash,
            winit::keyboard::KeyCode::IntlRo => KeyCode::IntlRo,
            winit::keyboard::KeyCode::IntlYen => KeyCode::IntlYen,
            winit::keyboard::KeyCode::KeyA => KeyCode::KeyA,
            winit::keyboard::KeyCode::KeyB => KeyCode::KeyB,
            winit::keyboard::KeyCode::KeyC => KeyCode::KeyC,
            winit::keyboard::KeyCode::KeyD => KeyCode::KeyD,
            winit::keyboard::KeyCode::KeyE => KeyCode::KeyE,
            winit::keyboard::KeyCode::KeyF => KeyCode::KeyF,
            winit::keyboard::KeyCode::KeyG => KeyCode::KeyG,
            winit::keyboard::KeyCode::KeyH => KeyCode::KeyH,
            winit::keyboard::KeyCode::KeyI => KeyCode::KeyI,
            winit::keyboard::KeyCode::KeyJ => KeyCode::KeyJ,
            winit::keyboard::KeyCode::KeyK => KeyCode::KeyK,
            winit::keyboard::KeyCode::KeyL => KeyCode::KeyL,
            winit::keyboard::KeyCode::KeyM => KeyCode::KeyM,
            winit::keyboard::KeyCode::KeyN => KeyCode::KeyN,
            winit::keyboard::KeyCode::KeyO => KeyCode::KeyO,
            winit::keyboard::KeyCode::KeyP => KeyCode::KeyP,
            winit::keyboard::KeyCode::KeyQ => KeyCode::KeyQ,
            winit::keyboard::KeyCode::KeyR => KeyCode::KeyR,
            winit::keyboard::KeyCode::KeyS => KeyCode::KeyS,
            winit::keyboard::KeyCode::KeyT => KeyCode::KeyT,
            winit::keyboard::KeyCode::KeyU => KeyCode::KeyU,
            winit::keyboard::KeyCode::KeyV => KeyCode::KeyV,
            winit::keyboard::KeyCode::KeyW => KeyCode::KeyW,
            winit::keyboard::KeyCode::KeyX => KeyCode::KeyX,
            winit::keyboard::KeyCode::KeyY => KeyCode::KeyY,
            winit::keyboard::KeyCode::KeyZ => KeyCode::KeyZ,
            winit::keyboard::KeyCode::Minus => KeyCode::Minus,
            winit::keyboard::KeyCode::Period => KeyCode::Period,
            winit::keyboard::KeyCode::Quote => KeyCode::Quote,
            winit::keyboard::KeyCode::Semicolon => KeyCode::Semicolon,
            winit::keyboard::KeyCode::Slash => KeyCode::Slash,
            winit::keyboard::KeyCode::AltLeft => KeyCode::AltLeft,
            winit::keyboard::KeyCode::AltRight => KeyCode::AltRight,
            winit::keyboard::KeyCode::Backspace => KeyCode::Backspace,
            winit::keyboard::KeyCode::CapsLock => KeyCode::CapsLock,
            winit::keyboard::KeyCode::ContextMenu => KeyCode::ContextMenu,
            winit::keyboard::KeyCode::ControlLeft => KeyCode::ControlLeft,
            winit::keyboard::KeyCode::ControlRight => KeyCode::ControlRight,
            winit::keyboard::KeyCode::Enter => KeyCode::Enter,
            winit::keyboard::KeyCode::SuperLeft => KeyCode::SuperLeft,
            winit::keyboard::KeyCode::SuperRight => KeyCode::SuperRight,
            winit::keyboard::KeyCode::ShiftLeft => KeyCode::ShiftLeft,
            winit::keyboard::KeyCode::ShiftRight => KeyCode::ShiftRight,
            winit::keyboard::KeyCode::Space => KeyCode::Space,
            winit::keyboard::KeyCode::Tab => KeyCode::Tab,
            winit::keyboard::KeyCode::Convert => KeyCode::Convert,
            winit::keyboard::KeyCode::KanaMode => KeyCode::KanaMode,
            winit::keyboard::KeyCode::Lang1 => KeyCode::Lang1,
            winit::keyboard::KeyCode::Lang2 => KeyCode::Lang2,
            winit::keyboard::KeyCode::Lang3 => KeyCode::Lang3,
            winit::keyboard::KeyCode::Lang4 => KeyCode::Lang4,
            winit::keyboard::KeyCode::Lang5 => KeyCode::Lang5,
            winit::keyboard::KeyCode::NonConvert => KeyCode::NonConvert,
            winit::keyboard::KeyCode::Delete => KeyCode::Delete,
            winit::keyboard::KeyCode::End => KeyCode::End,
            winit::keyboard::KeyCode::Help => KeyCode::Help,
            winit::keyboard::KeyCode::Home => KeyCode::Home,
            winit::keyboard::KeyCode::Insert => KeyCode::Insert,
            winit::keyboard::KeyCode::PageDown => KeyCode::PageDown,
            winit::keyboard::KeyCode::PageUp => KeyCode::PageUp,
            winit::keyboard::KeyCode::ArrowDown => KeyCode::ArrowDown,
            winit::keyboard::KeyCode::ArrowLeft => KeyCode::ArrowLeft,
            winit::keyboard::KeyCode::ArrowRight => KeyCode::ArrowRight,
            winit::keyboard::KeyCode::ArrowUp => KeyCode::ArrowUp,
            winit::keyboard::KeyCode::NumLock => KeyCode::NumLock,
            winit::keyboard::KeyCode::Numpad0 => KeyCode::Numpad0,
            winit::keyboard::KeyCode::Numpad1 => KeyCode::Numpad1,
            winit::keyboard::KeyCode::Numpad2 => KeyCode::Numpad2,
            winit::keyboard::KeyCode::Numpad3 => KeyCode::Numpad3,
            winit::keyboard::KeyCode::Numpad4 => KeyCode::Numpad4,
            winit::keyboard::KeyCode::Numpad5 => KeyCode::Numpad5,
            winit::keyboard::KeyCode::Numpad6 => KeyCode::Numpad6,
            winit::keyboard::KeyCode::Numpad7 => KeyCode::Numpad7,
            winit::keyboard::KeyCode::Numpad8 => KeyCode::Numpad8,
            winit::keyboard::KeyCode::Numpad9 => KeyCode::Numpad9,
            winit::keyboard::KeyCode::NumpadAdd => KeyCode::NumpadAdd,
            winit::keyboard::KeyCode::NumpadBackspace => KeyCode::NumpadBackspace,
            winit::keyboard::KeyCode::NumpadClear => KeyCode::NumpadClear,
            winit::keyboard::KeyCode::NumpadClearEntry => KeyCode::NumpadClearEntry,
            winit::keyboard::KeyCode::NumpadComma => KeyCode::NumpadComma,
            winit::keyboard::KeyCode::NumpadDecimal => KeyCode::NumpadDecimal,
            winit::keyboard::KeyCode::NumpadDivide => KeyCode::NumpadDivide,
            winit::keyboard::KeyCode::NumpadEnter => KeyCode::NumpadEnter,
            winit::keyboard::KeyCode::NumpadEqual => KeyCode::NumpadEqual,
            winit::keyboard::KeyCode::NumpadHash => KeyCode::NumpadHash,
            winit::keyboard::KeyCode::NumpadMemoryAdd => KeyCode::NumpadMemoryAdd,
            winit::keyboard::KeyCode::NumpadMemoryClear => KeyCode::NumpadMemoryClear,
            winit::keyboard::KeyCode::NumpadMemoryRecall => KeyCode::NumpadMemoryRecall,
            winit::keyboard::KeyCode::NumpadMemoryStore => KeyCode::NumpadMemoryStore,
            winit::keyboard::KeyCode::NumpadMemorySubtract => KeyCode::NumpadMemorySubtract,
            winit::keyboard::KeyCode::NumpadMultiply => KeyCode::NumpadMultiply,
            winit::keyboard::KeyCode::NumpadParenLeft => KeyCode::NumpadParenLeft,
            winit::keyboard::KeyCode::NumpadParenRight => KeyCode::NumpadParenRight,
            winit::keyboard::KeyCode::NumpadStar => KeyCode::NumpadStar,
            winit::keyboard::KeyCode::NumpadSubtract => KeyCode::NumpadSubtract,
            winit::keyboard::KeyCode::Escape => KeyCode::Escape,
            winit::keyboard::KeyCode::Fn => KeyCode::Fn,
            winit::keyboard::KeyCode::FnLock => KeyCode::FnLock,
            winit::keyboard::KeyCode::PrintScreen => KeyCode::PrintScreen,
            winit::keyboard::KeyCode::ScrollLock => KeyCode::ScrollLock,
            winit::keyboard::KeyCode::Pause => KeyCode::Pause,
            winit::keyboard::KeyCode::BrowserBack => KeyCode::BrowserBack,
            winit::keyboard::KeyCode::BrowserFavorites => KeyCode::BrowserFavorites,
            winit::keyboard::KeyCode::BrowserForward => KeyCode::BrowserForward,
            winit::keyboard::KeyCode::BrowserHome => KeyCode::BrowserHome,
            winit::keyboard::KeyCode::BrowserRefresh => KeyCode::BrowserRefresh,
            winit::keyboard::KeyCode::BrowserSearch => KeyCode::BrowserSearch,
            winit::keyboard::KeyCode::BrowserStop => KeyCode::BrowserStop,
            winit::keyboard::KeyCode::Eject => KeyCode::Eject,
            winit::keyboard::KeyCode::LaunchApp1 => KeyCode::LaunchApp1,
            winit::keyboard::KeyCode::LaunchApp2 => KeyCode::LaunchApp2,
            winit::keyboard::KeyCode::LaunchMail => KeyCode::LaunchMail,
            winit::keyboard::KeyCode::MediaPlayPause => KeyCode::MediaPlayPause,
            winit::keyboard::KeyCode::MediaSelect => KeyCode::MediaSelect,
            winit::keyboard::KeyCode::MediaStop => KeyCode::MediaStop,
            winit::keyboard::KeyCode::MediaTrackNext => KeyCode::MediaTrackNext,
            winit::keyboard::KeyCode::MediaTrackPrevious => KeyCode::MediaTrackPrevious,
            winit::keyboard::KeyCode::Power => KeyCode::Power,
            winit::keyboard::KeyCode::Sleep => KeyCode::Sleep,
            winit::keyboard::KeyCode::AudioVolumeDown => KeyCode::AudioVolumeDown,
            winit::keyboard::KeyCode::AudioVolumeMute => KeyCode::AudioVolumeMute,
            winit::keyboard::KeyCode::AudioVolumeUp => KeyCode::AudioVolumeUp,
            winit::keyboard::KeyCode::WakeUp => KeyCode::WakeUp,
            winit::keyboard::KeyCode::Meta => KeyCode::Meta,
            winit::keyboard::KeyCode::Hyper => KeyCode::Hyper,
            winit::keyboard::KeyCode::Turbo => KeyCode::Turbo,
            winit::keyboard::KeyCode::Abort => KeyCode::Abort,
            winit::keyboard::KeyCode::Resume => KeyCode::Resume,
            winit::keyboard::KeyCode::Suspend => KeyCode::Suspend,
            winit::keyboard::KeyCode::Again => KeyCode::Again,
            winit::keyboard::KeyCode::Copy => KeyCode::Copy,
            winit::keyboard::KeyCode::Cut => KeyCode::Cut,
            winit::keyboard::KeyCode::Find => KeyCode::Find,
            winit::keyboard::KeyCode::Open => KeyCode::Open,
            winit::keyboard::KeyCode::Paste => KeyCode::Paste,
            winit::keyboard::KeyCode::Props => KeyCode::Props,
            winit::keyboard::KeyCode::Select => KeyCode::Select,
            winit::keyboard::KeyCode::Undo => KeyCode::Undo,
            winit::keyboard::KeyCode::Hiragana => KeyCode::Hiragana,
            winit::keyboard::KeyCode::Katakana => KeyCode::Katakana,
            winit::keyboard::KeyCode::F1 => KeyCode::F1,
            winit::keyboard::KeyCode::F2 => KeyCode::F2,
            winit::keyboard::KeyCode::F3 => KeyCode::F3,
            winit::keyboard::KeyCode::F4 => KeyCode::F4,
            winit::keyboard::KeyCode::F5 => KeyCode::F5,
            winit::keyboard::KeyCode::F6 => KeyCode::F6,
            winit::keyboard::KeyCode::F7 => KeyCode::F7,
            winit::keyboard::KeyCode::F8 => KeyCode::F8,
            winit::keyboard::KeyCode::F9 => KeyCode::F9,
            winit::keyboard::KeyCode::F10 => KeyCode::F10,
            winit::keyboard::KeyCode::F11 => KeyCode::F11,
            winit::keyboard::KeyCode::F12 => KeyCode::F12,
            winit::keyboard::KeyCode::F13 => KeyCode::F13,
            winit::keyboard::KeyCode::F14 => KeyCode::F14,
            winit::keyboard::KeyCode::F15 => KeyCode::F15,
            winit::keyboard::KeyCode::F16 => KeyCode::F16,
            winit::keyboard::KeyCode::F17 => KeyCode::F17,
            winit::keyboard::KeyCode::F18 => KeyCode::F18,
            winit::keyboard::KeyCode::F19 => KeyCode::F19,
            winit::keyboard::KeyCode::F20 => KeyCode::F20,
            winit::keyboard::KeyCode::F21 => KeyCode::F21,
            winit::keyboard::KeyCode::F22 => KeyCode::F22,
            winit::keyboard::KeyCode::F23 => KeyCode::F23,
            winit::keyboard::KeyCode::F24 => KeyCode::F24,
            winit::keyboard::KeyCode::F25 => KeyCode::F25,
            winit::keyboard::KeyCode::F26 => KeyCode::F26,
            winit::keyboard::KeyCode::F27 => KeyCode::F27,
            winit::keyboard::KeyCode::F28 => KeyCode::F28,
            winit::keyboard::KeyCode::F29 => KeyCode::F29,
            winit::keyboard::KeyCode::F30 => KeyCode::F30,
            winit::keyboard::KeyCode::F31 => KeyCode::F31,
            winit::keyboard::KeyCode::F32 => KeyCode::F32,
            winit::keyboard::KeyCode::F33 => KeyCode::F33,
            winit::keyboard::KeyCode::F34 => KeyCode::F34,
            winit::keyboard::KeyCode::F35 => KeyCode::F35,
            _ => KeyCode::Unidentified(NativeKeyCode::Unidentified),
        },
    }
}

///Converts a [`winit::keyboard::Key`] to a Hearth [`hearth_input::keyboard::Key`]
pub fn convert_logical_key(logical_key_code: &Key) -> hearth_input::keyboard::Key {
    match logical_key_code {
        Key::Character(s) => hearth_input::keyboard::Key::Character(s.clone()),
        Key::Unidentified(nk) => hearth_input::keyboard::Key::Unidentified(convert_native_key(nk)),
        Key::Dead(c) => hearth_input::keyboard::Key::Dead(c.to_owned()),
        Key::Named(NamedKey::Alt) => hearth_input::keyboard::Key::Alt,
        Key::Named(NamedKey::AltGraph) => hearth_input::keyboard::Key::AltGraph,
        Key::Named(NamedKey::CapsLock) => hearth_input::keyboard::Key::CapsLock,
        Key::Named(NamedKey::Control) => hearth_input::keyboard::Key::Control,
        Key::Named(NamedKey::Fn) => hearth_input::keyboard::Key::Fn,
        Key::Named(NamedKey::FnLock) => hearth_input::keyboard::Key::FnLock,
        Key::Named(NamedKey::NumLock) => hearth_input::keyboard::Key::NumLock,
        Key::Named(NamedKey::ScrollLock) => hearth_input::keyboard::Key::ScrollLock,
        Key::Named(NamedKey::Shift) => hearth_input::keyboard::Key::Shift,
        Key::Named(NamedKey::Symbol) => hearth_input::keyboard::Key::Symbol,
        Key::Named(NamedKey::SymbolLock) => hearth_input::keyboard::Key::SymbolLock,
        Key::Named(NamedKey::Meta) => hearth_input::keyboard::Key::Meta,
        Key::Named(NamedKey::Hyper) => hearth_input::keyboard::Key::Hyper,
        Key::Named(NamedKey::Super) => hearth_input::keyboard::Key::Super,
        Key::Named(NamedKey::Enter) => hearth_input::keyboard::Key::Enter,
        Key::Named(NamedKey::Tab) => hearth_input::keyboard::Key::Tab,
        Key::Named(NamedKey::Space) => hearth_input::keyboard::Key::Space,
        Key::Named(NamedKey::ArrowDown) => hearth_input::keyboard::Key::ArrowDown,
        Key::Named(NamedKey::ArrowLeft) => hearth_input::keyboard::Key::ArrowLeft,
        Key::Named(NamedKey::ArrowRight) => hearth_input::keyboard::Key::ArrowRight,
        Key::Named(NamedKey::ArrowUp) => hearth_input::keyboard::Key::ArrowUp,
        Key::Named(NamedKey::End) => hearth_input::keyboard::Key::End,
        Key::Named(NamedKey::Home) => hearth_input::keyboard::Key::Home,
        Key::Named(NamedKey::PageDown) => hearth_input::keyboard::Key::PageDown,
        Key::Named(NamedKey::PageUp) => hearth_input::keyboard::Key::PageUp,
        Key::Named(NamedKey::Backspace) => hearth_input::keyboard::Key::Backspace,
        Key::Named(NamedKey::Clear) => hearth_input::keyboard::Key::Clear,
        Key::Named(NamedKey::Copy) => hearth_input::keyboard::Key::Copy,
        Key::Named(NamedKey::CrSel) => hearth_input::keyboard::Key::CrSel,
        Key::Named(NamedKey::Cut) => hearth_input::keyboard::Key::Cut,
        Key::Named(NamedKey::Delete) => hearth_input::keyboard::Key::Delete,
        Key::Named(NamedKey::EraseEof) => hearth_input::keyboard::Key::EraseEof,
        Key::Named(NamedKey::ExSel) => hearth_input::keyboard::Key::ExSel,
        Key::Named(NamedKey::Insert) => hearth_input::keyboard::Key::Insert,
        Key::Named(NamedKey::Paste) => hearth_input::keyboard::Key::Paste,
        Key::Named(NamedKey::Redo) => hearth_input::keyboard::Key::Redo,
        Key::Named(NamedKey::Undo) => hearth_input::keyboard::Key::Undo,
        Key::Named(NamedKey::Accept) => hearth_input::keyboard::Key::Accept,
        Key::Named(NamedKey::Again) => hearth_input::keyboard::Key::Again,
        Key::Named(NamedKey::Attn) => hearth_input::keyboard::Key::Attn,
        Key::Named(NamedKey::Cancel) => hearth_input::keyboard::Key::Cancel,
        Key::Named(NamedKey::ContextMenu) => hearth_input::keyboard::Key::ContextMenu,
        Key::Named(NamedKey::Escape) => hearth_input::keyboard::Key::Escape,
        Key::Named(NamedKey::Execute) => hearth_input::keyboard::Key::Execute,
        Key::Named(NamedKey::Find) => hearth_input::keyboard::Key::Find,
        Key::Named(NamedKey::Help) => hearth_input::keyboard::Key::Help,
        Key::Named(NamedKey::Pause) => hearth_input::keyboard::Key::Pause,
        Key::Named(NamedKey::Play) => hearth_input::keyboard::Key::Play,
        Key::Named(NamedKey::Props) => hearth_input::keyboard::Key::Props,
        Key::Named(NamedKey::Select) => hearth_input::keyboard::Key::Select,
        Key::Named(NamedKey::ZoomIn) => hearth_input::keyboard::Key::ZoomIn,
        Key::Named(NamedKey::ZoomOut) => hearth_input::keyboard::Key::ZoomOut,
        Key::Named(NamedKey::BrightnessDown) => hearth_input::keyboard::Key::BrightnessDown,
        Key::Named(NamedKey::BrightnessUp) => hearth_input::keyboard::Key::BrightnessUp,
        Key::Named(NamedKey::Eject) => hearth_input::keyboard::Key::Eject,
        Key::Named(NamedKey::LogOff) => hearth_input::keyboard::Key::LogOff,
        Key::Named(NamedKey::Power) => hearth_input::keyboard::Key::Power,
        Key::Named(NamedKey::PowerOff) => hearth_input::keyboard::Key::PowerOff,
        Key::Named(NamedKey::PrintScreen) => hearth_input::keyboard::Key::PrintScreen,
        Key::Named(NamedKey::Hibernate) => hearth_input::keyboard::Key::Hibernate,
        Key::Named(NamedKey::Standby) => hearth_input::keyboard::Key::Standby,
        Key::Named(NamedKey::WakeUp) => hearth_input::keyboard::Key::WakeUp,
        Key::Named(NamedKey::AllCandidates) => hearth_input::keyboard::Key::AllCandidates,
        Key::Named(NamedKey::Alphanumeric) => hearth_input::keyboard::Key::Alphanumeric,
        Key::Named(NamedKey::CodeInput) => hearth_input::keyboard::Key::CodeInput,
        Key::Named(NamedKey::Compose) => hearth_input::keyboard::Key::Compose,
        Key::Named(NamedKey::Convert) => hearth_input::keyboard::Key::Convert,
        Key::Named(NamedKey::FinalMode) => hearth_input::keyboard::Key::FinalMode,
        Key::Named(NamedKey::GroupFirst) => hearth_input::keyboard::Key::GroupFirst,
        Key::Named(NamedKey::GroupLast) => hearth_input::keyboard::Key::GroupLast,
        Key::Named(NamedKey::GroupNext) => hearth_input::keyboard::Key::GroupNext,
        Key::Named(NamedKey::GroupPrevious) => hearth_input::keyboard::Key::GroupPrevious,
        Key::Named(NamedKey::ModeChange) => hearth_input::keyboard::Key::ModeChange,
        Key::Named(NamedKey::NextCandidate) => hearth_input::keyboard::Key::NextCandidate,
        Key::Named(NamedKey::NonConvert) => hearth_input::keyboard::Key::NonConvert,
        Key::Named(NamedKey::PreviousCandidate) => hearth_input::keyboard::Key::PreviousCandidate,
        Key::Named(NamedKey::Process) => hearth_input::keyboard::Key::Process,
        Key::Named(NamedKey::SingleCandidate) => hearth_input::keyboard::Key::SingleCandidate,
        Key::Named(NamedKey::HangulMode) => hearth_input::keyboard::Key::HangulMode,
        Key::Named(NamedKey::HanjaMode) => hearth_input::keyboard::Key::HanjaMode,
        Key::Named(NamedKey::JunjaMode) => hearth_input::keyboard::Key::JunjaMode,
        Key::Named(NamedKey::Eisu) => hearth_input::keyboard::Key::Eisu,
        Key::Named(NamedKey::Hankaku) => hearth_input::keyboard::Key::Hankaku,
        Key::Named(NamedKey::Hiragana) => hearth_input::keyboard::Key::Hiragana,
        Key::Named(NamedKey::HiraganaKatakana) => hearth_input::keyboard::Key::HiraganaKatakana,
        Key::Named(NamedKey::KanaMode) => hearth_input::keyboard::Key::KanaMode,
        Key::Named(NamedKey::KanjiMode) => hearth_input::keyboard::Key::KanjiMode,
        Key::Named(NamedKey::Katakana) => hearth_input::keyboard::Key::Katakana,
        Key::Named(NamedKey::Romaji) => hearth_input::keyboard::Key::Romaji,
        Key::Named(NamedKey::Zenkaku) => hearth_input::keyboard::Key::Zenkaku,
        Key::Named(NamedKey::ZenkakuHankaku) => hearth_input::keyboard::Key::ZenkakuHankaku,
        Key::Named(NamedKey::Soft1) => hearth_input::keyboard::Key::Soft1,
        Key::Named(NamedKey::Soft2) => hearth_input::keyboard::Key::Soft2,
        Key::Named(NamedKey::Soft3) => hearth_input::keyboard::Key::Soft3,
        Key::Named(NamedKey::Soft4) => hearth_input::keyboard::Key::Soft4,
        Key::Named(NamedKey::ChannelDown) => hearth_input::keyboard::Key::ChannelDown,
        Key::Named(NamedKey::ChannelUp) => hearth_input::keyboard::Key::ChannelUp,
        Key::Named(NamedKey::Close) => hearth_input::keyboard::Key::Close,
        Key::Named(NamedKey::MailForward) => hearth_input::keyboard::Key::MailForward,
        Key::Named(NamedKey::MailReply) => hearth_input::keyboard::Key::MailReply,
        Key::Named(NamedKey::MailSend) => hearth_input::keyboard::Key::MailSend,
        Key::Named(NamedKey::MediaClose) => hearth_input::keyboard::Key::MediaClose,
        Key::Named(NamedKey::MediaFastForward) => hearth_input::keyboard::Key::MediaFastForward,
        Key::Named(NamedKey::MediaPause) => hearth_input::keyboard::Key::MediaPause,
        Key::Named(NamedKey::MediaPlay) => hearth_input::keyboard::Key::MediaPlay,
        Key::Named(NamedKey::MediaPlayPause) => hearth_input::keyboard::Key::MediaPlayPause,
        Key::Named(NamedKey::MediaRecord) => hearth_input::keyboard::Key::MediaRecord,
        Key::Named(NamedKey::MediaRewind) => hearth_input::keyboard::Key::MediaRewind,
        Key::Named(NamedKey::MediaStop) => hearth_input::keyboard::Key::MediaStop,
        Key::Named(NamedKey::MediaTrackNext) => hearth_input::keyboard::Key::MediaTrackNext,
        Key::Named(NamedKey::MediaTrackPrevious) => hearth_input::keyboard::Key::MediaTrackPrevious,
        Key::Named(NamedKey::New) => hearth_input::keyboard::Key::New,
        Key::Named(NamedKey::Open) => hearth_input::keyboard::Key::Open,
        Key::Named(NamedKey::Print) => hearth_input::keyboard::Key::Print,
        Key::Named(NamedKey::Save) => hearth_input::keyboard::Key::Save,
        Key::Named(NamedKey::SpellCheck) => hearth_input::keyboard::Key::SpellCheck,
        Key::Named(NamedKey::Key11) => hearth_input::keyboard::Key::Key11,
        Key::Named(NamedKey::Key12) => hearth_input::keyboard::Key::Key12,
        Key::Named(NamedKey::AudioBalanceLeft) => hearth_input::keyboard::Key::AudioBalanceLeft,
        Key::Named(NamedKey::AudioBalanceRight) => hearth_input::keyboard::Key::AudioBalanceRight,
        Key::Named(NamedKey::AudioBassBoostDown) => hearth_input::keyboard::Key::AudioBassBoostDown,
        Key::Named(NamedKey::AudioBassBoostToggle) => {
            hearth_input::keyboard::Key::AudioBassBoostToggle
        }
        Key::Named(NamedKey::AudioBassBoostUp) => hearth_input::keyboard::Key::AudioBassBoostUp,
        Key::Named(NamedKey::AudioFaderFront) => hearth_input::keyboard::Key::AudioFaderFront,
        Key::Named(NamedKey::AudioFaderRear) => hearth_input::keyboard::Key::AudioFaderRear,
        Key::Named(NamedKey::AudioSurroundModeNext) => {
            hearth_input::keyboard::Key::AudioSurroundModeNext
        }
        Key::Named(NamedKey::AudioTrebleDown) => hearth_input::keyboard::Key::AudioTrebleDown,
        Key::Named(NamedKey::AudioTrebleUp) => hearth_input::keyboard::Key::AudioTrebleUp,
        Key::Named(NamedKey::AudioVolumeDown) => hearth_input::keyboard::Key::AudioVolumeDown,
        Key::Named(NamedKey::AudioVolumeUp) => hearth_input::keyboard::Key::AudioVolumeUp,
        Key::Named(NamedKey::AudioVolumeMute) => hearth_input::keyboard::Key::AudioVolumeMute,
        Key::Named(NamedKey::MicrophoneToggle) => hearth_input::keyboard::Key::MicrophoneToggle,
        Key::Named(NamedKey::MicrophoneVolumeDown) => {
            hearth_input::keyboard::Key::MicrophoneVolumeDown
        }
        Key::Named(NamedKey::MicrophoneVolumeUp) => hearth_input::keyboard::Key::MicrophoneVolumeUp,
        Key::Named(NamedKey::MicrophoneVolumeMute) => {
            hearth_input::keyboard::Key::MicrophoneVolumeMute
        }
        Key::Named(NamedKey::SpeechCorrectionList) => {
            hearth_input::keyboard::Key::SpeechCorrectionList
        }
        Key::Named(NamedKey::SpeechInputToggle) => hearth_input::keyboard::Key::SpeechInputToggle,
        Key::Named(NamedKey::LaunchApplication1) => hearth_input::keyboard::Key::LaunchApplication1,
        Key::Named(NamedKey::LaunchApplication2) => hearth_input::keyboard::Key::LaunchApplication2,
        Key::Named(NamedKey::LaunchCalendar) => hearth_input::keyboard::Key::LaunchCalendar,
        Key::Named(NamedKey::LaunchContacts) => hearth_input::keyboard::Key::LaunchContacts,
        Key::Named(NamedKey::LaunchMail) => hearth_input::keyboard::Key::LaunchMail,
        Key::Named(NamedKey::LaunchMediaPlayer) => hearth_input::keyboard::Key::LaunchMediaPlayer,
        Key::Named(NamedKey::LaunchMusicPlayer) => hearth_input::keyboard::Key::LaunchMusicPlayer,
        Key::Named(NamedKey::LaunchPhone) => hearth_input::keyboard::Key::LaunchPhone,
        Key::Named(NamedKey::LaunchScreenSaver) => hearth_input::keyboard::Key::LaunchScreenSaver,
        Key::Named(NamedKey::LaunchSpreadsheet) => hearth_input::keyboard::Key::LaunchSpreadsheet,
        Key::Named(NamedKey::LaunchWebBrowser) => hearth_input::keyboard::Key::LaunchWebBrowser,
        Key::Named(NamedKey::LaunchWebCam) => hearth_input::keyboard::Key::LaunchWebCam,
        Key::Named(NamedKey::LaunchWordProcessor) => {
            hearth_input::keyboard::Key::LaunchWordProcessor
        }
        Key::Named(NamedKey::BrowserBack) => hearth_input::keyboard::Key::BrowserBack,
        Key::Named(NamedKey::BrowserFavorites) => hearth_input::keyboard::Key::BrowserFavorites,
        Key::Named(NamedKey::BrowserForward) => hearth_input::keyboard::Key::BrowserForward,
        Key::Named(NamedKey::BrowserHome) => hearth_input::keyboard::Key::BrowserHome,
        Key::Named(NamedKey::BrowserRefresh) => hearth_input::keyboard::Key::BrowserRefresh,
        Key::Named(NamedKey::BrowserSearch) => hearth_input::keyboard::Key::BrowserSearch,
        Key::Named(NamedKey::BrowserStop) => hearth_input::keyboard::Key::BrowserStop,
        Key::Named(NamedKey::AppSwitch) => hearth_input::keyboard::Key::AppSwitch,
        Key::Named(NamedKey::Call) => hearth_input::keyboard::Key::Call,
        Key::Named(NamedKey::Camera) => hearth_input::keyboard::Key::Camera,
        Key::Named(NamedKey::CameraFocus) => hearth_input::keyboard::Key::CameraFocus,
        Key::Named(NamedKey::EndCall) => hearth_input::keyboard::Key::EndCall,
        Key::Named(NamedKey::GoBack) => hearth_input::keyboard::Key::GoBack,
        Key::Named(NamedKey::GoHome) => hearth_input::keyboard::Key::GoHome,
        Key::Named(NamedKey::HeadsetHook) => hearth_input::keyboard::Key::HeadsetHook,
        Key::Named(NamedKey::LastNumberRedial) => hearth_input::keyboard::Key::LastNumberRedial,
        Key::Named(NamedKey::Notification) => hearth_input::keyboard::Key::Notification,
        Key::Named(NamedKey::MannerMode) => hearth_input::keyboard::Key::MannerMode,
        Key::Named(NamedKey::VoiceDial) => hearth_input::keyboard::Key::VoiceDial,
        Key::Named(NamedKey::TV) => hearth_input::keyboard::Key::TV,
        Key::Named(NamedKey::TV3DMode) => hearth_input::keyboard::Key::TV3DMode,
        Key::Named(NamedKey::TVAntennaCable) => hearth_input::keyboard::Key::TVAntennaCable,
        Key::Named(NamedKey::TVAudioDescription) => hearth_input::keyboard::Key::TVAudioDescription,
        Key::Named(NamedKey::TVAudioDescriptionMixDown) => {
            hearth_input::keyboard::Key::TVAudioDescriptionMixDown
        }
        Key::Named(NamedKey::TVAudioDescriptionMixUp) => {
            hearth_input::keyboard::Key::TVAudioDescriptionMixUp
        }
        Key::Named(NamedKey::TVContentsMenu) => hearth_input::keyboard::Key::TVContentsMenu,
        Key::Named(NamedKey::TVDataService) => hearth_input::keyboard::Key::TVDataService,
        Key::Named(NamedKey::TVInput) => hearth_input::keyboard::Key::TVInput,
        Key::Named(NamedKey::TVInputComponent1) => hearth_input::keyboard::Key::TVInputComponent1,
        Key::Named(NamedKey::TVInputComponent2) => hearth_input::keyboard::Key::TVInputComponent2,
        Key::Named(NamedKey::TVInputComposite1) => hearth_input::keyboard::Key::TVInputComposite1,
        Key::Named(NamedKey::TVInputComposite2) => hearth_input::keyboard::Key::TVInputComposite2,
        Key::Named(NamedKey::TVInputHDMI1) => hearth_input::keyboard::Key::TVInputHDMI1,
        Key::Named(NamedKey::TVInputHDMI2) => hearth_input::keyboard::Key::TVInputHDMI2,
        Key::Named(NamedKey::TVInputHDMI3) => hearth_input::keyboard::Key::TVInputHDMI3,
        Key::Named(NamedKey::TVInputHDMI4) => hearth_input::keyboard::Key::TVInputHDMI4,
        Key::Named(NamedKey::TVInputVGA1) => hearth_input::keyboard::Key::TVInputVGA1,
        Key::Named(NamedKey::TVMediaContext) => hearth_input::keyboard::Key::TVMediaContext,
        Key::Named(NamedKey::TVNetwork) => hearth_input::keyboard::Key::TVNetwork,
        Key::Named(NamedKey::TVNumberEntry) => hearth_input::keyboard::Key::TVNumberEntry,
        Key::Named(NamedKey::TVPower) => hearth_input::keyboard::Key::TVPower,
        Key::Named(NamedKey::TVRadioService) => hearth_input::keyboard::Key::TVRadioService,
        Key::Named(NamedKey::TVSatellite) => hearth_input::keyboard::Key::TVSatellite,
        Key::Named(NamedKey::TVSatelliteBS) => hearth_input::keyboard::Key::TVSatelliteBS,
        Key::Named(NamedKey::TVSatelliteCS) => hearth_input::keyboard::Key::TVSatelliteCS,
        Key::Named(NamedKey::TVSatelliteToggle) => hearth_input::keyboard::Key::TVSatelliteToggle,
        Key::Named(NamedKey::TVTerrestrialAnalog) => {
            hearth_input::keyboard::Key::TVTerrestrialAnalog
        }
        Key::Named(NamedKey::TVTerrestrialDigital) => {
            hearth_input::keyboard::Key::TVTerrestrialDigital
        }
        Key::Named(NamedKey::TVTimer) => hearth_input::keyboard::Key::TVTimer,
        Key::Named(NamedKey::AVRInput) => hearth_input::keyboard::Key::AVRInput,
        Key::Named(NamedKey::AVRPower) => hearth_input::keyboard::Key::AVRPower,
        Key::Named(NamedKey::ColorF0Red) => hearth_input::keyboard::Key::ColorF0Red,
        Key::Named(NamedKey::ColorF1Green) => hearth_input::keyboard::Key::ColorF1Green,
        Key::Named(NamedKey::ColorF2Yellow) => hearth_input::keyboard::Key::ColorF2Yellow,
        Key::Named(NamedKey::ColorF3Blue) => hearth_input::keyboard::Key::ColorF3Blue,
        Key::Named(NamedKey::ColorF4Grey) => hearth_input::keyboard::Key::ColorF4Grey,
        Key::Named(NamedKey::ColorF5Brown) => hearth_input::keyboard::Key::ColorF5Brown,
        Key::Named(NamedKey::ClosedCaptionToggle) => {
            hearth_input::keyboard::Key::ClosedCaptionToggle
        }
        Key::Named(NamedKey::Dimmer) => hearth_input::keyboard::Key::Dimmer,
        Key::Named(NamedKey::DisplaySwap) => hearth_input::keyboard::Key::DisplaySwap,
        Key::Named(NamedKey::DVR) => hearth_input::keyboard::Key::DVR,
        Key::Named(NamedKey::Exit) => hearth_input::keyboard::Key::Exit,
        Key::Named(NamedKey::FavoriteClear0) => hearth_input::keyboard::Key::FavoriteClear0,
        Key::Named(NamedKey::FavoriteClear1) => hearth_input::keyboard::Key::FavoriteClear1,
        Key::Named(NamedKey::FavoriteClear2) => hearth_input::keyboard::Key::FavoriteClear2,
        Key::Named(NamedKey::FavoriteClear3) => hearth_input::keyboard::Key::FavoriteClear3,
        Key::Named(NamedKey::FavoriteRecall0) => hearth_input::keyboard::Key::FavoriteRecall0,
        Key::Named(NamedKey::FavoriteRecall1) => hearth_input::keyboard::Key::FavoriteRecall1,
        Key::Named(NamedKey::FavoriteRecall2) => hearth_input::keyboard::Key::FavoriteRecall2,
        Key::Named(NamedKey::FavoriteRecall3) => hearth_input::keyboard::Key::FavoriteRecall3,
        Key::Named(NamedKey::FavoriteStore0) => hearth_input::keyboard::Key::FavoriteStore0,
        Key::Named(NamedKey::FavoriteStore1) => hearth_input::keyboard::Key::FavoriteStore1,
        Key::Named(NamedKey::FavoriteStore2) => hearth_input::keyboard::Key::FavoriteStore2,
        Key::Named(NamedKey::FavoriteStore3) => hearth_input::keyboard::Key::FavoriteStore3,
        Key::Named(NamedKey::Guide) => hearth_input::keyboard::Key::Guide,
        Key::Named(NamedKey::GuideNextDay) => hearth_input::keyboard::Key::GuideNextDay,
        Key::Named(NamedKey::GuidePreviousDay) => hearth_input::keyboard::Key::GuidePreviousDay,
        Key::Named(NamedKey::Info) => hearth_input::keyboard::Key::Info,
        Key::Named(NamedKey::InstantReplay) => hearth_input::keyboard::Key::InstantReplay,
        Key::Named(NamedKey::Link) => hearth_input::keyboard::Key::Link,
        Key::Named(NamedKey::ListProgram) => hearth_input::keyboard::Key::ListProgram,
        Key::Named(NamedKey::LiveContent) => hearth_input::keyboard::Key::LiveContent,
        Key::Named(NamedKey::Lock) => hearth_input::keyboard::Key::Lock,
        Key::Named(NamedKey::MediaApps) => hearth_input::keyboard::Key::MediaApps,
        Key::Named(NamedKey::MediaAudioTrack) => hearth_input::keyboard::Key::MediaAudioTrack,
        Key::Named(NamedKey::MediaLast) => hearth_input::keyboard::Key::MediaLast,
        Key::Named(NamedKey::MediaSkipBackward) => hearth_input::keyboard::Key::MediaSkipBackward,
        Key::Named(NamedKey::MediaSkipForward) => hearth_input::keyboard::Key::MediaSkipForward,
        Key::Named(NamedKey::MediaStepBackward) => hearth_input::keyboard::Key::MediaStepBackward,
        Key::Named(NamedKey::MediaStepForward) => hearth_input::keyboard::Key::MediaStepForward,
        Key::Named(NamedKey::MediaTopMenu) => hearth_input::keyboard::Key::MediaTopMenu,
        Key::Named(NamedKey::NavigateIn) => hearth_input::keyboard::Key::NavigateIn,
        Key::Named(NamedKey::NavigateNext) => hearth_input::keyboard::Key::NavigateNext,
        Key::Named(NamedKey::NavigateOut) => hearth_input::keyboard::Key::NavigateOut,
        Key::Named(NamedKey::NavigatePrevious) => hearth_input::keyboard::Key::NavigatePrevious,
        Key::Named(NamedKey::NextFavoriteChannel) => {
            hearth_input::keyboard::Key::NextFavoriteChannel
        }
        Key::Named(NamedKey::NextUserProfile) => hearth_input::keyboard::Key::NextUserProfile,
        Key::Named(NamedKey::OnDemand) => hearth_input::keyboard::Key::OnDemand,
        Key::Named(NamedKey::Pairing) => hearth_input::keyboard::Key::Pairing,
        Key::Named(NamedKey::PinPDown) => hearth_input::keyboard::Key::PinPDown,
        Key::Named(NamedKey::PinPMove) => hearth_input::keyboard::Key::PinPMove,
        Key::Named(NamedKey::PinPToggle) => hearth_input::keyboard::Key::PinPToggle,
        Key::Named(NamedKey::PinPUp) => hearth_input::keyboard::Key::PinPUp,
        Key::Named(NamedKey::PlaySpeedDown) => hearth_input::keyboard::Key::PlaySpeedDown,
        Key::Named(NamedKey::PlaySpeedReset) => hearth_input::keyboard::Key::PlaySpeedReset,
        Key::Named(NamedKey::PlaySpeedUp) => hearth_input::keyboard::Key::PlaySpeedUp,
        Key::Named(NamedKey::RandomToggle) => hearth_input::keyboard::Key::RandomToggle,
        Key::Named(NamedKey::RcLowBattery) => hearth_input::keyboard::Key::RcLowBattery,
        Key::Named(NamedKey::RecordSpeedNext) => hearth_input::keyboard::Key::RecordSpeedNext,
        Key::Named(NamedKey::RfBypass) => hearth_input::keyboard::Key::RfBypass,
        Key::Named(NamedKey::ScanChannelsToggle) => hearth_input::keyboard::Key::ScanChannelsToggle,
        Key::Named(NamedKey::ScreenModeNext) => hearth_input::keyboard::Key::ScreenModeNext,
        Key::Named(NamedKey::Settings) => hearth_input::keyboard::Key::Settings,
        Key::Named(NamedKey::SplitScreenToggle) => hearth_input::keyboard::Key::SplitScreenToggle,
        Key::Named(NamedKey::STBInput) => hearth_input::keyboard::Key::STBInput,
        Key::Named(NamedKey::STBPower) => hearth_input::keyboard::Key::STBPower,
        Key::Named(NamedKey::Subtitle) => hearth_input::keyboard::Key::Subtitle,
        Key::Named(NamedKey::Teletext) => hearth_input::keyboard::Key::Teletext,
        Key::Named(NamedKey::VideoModeNext) => hearth_input::keyboard::Key::VideoModeNext,
        Key::Named(NamedKey::Wink) => hearth_input::keyboard::Key::Wink,
        Key::Named(NamedKey::ZoomToggle) => hearth_input::keyboard::Key::ZoomToggle,
        Key::Named(NamedKey::F1) => hearth_input::keyboard::Key::F1,
        Key::Named(NamedKey::F2) => hearth_input::keyboard::Key::F2,
        Key::Named(NamedKey::F3) => hearth_input::keyboard::Key::F3,
        Key::Named(NamedKey::F4) => hearth_input::keyboard::Key::F4,
        Key::Named(NamedKey::F5) => hearth_input::keyboard::Key::F5,
        Key::Named(NamedKey::F6) => hearth_input::keyboard::Key::F6,
        Key::Named(NamedKey::F7) => hearth_input::keyboard::Key::F7,
        Key::Named(NamedKey::F8) => hearth_input::keyboard::Key::F8,
        Key::Named(NamedKey::F9) => hearth_input::keyboard::Key::F9,
        Key::Named(NamedKey::F10) => hearth_input::keyboard::Key::F10,
        Key::Named(NamedKey::F11) => hearth_input::keyboard::Key::F11,
        Key::Named(NamedKey::F12) => hearth_input::keyboard::Key::F12,
        Key::Named(NamedKey::F13) => hearth_input::keyboard::Key::F13,
        Key::Named(NamedKey::F14) => hearth_input::keyboard::Key::F14,
        Key::Named(NamedKey::F15) => hearth_input::keyboard::Key::F15,
        Key::Named(NamedKey::F16) => hearth_input::keyboard::Key::F16,
        Key::Named(NamedKey::F17) => hearth_input::keyboard::Key::F17,
        Key::Named(NamedKey::F18) => hearth_input::keyboard::Key::F18,
        Key::Named(NamedKey::F19) => hearth_input::keyboard::Key::F19,
        Key::Named(NamedKey::F20) => hearth_input::keyboard::Key::F20,
        Key::Named(NamedKey::F21) => hearth_input::keyboard::Key::F21,
        Key::Named(NamedKey::F22) => hearth_input::keyboard::Key::F22,
        Key::Named(NamedKey::F23) => hearth_input::keyboard::Key::F23,
        Key::Named(NamedKey::F24) => hearth_input::keyboard::Key::F24,
        Key::Named(NamedKey::F25) => hearth_input::keyboard::Key::F25,
        Key::Named(NamedKey::F26) => hearth_input::keyboard::Key::F26,
        Key::Named(NamedKey::F27) => hearth_input::keyboard::Key::F27,
        Key::Named(NamedKey::F28) => hearth_input::keyboard::Key::F28,
        Key::Named(NamedKey::F29) => hearth_input::keyboard::Key::F29,
        Key::Named(NamedKey::F30) => hearth_input::keyboard::Key::F30,
        Key::Named(NamedKey::F31) => hearth_input::keyboard::Key::F31,
        Key::Named(NamedKey::F32) => hearth_input::keyboard::Key::F32,
        Key::Named(NamedKey::F33) => hearth_input::keyboard::Key::F33,
        Key::Named(NamedKey::F34) => hearth_input::keyboard::Key::F34,
        Key::Named(NamedKey::F35) => hearth_input::keyboard::Key::F35,
        _ => todo!(),
    }
}

///Converts a [`winit::keyboard::NativeKey`] to a Hearth [`NativeKey`](hearth_input::keyboard::NativeKey)
pub fn convert_native_key(native_key: &NativeKey) -> hearth_input::keyboard::NativeKey {
    match native_key {
        NativeKey::Unidentified => hearth_input::keyboard::NativeKey::Unidentified,
        NativeKey::Android(v) => hearth_input::keyboard::NativeKey::Android(*v),
        NativeKey::MacOS(v) => hearth_input::keyboard::NativeKey::MacOS(*v),
        NativeKey::Windows(v) => hearth_input::keyboard::NativeKey::Windows(*v),
        NativeKey::Xkb(v) => hearth_input::keyboard::NativeKey::Xkb(*v),
        NativeKey::Web(v) => hearth_input::keyboard::NativeKey::Web(v.clone()),
    }
}

/// Converts a Hearth [`SystemCursorIcon`] to a [`winit::window::CursorIcon`].
pub fn convert_system_cursor_icon(cursor_icon: SystemCursorIcon) -> winit::window::CursorIcon {
    match cursor_icon {
        SystemCursorIcon::Crosshair => winit::window::CursorIcon::Crosshair,
        SystemCursorIcon::Pointer => winit::window::CursorIcon::Pointer,
        SystemCursorIcon::Move => winit::window::CursorIcon::Move,
        SystemCursorIcon::Text => winit::window::CursorIcon::Text,
        SystemCursorIcon::Wait => winit::window::CursorIcon::Wait,
        SystemCursorIcon::Help => winit::window::CursorIcon::Help,
        SystemCursorIcon::Progress => winit::window::CursorIcon::Progress,
        SystemCursorIcon::NotAllowed => winit::window::CursorIcon::NotAllowed,
        SystemCursorIcon::ContextMenu => winit::window::CursorIcon::ContextMenu,
        SystemCursorIcon::Cell => winit::window::CursorIcon::Cell,
        SystemCursorIcon::VerticalText => winit::window::CursorIcon::VerticalText,
        SystemCursorIcon::Alias => winit::window::CursorIcon::Alias,
        SystemCursorIcon::Copy => winit::window::CursorIcon::Copy,
        SystemCursorIcon::NoDrop => winit::window::CursorIcon::NoDrop,
        SystemCursorIcon::Grab => winit::window::CursorIcon::Grab,
        SystemCursorIcon::Grabbing => winit::window::CursorIcon::Grabbing,
        SystemCursorIcon::AllScroll => winit::window::CursorIcon::AllScroll,
        SystemCursorIcon::ZoomIn => winit::window::CursorIcon::ZoomIn,
        SystemCursorIcon::ZoomOut => winit::window::CursorIcon::ZoomOut,
        SystemCursorIcon::EResize => winit::window::CursorIcon::EResize,
        SystemCursorIcon::NResize => winit::window::CursorIcon::NResize,
        SystemCursorIcon::NeResize => winit::window::CursorIcon::NeResize,
        SystemCursorIcon::NwResize => winit::window::CursorIcon::NwResize,
        SystemCursorIcon::SResize => winit::window::CursorIcon::SResize,
        SystemCursorIcon::SeResize => winit::window::CursorIcon::SeResize,
        SystemCursorIcon::SwResize => winit::window::CursorIcon::SwResize,
        SystemCursorIcon::WResize => winit::window::CursorIcon::WResize,
        SystemCursorIcon::EwResize => winit::window::CursorIcon::EwResize,
        SystemCursorIcon::NsResize => winit::window::CursorIcon::NsResize,
        SystemCursorIcon::NeswResize => winit::window::CursorIcon::NeswResize,
        SystemCursorIcon::NwseResize => winit::window::CursorIcon::NwseResize,
        SystemCursorIcon::ColResize => winit::window::CursorIcon::ColResize,
        SystemCursorIcon::RowResize => winit::window::CursorIcon::RowResize,
        _ => winit::window::CursorIcon::Default,
    }
}

/// Converts a Hearth [`WindowLevel`] to a [`winit::window::WindowLevel`]
pub fn convert_window_level(window_level: WindowLevel) -> winit::window::WindowLevel {
    match window_level {
        WindowLevel::AlwaysOnBottom => winit::window::WindowLevel::AlwaysOnBottom,
        WindowLevel::Normal => winit::window::WindowLevel::Normal,
        WindowLevel::AlwaysOnTop => winit::window::WindowLevel::AlwaysOnTop,
    }
}

/// Converts a [`winit::window::Theme`] to a Hearth [`WindowTheme`]
pub fn convert_winit_theme(theme: winit::window::Theme) -> WindowTheme {
    match theme {
        winit::window::Theme::Light => WindowTheme::Light,
        winit::window::Theme::Dark => WindowTheme::Dark,
    }
}

/// Converts a Hearth [`WindowTheme`] to a [`winit::window::Theme`]
pub fn convert_window_theme(theme: WindowTheme) -> winit::window::Theme {
    match theme {
        WindowTheme::Light => winit::window::Theme::Light,
        WindowTheme::Dark => winit::window::Theme::Dark,
    }
}

/// Converts a Hearth [`EnabledButtons`] to a [`winit::window::WindowButtons`]
pub fn convert_enabled_buttons(enabled_buttons: EnabledButtons) -> winit::window::WindowButtons {
    let mut window_buttons = winit::window::WindowButtons::empty();
    if enabled_buttons.minimize {
        window_buttons.insert(winit::window::WindowButtons::MINIMIZE);
    }
    if enabled_buttons.maximize {
        window_buttons.insert(winit::window::WindowButtons::MAXIMIZE);
    }
    if enabled_buttons.close {
        window_buttons.insert(winit::window::WindowButtons::CLOSE);
    }
    window_buttons
}

/// Converts a Hearth [`CompassOctant`] to a [`winit::window::ResizeDirection`]
pub fn convert_resize_direction(resize_direction: CompassOctant) -> winit::window::ResizeDirection {
    match resize_direction {
        CompassOctant::West => winit::window::ResizeDirection::West,
        CompassOctant::North => winit::window::ResizeDirection::North,
        CompassOctant::East => winit::window::ResizeDirection::East,
        CompassOctant::South => winit::window::ResizeDirection::South,
        CompassOctant::NorthWest => winit::window::ResizeDirection::NorthWest,
        CompassOctant::NorthEast => winit::window::ResizeDirection::NorthEast,
        CompassOctant::SouthWest => winit::window::ResizeDirection::SouthWest,
        CompassOctant::SouthEast => winit::window::ResizeDirection::SouthEast,
    }
}

#[cfg(target_os = "ios")]
/// Converts a [`hearth_window::ScreenEdge`] to a [`winit::platform::ios::ScreenEdge`].
pub(crate) fn convert_screen_edge(edge: ScreenEdge) -> winit::platform::ios::ScreenEdge {
    match edge {
        ScreenEdge::None => winit::platform::ios::ScreenEdge::NONE,
        ScreenEdge::Top => winit::platform::ios::ScreenEdge::TOP,
        ScreenEdge::Bottom => winit::platform::ios::ScreenEdge::BOTTOM,
        ScreenEdge::Left => winit::platform::ios::ScreenEdge::LEFT,
        ScreenEdge::Right => winit::platform::ios::ScreenEdge::RIGHT,
        ScreenEdge::All => winit::platform::ios::ScreenEdge::ALL,
    }
}
