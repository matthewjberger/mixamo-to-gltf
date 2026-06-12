use crate::ecs::ViewerWorld;
use nightshade::prelude::*;

/// Builds the scene: atmosphere, lighting, and the orbit camera.
pub fn initialize(_viewer_world: &mut ViewerWorld, world: &mut World) {
    if let Some((width, height)) = world.resources.window.cached_viewport_size {
        world.resources.window.active_viewport_rect =
            Some(nightshade::ecs::window::resources::ViewportRect {
                x: 0.0,
                y: 0.0,
                width: width as f32,
                height: height as f32,
            });
    }
    world.resources.render_settings.atmosphere = Atmosphere::Nebula;
    capture_procedural_atmosphere_ibl(world, Atmosphere::Nebula, 0.0);
    world.resources.debug_draw.show_grid = true;
    world.resources.debug_draw.selection_outline_enabled = true;
    world.resources.debug_draw.selection_outline_color = [1.0, 0.5, 0.15, 1.0];

    spawn_sun(world);

    let camera = spawn_pan_orbit_camera(
        world,
        Vec3::new(0.0, 1.0, 0.0),
        4.0,
        0.5,
        0.3,
        "Main Camera".to_string(),
    );
    world.resources.active_camera = Some(camera);
}
