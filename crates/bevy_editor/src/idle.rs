//! Keeps the editor from redrawing while nothing happens, so that an open editor leaves the CPU to
//! the builds and the game it is waiting on.
//!
//! The editor updates when input or a window event arrives, and otherwise a few times a second,
//! which is often enough for the play-mode poll and assets that finish loading in the background.
//! While a key or mouse button is held, such as when flying the camera or dragging the gizmo, for
//! a moment after any input, and while a screenshot is being taken, it updates every frame.

use core::time::Duration;

use bevy_app::{App, Last};
use bevy_ecs::prelude::*;
use bevy_input::{
    keyboard::KeyCode,
    mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseButton},
    ButtonInput,
};
use bevy_math::Vec2;
use bevy_render::view::screenshot::Screenshot;
use bevy_time::{Real, Time};
use bevy_window::{CursorMoved, RequestRedraw};
use bevy_winit::{UpdateMode, WinitSettings};

/// How long the editor waits for input before it updates anyway, while it has focus.
const FOCUSED_WAIT: Duration = Duration::from_millis(100);

/// How long the editor waits before it updates anyway, while another window has focus, as the
/// game's does in play mode.
const UNFOCUSED_WAIT: Duration = Duration::from_millis(250);

/// How long the editor keeps updating every frame after the last input, so that what the input
/// started, such as a hover or a panel that takes a few frames to lay out, finishes promptly.
const LINGER: Duration = Duration::from_millis(250);

pub(crate) fn plugin(app: &mut App) {
    app.insert_resource(WinitSettings {
        focused_mode: UpdateMode::reactive(FOCUSED_WAIT),
        unfocused_mode: UpdateMode::reactive_low_power(UNFOCUSED_WAIT),
    })
    .add_systems(Last, stay_awake_after_input);
}

fn stay_awake_after_input(
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    mut cursor: MessageReader<CursorMoved>,
    screenshots: Query<(), With<Screenshot>>,
    time: Res<Time<Real>>,
    mut awake_until: Local<Duration>,
    mut redraw: MessageWriter<RequestRedraw>,
) {
    let cursor_moved = cursor.read().count() > 0;
    let input = cursor_moved
        || keys.get_pressed().next().is_some()
        || keys.get_just_released().next().is_some()
        || buttons.get_pressed().next().is_some()
        || buttons.get_just_released().next().is_some()
        || motion.delta != Vec2::ZERO
        || scroll.delta != Vec2::ZERO;
    let now = time.elapsed();
    if input {
        *awake_until = now + LINGER;
    }
    // A screenshot takes a few frames to capture and read back.
    if now < *awake_until || !screenshots.is_empty() {
        redraw.write(RequestRedraw);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_ecs::{message::Messages, system::SystemId};
    use bevy_platform::time::Instant;

    fn redraws_at(world: &mut World, system: SystemId, start: Instant, after: Duration) -> usize {
        world
            .resource_mut::<Time<Real>>()
            .update_with_instant(start + after);
        world.resource_mut::<Messages<RequestRedraw>>().clear();
        world.run_system(system).unwrap();
        world.resource::<Messages<RequestRedraw>>().len()
    }

    #[test]
    fn redraws_while_a_key_is_held_and_for_a_moment_after() {
        let mut world = World::new();
        let start = Instant::now();
        world.insert_resource(Time::<Real>::new(start));
        world.init_resource::<ButtonInput<KeyCode>>();
        world.init_resource::<ButtonInput<MouseButton>>();
        world.init_resource::<AccumulatedMouseMotion>();
        world.init_resource::<AccumulatedMouseScroll>();
        world.init_resource::<Messages<CursorMoved>>();
        world.init_resource::<Messages<RequestRedraw>>();
        // Registered, so that the system keeps its `Local` from one run to the next.
        let system = world.register_system(stay_awake_after_input);

        assert_eq!(redraws_at(&mut world, system, start, Duration::ZERO), 0);

        world
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyW);
        assert_eq!(
            redraws_at(&mut world, system, start, Duration::from_secs(1)),
            1
        );
        assert_eq!(
            redraws_at(&mut world, system, start, Duration::from_secs(5)),
            1
        );

        let mut keys = world.resource_mut::<ButtonInput<KeyCode>>();
        keys.release(KeyCode::KeyW);
        keys.clear();
        assert_eq!(
            redraws_at(
                &mut world,
                system,
                start,
                Duration::from_secs(5) + LINGER / 2
            ),
            1
        );
        assert_eq!(
            redraws_at(&mut world, system, start, Duration::from_secs(6)),
            0
        );
    }
}
