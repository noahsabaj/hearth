use hearth_asset::{AssetServer, Handle};
use hearth_ecs::{resource::Resource, world::FromWorld};
use hearth_render::render_resource::VertexState;
use hearth_shader::Shader;

/// A shader that renders to the whole screen. Useful for post-processing.
#[derive(Resource, Clone)]
pub struct FullscreenShader(Handle<Shader>);

impl FromWorld for FullscreenShader {
    fn from_world(world: &mut hearth_ecs::world::World) -> Self {
        Self(
            world
                .resource::<AssetServer>()
                .load("embedded://hearth_core_pipeline/fullscreen_vertex_shader.wesl"),
        )
    }
}

impl FullscreenShader {
    /// Gets the raw shader handle.
    pub fn shader(&self) -> Handle<Shader> {
        self.0.clone()
    }

    /// Creates a [`VertexState`] that uses the [`FullscreenShader`] to output a
    /// ```wgsl
    /// struct FullscreenVertexOutput {
    ///     @builtin(position)
    ///     position: vec4<f32>;
    ///     @location(0)
    ///     uv: vec2<f32>;
    /// };
    /// ```
    /// from the vertex shader.
    /// The draw call should render one triangle: `render_pass.draw(0..3, 0..1);`
    pub fn to_vertex_state(&self) -> VertexState {
        VertexState {
            shader: self.0.clone(),
            shader_defs: Vec::new(),
            entry_point: Some("fullscreen_vertex_shader".into()),
            buffers: Vec::new(),
            constants: Vec::new(),
        }
    }
}
