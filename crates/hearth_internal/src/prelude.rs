#[doc(hidden)]
pub use crate::{
    app::prelude::*, ecs::prelude::*, input::prelude::*, math::prelude::*, platform::prelude::*,
    reflect::prelude::*, shape::prelude::*, time::prelude::*, transform::prelude::*,
    utils::prelude::*, DefaultPlugins, MinimalPlugins,
};

#[doc(hidden)]
#[cfg(feature = "hearth_log")]
pub use crate::log::prelude::*;

#[doc(hidden)]
#[cfg(feature = "hearth_window")]
pub use crate::window::prelude::*;

#[doc(hidden)]
#[cfg(feature = "hearth_image")]
pub use crate::image::prelude::*;

#[doc(hidden)]
#[cfg(feature = "hearth_curve")]
pub use crate::curve::prelude::*;

#[doc(hidden)]
#[cfg(feature = "hearth_mesh")]
pub use crate::mesh::prelude::*;

#[doc(hidden)]
#[cfg(feature = "hearth_light")]
pub use crate::light::prelude::*;

#[doc(hidden)]
#[cfg(feature = "hearth_camera")]
pub use crate::camera::prelude::*;

#[doc(hidden)]
#[cfg(feature = "hearth_shader")]
pub use crate::shader::prelude::*;

#[doc(hidden)]
#[cfg(feature = "hearth_material")]
pub use crate::material::prelude::*;

pub use hearth_derive::{hearth_main, Deref, DerefMut};

#[doc(hidden)]
#[cfg(feature = "hearth_asset")]
pub use crate::asset::prelude::*;

#[doc(hidden)]
#[cfg(feature = "hearth_audio")]
pub use crate::audio::prelude::*;

#[doc(hidden)]
#[cfg(feature = "hearth_animation")]
pub use crate::animation::prelude::*;

#[doc(hidden)]
#[cfg(feature = "hearth_color")]
pub use crate::color::prelude::*;

#[doc(hidden)]
#[cfg(feature = "hearth_pbr")]
pub use crate::pbr::prelude::*;

#[doc(hidden)]
#[cfg(feature = "hearth_render")]
pub use crate::render::prelude::*;

#[doc(hidden)]
#[cfg(feature = "hearth_scene")]
pub use crate::scene::prelude::*;

#[doc(hidden)]
#[cfg(feature = "hearth_world_serialization")]
pub use crate::world_serialization::prelude::*;

#[doc(hidden)]
#[cfg(feature = "hearth_sprite")]
pub use crate::sprite::prelude::*;

#[doc(hidden)]
#[cfg(feature = "hearth_sprite_render")]
pub use crate::sprite_render::prelude::*;

#[doc(hidden)]
#[cfg(feature = "hearth_text")]
pub use crate::text::prelude::*;

#[doc(hidden)]
#[cfg(feature = "hearth_ui")]
pub use crate::ui::prelude::*;

#[doc(hidden)]
#[cfg(feature = "hearth_ui_render")]
pub use crate::ui_render::prelude::*;

#[doc(hidden)]
#[cfg(feature = "hearth_gizmos")]
pub use crate::gizmos::prelude::*;

#[doc(hidden)]
#[cfg(feature = "hearth_gilrs")]
pub use crate::gilrs::*;

#[doc(hidden)]
#[cfg(feature = "hearth_state")]
pub use crate::state::prelude::*;

#[doc(hidden)]
#[cfg(feature = "hearth_gltf")]
pub use crate::gltf::prelude::*;

#[doc(hidden)]
#[cfg(feature = "hearth_picking")]
pub use crate::picking::prelude::*;

#[doc(hidden)]
#[cfg(feature = "hearth_clipboard")]
pub use crate::clipboard::prelude::*;
