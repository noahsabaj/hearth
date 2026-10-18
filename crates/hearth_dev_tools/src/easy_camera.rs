use hearth_app::{App, Plugin, PostUpdate};
use hearth_camera::Camera;
use hearth_ecs::prelude::*;
use hearth_math::{Quat, StableInterpolate, Vec3};
use hearth_time::Time;
use hearth_transform::{components::Transform, TransformSystems};

/// Plugin to move the camera smoothly according to the current time
pub struct EasyCameraMovementPlugin {
    /// Decay rate for the camera movement
    pub decay_rate: f32,
}

impl Default for EasyCameraMovementPlugin {
    fn default() -> Self {
        Self { decay_rate: 1.0 }
    }
}

/// Move the camera to the given position
#[derive(Component)]
pub struct CameraMovement {
    /// Target position for the camera movement
    pub translation: Vec3,
    /// Target rotation for the camera movement
    pub rotation: Quat,
}

impl Plugin for EasyCameraMovementPlugin {
    fn build(&self, app: &mut App) {
        let decay_rate = self.decay_rate;
        app.add_systems(
            PostUpdate,
            (move |mut query: Single<(&mut Transform, &CameraMovement), With<Camera>>,
                   time: Res<Time>| {
                {
                    {
                        let target = query.1;
                        query.0.translation.smooth_nudge(
                            &target.translation,
                            decay_rate,
                            time.delta_secs(),
                        );
                        query.0.rotation.smooth_nudge(
                            &target.rotation,
                            decay_rate,
                            time.delta_secs(),
                        );
                    }
                }
            })
            .before(TransformSystems::Propagate),
        );
    }
}
