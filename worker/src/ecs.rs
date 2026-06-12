mod components;
mod resources;

pub use components::*;
pub use resources::*;

use nightshade::prelude::freecs;

freecs::ecs! {
    ViewerWorld {
        marker: Marker => MARKER,
    }
    Tags {
    }
    Events {
    }
    Resources {
        viewer: ViewerState,
        selection: Selection,
        picking: Picking,
    }
}
