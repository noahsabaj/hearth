//! Utilities for hotpatching code.
extern crate alloc;

use alloc::sync::Arc;

#[cfg(not(target_family = "wasm"))]
use dioxus_devtools::connect_subsecond;
use dioxus_devtools::subsecond;
#[cfg(feature = "reflect_auto_register")]
use hearth_ecs::schedule::IntoScheduleConfigs;
use hearth_ecs::{
    change_detection::DetectChangesMut, message::MessageWriter, system::ResMut, HotPatchChanges,
    HotPatched,
};

pub use dioxus_devtools::subsecond::{call, HotFunction};

use crate::{Last, Plugin};

/// Plugin connecting to Dioxus CLI to enable hot patching.
#[derive(Default)]
pub struct HotPatchPlugin;

impl Plugin for HotPatchPlugin {
    fn build(&self, app: &mut crate::App) {
        let (sender, receiver) = crossbeam_channel::bounded::<HotPatched>(1);

        // Connects to the dioxus CLI that will handle rebuilds
        // This will open a connection to the dioxus CLI to receive updated jump tables
        // Sends a `HotPatched` message through the channel when the jump table is updated
        #[cfg(not(target_family = "wasm"))]
        connect_subsecond();
        subsecond::register_handler(Arc::new(move || {
            sender.send(HotPatched).unwrap();
        }));

        // Adds a system that will read the channel for new `HotPatched` messages, send the message, and update change detection.
        app.init_resource::<HotPatchChanges>()
            .add_message::<HotPatched>()
            .add_systems(
                Last,
                move |mut hot_patched_writer: MessageWriter<HotPatched>,
                      mut res: ResMut<HotPatchChanges>| {
                    if receiver.try_recv().is_ok() {
                        hot_patched_writer.write_default();
                        res.set_changed();
                    }
                },
            );

        #[cfg(feature = "reflect_auto_register")]
        app.add_systems(
            crate::First,
            (move |registry: hearth_ecs::system::Res<hearth_ecs::reflect::AppTypeRegistry>| {
                registry.write().register_derived_types();
            })
            .run_if(hearth_ecs::schedule::common_conditions::on_message::<HotPatched>),
        );
    }
}
