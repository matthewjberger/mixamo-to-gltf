use serde::{Deserialize, Serialize};

/// Envelope field carrying the serialized message in every `postMessage`.
pub const MESSAGE_KEY: &str = "message";
/// Envelope field carrying the transferred `OffscreenCanvas` (on `Init` only).
pub const CANVAS_KEY: &str = "canvas";
/// Envelope field carrying the transferred GLB `ArrayBuffer` (on `LoadGlb` only).
pub const GLB_KEY: &str = "glb";

/// Lifecycle phase of a forwarded touch contact.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TouchPhase {
    Started,
    Moved,
    Ended,
    Cancelled,
}

/// Playback control for the animation player on the loaded model.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum AnimationCommand {
    Play { index: u32 },
    PlayAll,
    Pause,
    Resume,
    Stop,
    Seek { time: f32 },
    SetSpeed { speed: f32 },
    SetLooping { looping: bool },
}

/// Page to worker. Pixel quantities are physical surface pixels (CSS pixels
/// times the device pixel ratio), origin at the canvas top-left.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ClientMessage {
    /// Sent once with the `OffscreenCanvas` in the transfer list.
    Init {
        width: f32,
        height: f32,
    },
    Resize {
        width: f32,
        height: f32,
    },
    /// Absolute cursor position in physical pixels. Drives the engine camera.
    PointerMove {
        x: f32,
        y: f32,
    },
    /// A mouse button changed. `button` is 0 left, 1 middle, 2 right.
    PointerButton {
        button: u8,
        pressed: bool,
    },
    /// Wheel delta in raw pixels (the worker converts to scroll lines).
    Wheel {
        delta: f32,
    },
    /// A touch contact in physical pixels. Drives the engine touch controller:
    /// one finger orbits, two fingers pan, a pinch zooms. `id` is the pointer id.
    Touch {
        id: u64,
        phase: TouchPhase,
        x: f32,
        y: f32,
    },
    /// A keyboard event. `code` is the DOM `KeyboardEvent.code`, `text` the
    /// produced character if any.
    Key {
        code: String,
        pressed: bool,
        text: Option<String>,
    },
    /// A click without drag: GPU-pick and select the entity at this position.
    Pick {
        x: f32,
        y: f32,
    },
    /// Load the converted GLB into the viewer. The bytes travel alongside this
    /// message as a transferred `ArrayBuffer` under [`GLB_KEY`].
    LoadGlb {
        name: String,
    },
    /// Remove the currently loaded model from the viewer.
    ClearModel,
    /// Drive the animation player on the loaded model.
    Animation {
        command: AnimationCommand,
    },
}

/// The selected entity, reported after a pick resolves.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SelectedEntity {
    pub id: u32,
    pub name: String,
}

/// One animation clip baked into the loaded GLB.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClipInfo {
    pub name: String,
    pub duration: f32,
}

/// Worker to page.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum WorkerMessage {
    /// The renderer is up and the render loop is running.
    Ready { adapter: String },
    /// Streamed twice a second for the HUD.
    Stats { fps: f32, entity_count: u32 },
    /// The pick result: the entity under the click, or `None` for background.
    Selected { detail: Option<SelectedEntity> },
    /// The GLB was imported and spawned into the viewer.
    ModelLoaded {
        name: String,
        clips: Vec<ClipInfo>,
        mesh_count: u32,
        skin_count: u32,
        animation_count: u32,
    },
    /// The GLB failed to import.
    ModelLoadFailed { error: String },
    /// Streamed every frame while a model is loaded so the playback bar can
    /// mirror the player.
    AnimationState {
        current: Option<u32>,
        playing: bool,
        time: f32,
        duration: f32,
        speed: f32,
        looping: bool,
    },
}

/// Response body of `POST /api/import`: the parsed Mixamo bundle.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BundleSummary {
    pub name: String,
    pub models: Vec<BundleModel>,
    pub animations: Vec<BundleAnimation>,
    pub log: Vec<String>,
}

/// A character model FBX found in the bundle.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BundleModel {
    pub name: String,
    pub mesh_count: u32,
    pub skin_count: u32,
    pub texture_count: u32,
    pub node_count: u32,
    pub size_kb: u64,
}

/// An animation FBX found in the bundle.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BundleAnimation {
    pub name: String,
    pub duration: f32,
    pub channel_count: u32,
    pub size_kb: u64,
}

/// Request body of `POST /api/build`: which model and animations to bake.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BuildRequest {
    pub model_index: u32,
    pub animation_indices: Vec<u32>,
    pub strip_root_motion: bool,
}

/// Request body of `POST /api/save`: suggested file name for the dialog.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SaveRequest {
    pub file_name: String,
}

/// Response body of `POST /api/save`. `saved_path` is `None` when the user
/// cancelled the dialog.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SaveResponse {
    pub saved_path: Option<String>,
}
