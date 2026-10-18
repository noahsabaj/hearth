use crate::Material;
use derive_more::derive::From;
use hearth_asset::{AsAssetId, AssetId, Handle};
use hearth_derive::{Deref, DerefMut};
use hearth_ecs::{component::Component, reflect::ReflectComponent, template::FromTemplate};
use hearth_reflect::{std_traits::ReflectDefault, Reflect};

/// A [material](Material) used for rendering a [`Mesh3d`].
///
/// See [`Material`] for general information about 3D materials and how to implement your own materials.
///
/// [`Mesh3d`]: hearth_mesh::Mesh3d
///
/// # Example
///
/// ```
/// # use hearth_pbr::{Material, MeshMaterial3d, StandardMaterial};
/// # use hearth_ecs::prelude::*;
/// # use hearth_mesh::{Mesh, Mesh3d};
/// # use hearth_color::palettes::basic::RED;
/// # use hearth_asset::Assets;
/// # use hearth_shape::Capsule3d;
/// #
/// // Spawn an entity with a mesh using `StandardMaterial`.
/// fn setup(
///     mut commands: Commands,
///     mut meshes: ResMut<Assets<Mesh>>,
///     mut materials: ResMut<Assets<StandardMaterial>>,
/// ) {
///     commands.spawn((
///         Mesh3d(meshes.add(Capsule3d::default())),
///         MeshMaterial3d(materials.add(StandardMaterial {
///             base_color: RED.into(),
///             ..Default::default()
///         })),
///     ));
/// }
/// ```
#[derive(Component, FromTemplate, Clone, Debug, Deref, DerefMut, Reflect, From)]
#[reflect(Component, Default, Clone, PartialEq)]
#[component(summary_tick)]
pub struct MeshMaterial3d<M: Material>(pub Handle<M>);

impl<M: Material> Default for MeshMaterial3d<M> {
    fn default() -> Self {
        Self(Handle::default())
    }
}

impl<M: Material> PartialEq for MeshMaterial3d<M> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<M: Material> Eq for MeshMaterial3d<M> {}

impl<M: Material> From<MeshMaterial3d<M>> for AssetId<M> {
    fn from(material: MeshMaterial3d<M>) -> Self {
        material.id()
    }
}

impl<M: Material> From<&MeshMaterial3d<M>> for AssetId<M> {
    fn from(material: &MeshMaterial3d<M>) -> Self {
        material.id()
    }
}

impl<M: Material> AsAssetId for MeshMaterial3d<M> {
    type Asset = M;

    fn as_asset_id(&self) -> AssetId<Self::Asset> {
        self.id()
    }
}
