use nightshade::prelude::Entity;

/// The loaded preview model: spawned root entities and the camera-fit
/// countdown that waits for transforms to settle after a load.
#[derive(Default)]
pub struct ViewerState {
    pub model_roots: Vec<Entity>,
    pub pending_fit_frames: u8,
}

/// The currently selected engine entity.
#[derive(Default)]
pub struct Selection {
    pub selected: Option<Entity>,
}

/// Whether a GPU pick is in flight.
#[derive(Default)]
pub struct Picking {
    pub pending: bool,
}
