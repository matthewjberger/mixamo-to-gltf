use leptos::prelude::*;
use protocol::{BundleSummary, ClipInfo, SelectedEntity};

/// All page state, grouped as signals. `Copy`, so it threads into every
/// component and closure without cloning. `clips` is non-empty exactly when
/// a model is loaded in the viewer.
#[derive(Clone, Copy)]
pub struct AppState {
    pub ready: RwSignal<bool>,
    pub adapter: RwSignal<String>,
    pub fps: RwSignal<f32>,
    pub entity_count: RwSignal<u32>,
    pub selected: RwSignal<Option<SelectedEntity>>,
    pub grabbing: RwSignal<bool>,
    pub drag_over: RwSignal<bool>,
    pub busy: RwSignal<Option<String>>,
    pub status: RwSignal<String>,
    pub error: RwSignal<Option<String>>,
    pub log: RwSignal<Vec<String>>,
    pub bundle: RwSignal<Option<BundleSummary>>,
    pub model_index: RwSignal<usize>,
    pub animation_selected: RwSignal<Vec<bool>>,
    pub strip_root_motion: RwSignal<bool>,
    pub glb_size: RwSignal<Option<usize>>,
    pub clips: RwSignal<Vec<ClipInfo>>,
    pub anim_current: RwSignal<Option<u32>>,
    pub anim_playing: RwSignal<bool>,
    pub anim_time: RwSignal<f32>,
    pub anim_duration: RwSignal<f32>,
    pub anim_speed: RwSignal<f32>,
    pub anim_looping: RwSignal<bool>,
    pub scrubbing: RwSignal<bool>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            ready: RwSignal::new(false),
            adapter: RwSignal::new(String::new()),
            fps: RwSignal::new(0.0),
            entity_count: RwSignal::new(0),
            selected: RwSignal::new(None),
            grabbing: RwSignal::new(false),
            drag_over: RwSignal::new(false),
            busy: RwSignal::new(None),
            status: RwSignal::new("Drop a Mixamo .zip to begin".to_string()),
            error: RwSignal::new(None),
            log: RwSignal::new(Vec::new()),
            bundle: RwSignal::new(None),
            model_index: RwSignal::new(0),
            animation_selected: RwSignal::new(Vec::new()),
            strip_root_motion: RwSignal::new(true),
            glb_size: RwSignal::new(None),
            clips: RwSignal::new(Vec::new()),
            anim_current: RwSignal::new(None),
            anim_playing: RwSignal::new(false),
            anim_time: RwSignal::new(0.0),
            anim_duration: RwSignal::new(0.0),
            anim_speed: RwSignal::new(1.0),
            anim_looping: RwSignal::new(true),
            scrubbing: RwSignal::new(false),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
