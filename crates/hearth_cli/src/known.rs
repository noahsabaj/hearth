//! Type paths of the engine types the CLI uses directly.

/// `Name`.
pub const NAME: &str = "hearth_ecs::name::Name";
/// `ChildOf`, the parent relationship.
pub const CHILD_OF: &str = "hearth_ecs::hierarchy::ChildOf";
/// `Children`, the ordered list of children.
pub const CHILDREN: &str = "hearth_ecs::hierarchy::Children";
/// `IsResource`, the marker of the entities that hold resources.
pub const IS_RESOURCE: &str = "hearth_ecs::resource::IsResource";
/// `Window`.
pub const WINDOW: &str = "hearth_window::window::Window";
/// `PrimaryWindow`, the marker of the main window.
pub const PRIMARY_WINDOW: &str = "hearth_window::window::PrimaryWindow";
/// `Time<Virtual>`, the clock gameplay runs on.
pub const TIME_VIRTUAL: &str = "hearth_time::time::Time<hearth_time::virt::Virtual>";
/// `AppExit`, the message that ends the app.
pub const APP_EXIT: &str = "hearth_app::app::AppExit";
/// `Screenshot`, the component that requests a capture.
pub const SCREENSHOT: &str = "hearth_render::view::window::screenshot::Screenshot";
/// `ScreenshotCaptured`, the event that delivers a capture.
pub const SCREENSHOT_CAPTURED: &str = "hearth_render::view::window::screenshot::ScreenshotCaptured";
/// `WindowEvent`, which carries every window input event.
pub const WINDOW_EVENT: &str = "hearth_window::event::WindowEvent";
/// `KeyboardInput`.
pub const KEYBOARD_INPUT: &str = "hearth_input::keyboard::KeyboardInput";
/// `KeyCode`.
pub const KEY_CODE: &str = "hearth_input::keyboard::KeyCode";
/// `MouseButtonInput`.
pub const MOUSE_BUTTON_INPUT: &str = "hearth_input::mouse::MouseButtonInput";
/// `MouseWheel`.
pub const MOUSE_WHEEL: &str = "hearth_input::mouse::MouseWheel";
/// `CursorMoved`.
pub const CURSOR_MOVED: &str = "hearth_window::event::CursorMoved";

/// Components that say what an unnamed entity is, most telling first.
pub const LABELS: &[&str] = &[
    "Window",
    "Camera3d",
    "Camera2d",
    "Camera",
    "DirectionalLight",
    "PointLight",
    "SpotLight",
    "RectLight",
    "LightProbe",
    "Button",
    "Text",
    "Text2d",
    "TextSpan",
    "ImageNode",
    "Node",
    "Sprite",
    "Mesh3d",
    "Mesh2d",
    "SceneRoot",
    "AudioPlayer",
    "Gamepad",
    "Monitor",
    "PointerId",
];

/// Components too common to say what an entity is.
pub const GENERIC: &[&str] = &[
    "Transform",
    "GlobalTransform",
    "TransformTreeChanged",
    "Visibility",
    "InheritedVisibility",
    "ViewVisibility",
    "ChildOf",
    "Children",
    "Name",
    "VisibilityClass",
    "SyncToRenderWorld",
];
