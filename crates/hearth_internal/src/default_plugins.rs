use hearth_app::{plugin_group, Plugin};

plugin_group! {
    /// This plugin group will add all the default plugins for a *Hearth* application:
    pub struct DefaultPlugins {
        hearth_app:::PanicHandlerPlugin,
        #[cfg(feature = "hearth_log")]
        hearth_log:::LogPlugin,
        hearth_app:::TaskPoolPlugin,
        hearth_diagnostic:::FrameCountPlugin,
        hearth_time:::TimePlugin,
        hearth_transform:::TransformPlugin,
        hearth_diagnostic:::DiagnosticsPlugin,
        hearth_input:::InputPlugin,
        #[cfg(feature = "hearth_input_focus")]
        hearth_input_focus:::InputFocusPlugin,
        #[cfg(feature = "hearth_input_focus")]
        hearth_input_focus:::InputDispatchPlugin,
        #[custom(cfg(not(feature = "hearth_window")))]
        hearth_app:::ScheduleRunnerPlugin,
        #[cfg(feature = "hearth_window")]
        hearth_window:::WindowPlugin,
        #[cfg(feature = "hearth_window")]
        hearth_a11y:::AccessibilityPlugin,
        #[cfg(feature = "std")]
        #[custom(cfg(any(all(unix, not(target_os = "horizon")), windows)))]
        hearth_app:::TerminalCtrlCHandlerPlugin,
        // NOTE: Load this before AssetPlugin to properly register http asset sources.
        #[cfg(feature = "hearth_asset")]
        #[custom(cfg(any(feature = "http", feature = "https")))]
        hearth_asset::io::web:::WebAssetPlugin,
        #[cfg(feature = "hearth_asset")]
        hearth_asset:::AssetPlugin,
        #[cfg(feature = "hearth_world_serialization")]
        hearth_world_serialization:::WorldSerializationPlugin,
        #[cfg(feature = "hearth_scene")]
        hearth_scene:::ScenePlugin,
        // NOTE: WinitPlugin needs to be after AssetPlugin because of custom cursors.
        #[cfg(feature = "hearth_winit")]
        hearth_winit:::WinitPlugin,
        #[custom(cfg(all(feature = "dlss", not(feature = "force_disable_dlss"))))]
        hearth_anti_alias::dlss:::DlssInitPlugin,
        #[cfg(feature = "hearth_render")]
        hearth_render:::RenderPlugin,
        // NOTE: Load this after renderer initialization so that it knows about the supported
        // compressed texture formats.
        #[cfg(feature = "hearth_image")]
        hearth_image:::ImagePlugin,
        #[cfg(feature = "hearth_mesh")]
        hearth_mesh:::MeshPlugin,
        #[cfg(feature = "hearth_camera")]
        hearth_camera:::CameraPlugin,
        #[cfg(feature = "hearth_light")]
        hearth_light:::LightPlugin,
        #[cfg(feature = "hearth_render")]
        #[custom(cfg(all(not(target_arch = "wasm32"), feature = "multi_threaded")))]
        hearth_render::pipelined_rendering:::PipelinedRenderingPlugin,
        #[cfg(feature = "hearth_core_pipeline")]
        hearth_core_pipeline:::CorePipelinePlugin,
        #[cfg(feature = "hearth_post_process")]
        hearth_post_process:::PostProcessPlugin,
        #[cfg(feature = "hearth_anti_alias")]
        hearth_anti_alias:::AntiAliasPlugin,
        #[cfg(feature = "hearth_sprite")]
        hearth_sprite:::SpritePlugin,
        #[cfg(feature = "hearth_sprite_render")]
        hearth_sprite_render:::SpriteRenderPlugin,
        #[cfg(feature = "hearth_clipboard")]
        hearth_clipboard:::ClipboardPlugin,
        #[cfg(feature = "hearth_text")]
        hearth_text:::TextPlugin,
        #[cfg(feature = "hearth_ui")]
        hearth_ui:::UiPlugin,
        #[cfg(feature = "hearth_ui_render")]
        hearth_ui_render:::UiRenderPlugin,
        #[cfg(feature = "hearth_gltf")]
        hearth_gltf:::GltfPlugin,
        #[cfg(feature = "hearth_pbr")]
        hearth_pbr:::PbrPlugin,
        #[cfg(feature = "hearth_audio")]
        hearth_audio:::AudioPlugin,
        #[cfg(feature = "hearth_gilrs")]
        hearth_gilrs:::GilrsPlugin,
        #[cfg(feature = "hearth_animation")]
        hearth_animation:::AnimationPlugin,
        #[cfg(feature = "hearth_gizmos")]
        hearth_gizmos:::GizmoPlugin,
        #[cfg(feature = "hearth_gizmos_render")]
        hearth_gizmos_render:::GizmoRenderPlugin,
        #[cfg(feature = "hearth_state")]
        hearth_state::app:::StatesPlugin,
        #[cfg(feature = "hearth_ci_testing")]
        hearth_dev_tools::ci_testing:::CiTestingPlugin,
        #[cfg(feature = "render_dev_tools")]
        hearth_dev_tools::render_debug:::RenderDebugOverlayPlugin,
        #[cfg(feature = "hotpatching")]
        hearth_app::hotpatch:::HotPatchPlugin,
        #[plugin_group]
        #[cfg(feature = "hearth_ui_widgets")]
        hearth_ui_widgets:::UiWidgetsPlugins,
        #[plugin_group]
        #[cfg(feature = "hearth_picking")]
        hearth_picking:::DefaultPickingPlugins,
        #[doc(hidden)]
        :IgnoreAmbiguitiesPlugin,
    }
    /// [`DefaultPlugins`] obeys *Cargo* *feature* flags. Users may exert control over this plugin group
    /// by disabling `default-features` in their `Cargo.toml` and enabling only those features
    /// that they wish to use.
    ///
    /// [`DefaultPlugins`] contains all the plugins typically required to build
    /// a *Hearth* application which includes a *window* and presentation components.
    /// For the absolute minimum number of plugins needed to run a Hearth application, see [`MinimalPlugins`].
}

