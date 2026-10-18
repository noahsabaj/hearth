//! Gestures functionality, from touchscreens and touchpads.

use hearth_ecs::message::Message;
#[cfg(feature = "hearth_reflect")]
use hearth_ecs::prelude::ReflectMessage;
use hearth_math::Vec2;
#[cfg(feature = "hearth_reflect")]
use hearth_reflect::Reflect;

#[cfg(all(feature = "serialize", feature = "hearth_reflect"))]
use hearth_reflect::{ReflectDeserialize, ReflectSerialize};

/// Two-finger pinch gesture, often used for magnifications.
///
/// Positive delta values indicate magnification (zooming in) and
/// negative delta values indicate shrinking (zooming out).
///
/// ## Platform-specific
///
/// - Only available on **`macOS`** and **`iOS`**.
/// - On **`iOS`**, must be enabled first
#[derive(Message, Debug, Clone, Copy, PartialEq)]
#[cfg_attr(
    feature = "hearth_reflect",
    derive(Reflect),
    reflect(Debug, PartialEq, Clone, Message)
)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    all(feature = "serialize", feature = "hearth_reflect"),
    reflect(Serialize, Deserialize)
)]
pub struct PinchGesture(pub f32);

/// Two-finger rotation gesture.
///
/// Positive delta values indicate rotation counterclockwise and
/// negative delta values indicate rotation clockwise.
///
/// ## Platform-specific
///
/// - Only available on **`macOS`** and **`iOS`**.
/// - On **`iOS`**, must be enabled first
#[derive(Message, Debug, Clone, Copy, PartialEq)]
#[cfg_attr(
    feature = "hearth_reflect",
    derive(Reflect),
    reflect(Debug, PartialEq, Clone, Message)
)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    all(feature = "serialize", feature = "hearth_reflect"),
    reflect(Serialize, Deserialize)
)]
pub struct RotationGesture(pub f32);

/// Double tap gesture.
///
/// ## Platform-specific
///
/// - Only available on **`macOS`** and **`iOS`**.
/// - On **`iOS`**, must be enabled first
#[derive(Message, Debug, Clone, Copy, PartialEq)]
#[cfg_attr(
    feature = "hearth_reflect",
    derive(Reflect),
    reflect(Debug, PartialEq, Clone, Message)
)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    all(feature = "serialize", feature = "hearth_reflect"),
    reflect(Serialize, Deserialize)
)]
pub struct DoubleTapGesture;

/// Pan gesture.
///
/// ## Platform-specific
///
/// - On **`iOS`**, must be enabled first
#[derive(Message, Debug, Clone, Copy, PartialEq)]
#[cfg_attr(
    feature = "hearth_reflect",
    derive(Reflect),
    reflect(Debug, PartialEq, Clone, Message)
)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    all(feature = "serialize", feature = "hearth_reflect"),
    reflect(Serialize, Deserialize)
)]
pub struct PanGesture(pub Vec2);
