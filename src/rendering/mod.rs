//! Swappable output backends for composed game frames.

mod extruded;
mod figures;
mod light;
mod settings;
mod text;
mod volume;
// Desktop only: the web build leaves 3D rendering out to keep its download
// small.
#[cfg(not(target_family = "wasm"))]
mod walls_3d;

pub use light::LightEffectsPlugin;
pub use text::{TextRenderStats, TextRendererPlugin};
pub use volume::WallVolumePlugin;
#[cfg(not(target_family = "wasm"))]
pub use walls_3d::ExtrudedWallRendererPlugin;

/// The view functions the text renderer's shaders import
/// (`gridvail::view`), loaded once by whichever plugin needs them first.
fn load_view_shader(app: &mut bevy::prelude::App) {
    #[derive(bevy::prelude::Resource)]
    struct ViewShaderLoaded;
    if app.world().contains_resource::<ViewShaderLoaded>() {
        return;
    }
    app.insert_resource(ViewShaderLoaded);
    bevy::shader::load_shader_library!(app, "view.wgsl");
}