#[derive(Default)]
struct IgnoreAmbiguitiesPlugin;

impl Plugin for IgnoreAmbiguitiesPlugin {
    #[expect(
        clippy::allow_attributes,
        reason = "`unused_variables` is not always linted"
    )]
    #[allow(
        unused_variables,
        reason = "The `app` parameter is used only if a combination of crates that contain ambiguities with each other are enabled."
    )]
    fn build(&self, app: &mut hearth_app::App) {
        #[cfg(all(feature = "hearth_ui_widgets", feature = "hearth_sprite"))]
        if app.is_plugin_added::<hearth_ui_widgets::TextInputPlugin>()
            && app.is_plugin_added::<hearth_sprite::SpritePlugin>()
        {
            // update_ime_position reads Window to reposition the IME cursor, while
            // update_text2d_layout writes Window bounds for text wrapping.
            app.ignore_ambiguity(
                hearth_app::PostUpdate,
                hearth_ui_widgets::ImeSystems::UpdatePosition,
                hearth_sprite::update_text2d_layout,
            );
        }

        // hearth_ui owns the Transform and cannot be animated
        #[cfg(all(feature = "hearth_animation", feature = "hearth_ui"))]
        if app.is_plugin_added::<hearth_animation::AnimationPlugin>()
            && app.is_plugin_added::<hearth_ui::UiPlugin>()
        {
            app.ignore_ambiguity(
                hearth_app::PostUpdate,
                hearth_animation::advance_animations,
                hearth_ui::UiSystems::Layout,
            );
            app.ignore_ambiguity(
                hearth_app::PostUpdate,
                hearth_animation::animate_targets,
                hearth_ui::UiSystems::Layout,
            );
        }
    }
}

plugin_group! {
    /// This plugin group will add the minimal plugins for a *Hearth* application:
    pub struct MinimalPlugins {
        hearth_app:::TaskPoolPlugin,
        hearth_diagnostic:::FrameCountPlugin,
        hearth_time:::TimePlugin,
        hearth_app:::ScheduleRunnerPlugin,
        #[cfg(feature = "hearth_ci_testing")]
        hearth_dev_tools::ci_testing:::CiTestingPlugin,
    }
    /// This plugin group represents the absolute minimum, bare-bones, hearth application.
    /// Use this if you want to have absolute control over the plugins used.
    ///
    /// It includes a [schedule runner (`ScheduleRunnerPlugin`)](crate::app::ScheduleRunnerPlugin)
    /// to provide functionality that would otherwise be driven by a windowed application's
    /// *event loop* or *message loop*.
    ///
    /// By default, this loop will run as fast as possible, which can result in high CPU usage.
    /// You can add a delay using [`run_loop`](crate::app::ScheduleRunnerPlugin::run_loop),
    /// or remove the loop using [`run_once`](crate::app::ScheduleRunnerPlugin::run_once).
    /// # Example:
    /// ```rust, no_run
    /// # use std::time::Duration;
    /// # use hearth_app::{App, PluginGroup, ScheduleRunnerPlugin};
    /// # use hearth_internal::MinimalPlugins;
    /// App::new().add_plugins(MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(
    ///     // Run 60 times per second.
    ///     Duration::from_secs_f64(1.0 / 60.0),
    /// ))).run();
}
