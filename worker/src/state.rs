use crate::ecs::ViewerWorld;
use crate::systems;
use nightshade::prelude::*;

/// The application root. Holds the viewer-side ECS world and forwards each
/// `State` hook to system functions in `src/systems/`.
#[derive(Default)]
pub struct Viewer {
    pub viewer_world: ViewerWorld,
}

impl State for Viewer {
    fn initialize(&mut self, world: &mut World) {
        systems::setup::initialize(&mut self.viewer_world, world);
    }

    fn run_systems(&mut self, world: &mut World) {
        pan_orbit_camera_system(world);
        systems::picking::apply(&mut self.viewer_world, world);
        systems::viewer::tick(&mut self.viewer_world, world);
    }
}
