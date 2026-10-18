use hearth_ecs::{prelude::Component, reflect::ReflectComponent};
use hearth_reflect::{std_traits::ReflectDefault, Reflect};

/// Marker struct for labels
#[derive(Component, Debug, Default, Clone, Copy, Reflect)]
#[reflect(Component, Default, Debug, Clone)]
pub struct Label;
