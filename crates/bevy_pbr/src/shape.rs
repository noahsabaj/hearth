//! Meshes described entirely by data, so that scene files and editors can create and change them.

use bevy_app::{App, Plugin, PostUpdate};
use bevy_asset::{Assets, Handle};
use bevy_camera::visibility::{Visibility, VisibilitySystems};
use bevy_color::Color;
use bevy_ecs::prelude::*;
use bevy_math::{Vec2, Vec3};
use bevy_mesh::{Mesh, Mesh3d, Meshable};
use bevy_reflect::prelude::*;
use bevy_shape::{Capsule3d, Cone, Cuboid, Cylinder, Plane3d, Sphere, Torus};
use bevy_transform::components::Transform;

use crate::{MeshMaterial3d, StandardMaterial};

/// A primitive shape with a plain [`StandardMaterial`].
///
/// Unlike a [`Mesh3d`] made with `Assets::add`, a shape is plain data: it survives being saved to
/// a scene file and loaded again, and editing its fields changes the mesh and material.
/// [`ShapeMeshPlugin`] keeps the entity's [`Mesh3d`] and [`MeshMaterial3d`] in step with it, so
/// don't set those yourself.
#[derive(Component, Reflect, Clone, Debug, PartialEq)]
#[reflect(Component, Default, Clone, Debug, PartialEq)]
#[require(Transform, Visibility)]
pub struct ShapeMesh {
    /// The shape.
    pub shape: Shape3d,
    /// The material's base color.
    pub color: Color,
    /// The material's metallic factor, from 0 to 1.
    pub metallic: f32,
    /// The material's perceptual roughness, from 0.089 to 1.
    pub perceptual_roughness: f32,
}

impl Default for ShapeMesh {
    fn default() -> Self {
        Self::new(Shape3d::default())
    }
}

impl ShapeMesh {
    /// A shape with a light gray, rough material.
    pub fn new(shape: Shape3d) -> Self {
        Self {
            shape,
            color: Color::srgb(0.8, 0.8, 0.8),
            metallic: 0.0,
            perceptual_roughness: 0.5,
        }
    }

    /// The shape with the given base color.
    pub fn with_color(mut self, color: impl Into<Color>) -> Self {
        self.color = color.into();
        self
    }

    fn material(&self) -> StandardMaterial {
        StandardMaterial {
            base_color: self.color,
            metallic: self.metallic,
            perceptual_roughness: self.perceptual_roughness,
            ..Default::default()
        }
    }
}

/// The shapes a [`ShapeMesh`] can take, centered on the origin.
#[derive(Reflect, Clone, Copy, Debug, PartialEq)]
#[reflect(Default, Clone, Debug, PartialEq)]
pub enum Shape3d {
    /// A box.
    Cuboid {
        /// Its full size along each axis.
        size: Vec3,
    },
    /// A sphere.
    Sphere {
        /// Its radius.
        radius: f32,
    },
    /// A flat rectangle facing up.
    Plane {
        /// Its full size along the X and Z axes.
        size: Vec2,
    },
    /// A cylinder standing along the Y axis.
    Cylinder {
        /// Its radius.
        radius: f32,
        /// Its full height.
        height: f32,
    },
    /// A capsule standing along the Y axis.
    Capsule {
        /// The radius of its ends.
        radius: f32,
        /// The length of its straight part.
        length: f32,
    },
    /// A cone standing along the Y axis, its tip up.
    Cone {
        /// The radius of its base.
        radius: f32,
        /// Its full height.
        height: f32,
    },
    /// A ring lying flat.
    Torus {
        /// The radius of the tube.
        minor_radius: f32,
        /// The distance from the center to the middle of the tube.
        major_radius: f32,
    },
}

impl Default for Shape3d {
    fn default() -> Self {
        Self::Cuboid { size: Vec3::ONE }
    }
}

impl Shape3d {
    /// Builds the shape's mesh.
    pub fn mesh(&self) -> Mesh {
        // Clamp sizes so a field dragged through zero doesn't make a degenerate mesh.
        let positive = |value: f32| value.abs().max(0.001);
        match *self {
            Self::Cuboid { size } => Cuboid::from_size(size.abs().max(Vec3::splat(0.001))).into(),
            Self::Sphere { radius } => Sphere::new(positive(radius)).into(),
            Self::Plane { size } => Plane3d::default()
                .mesh()
                .size(positive(size.x), positive(size.y))
                .into(),
            Self::Cylinder { radius, height } => {
                Cylinder::new(positive(radius), positive(height)).into()
            }
            Self::Capsule { radius, length } => {
                Capsule3d::new(positive(radius), positive(length)).into()
            }
            Self::Cone { radius, height } => Cone::new(positive(radius), positive(height)).into(),
            Self::Torus {
                minor_radius,
                major_radius,
            } => {
                let minor = positive(minor_radius);
                Torus {
                    minor_radius: minor,
                    major_radius: positive(major_radius).max(minor * 1.01),
                }
                .into()
            }
        }
    }
}

/// The assets made for a [`ShapeMesh`], and the shape they were made for.
#[derive(Component)]
struct ShapeMeshAssets {
    shape: Shape3d,
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
}

/// Keeps the [`Mesh3d`] and [`MeshMaterial3d`] of each [`ShapeMesh`] in step with it.
pub struct ShapeMeshPlugin;

impl Plugin for ShapeMeshPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<ShapeMesh>().add_systems(
            PostUpdate,
            update_shape_meshes.before(VisibilitySystems::CalculateBounds),
        );
    }
}

fn update_shape_meshes(
    shapes: Query<(Entity, &ShapeMesh, Option<&ShapeMeshAssets>), Changed<ShapeMesh>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    for (entity, shape, made) in &shapes {
        let mesh = match made {
            Some(made) if made.shape == shape.shape => made.mesh.clone(),
            _ => meshes.add(shape.shape.mesh()),
        };
        let reused = made.and_then(|made| {
            let mut material = materials.get_mut(&made.material)?;
            *material = shape.material();
            Some(made.material.clone())
        });
        let material = reused.unwrap_or_else(|| materials.add(shape.material()));
        commands.entity(entity).insert((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material.clone()),
            ShapeMeshAssets {
                shape: shape.shape,
                mesh,
                material,
            },
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shape_makes_a_mesh() {
        let shapes = [
            Shape3d::default(),
            Shape3d::Sphere { radius: 0.5 },
            Shape3d::Plane { size: Vec2::ONE },
            Shape3d::Cylinder {
                radius: 0.5,
                height: 1.0,
            },
            Shape3d::Capsule {
                radius: 0.5,
                length: 1.0,
            },
            Shape3d::Cone {
                radius: 0.5,
                height: 1.0,
            },
            Shape3d::Torus {
                minor_radius: 0.0,
                major_radius: 0.0,
            },
        ];
        for shape in shapes {
            assert!(shape.mesh().count_vertices() > 0, "{shape:?}");
        }
    }
}
