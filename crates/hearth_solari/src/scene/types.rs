use derive_more::derive::From;
use hearth_asset::Handle;
use hearth_derive::{Deref, DerefMut};
use hearth_ecs::{component::Component, prelude::ReflectComponent, template::FromTemplate};
use hearth_mesh::Mesh;
use hearth_pbr::{MeshMaterial3d, StandardMaterial};
use hearth_reflect::{prelude::ReflectDefault, Reflect};
use hearth_render::sync_world::SyncToRenderWorld;
use hearth_transform::components::Transform;

/// A mesh component used for raytracing.
///
/// The mesh used in this component must declare the BLAS its material needs in
/// [`Mesh::raytracing`],
/// use the following set of vertex attributes: `{POSITION, NORMAL, UV_0, TANGENT}`, use [`hearth_mesh::PrimitiveTopology::TriangleList`],
/// and use [`hearth_mesh::Indices::U32`].
///
/// The material used for this entity must be [`MeshMaterial3d<StandardMaterial>`].
#[derive(
    Component, FromTemplate, Clone, Debug, Default, Deref, DerefMut, Reflect, PartialEq, Eq, From,
)]
#[reflect(Component, Default, Clone, PartialEq)]
#[require(MeshMaterial3d<StandardMaterial>, Transform, SyncToRenderWorld)]
pub struct RaytracingMesh3d(pub Handle<Mesh>);
