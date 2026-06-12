use leptos::prelude::*;
use protocol::{CANVAS_KEY, ClientMessage, GLB_KEY, MESSAGE_KEY, WorkerMessage};
use wasm_bindgen::prelude::*;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{MessageEvent, OffscreenCanvas, Worker, WorkerOptions, WorkerType};

use crate::state::AppState;

/// The page side of the worker conversation. Data only; behavior is the free
/// functions below.
#[derive(Clone)]
pub struct Bridge {
    worker: Worker,
}

/// Spawns the worker, wires its `onmessage` to the state signals, sends `Init`
/// with the transferred canvas, and returns the bridge.
pub fn connect(offscreen: OffscreenCanvas, width: f32, height: f32, state: AppState) -> Bridge {
    let options = WorkerOptions::new();
    options.set_type(WorkerType::Module);
    let worker =
        Worker::new_with_options("runtime/worker.js", &options).expect("failed to spawn worker");

    let onmessage = Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
        let data = event.data();
        let Ok(payload) = js_sys::Reflect::get(&data, &JsValue::from_str(MESSAGE_KEY)) else {
            return;
        };
        let Ok(message) = serde_wasm_bindgen::from_value::<WorkerMessage>(payload) else {
            return;
        };
        match message {
            WorkerMessage::Ready { adapter } => {
                state.adapter.set(adapter);
                state.ready.set(true);
            }
            WorkerMessage::Stats { fps, entity_count } => {
                state.fps.set(fps);
                state.entity_count.set(entity_count);
            }
            WorkerMessage::Selected { detail } => state.selected.set(detail),
            WorkerMessage::ModelLoaded {
                name,
                clips,
                mesh_count,
                skin_count,
                animation_count,
            } => {
                state.status.set(format!(
                    "Loaded {name}: {mesh_count} meshes, {skin_count} skins, {animation_count} animations"
                ));
                state.clips.set(clips);
                state.anim_current.set(None);
                state.anim_time.set(0.0);
            }
            WorkerMessage::ModelLoadFailed { error } => {
                state
                    .error
                    .set(Some(format!("Viewer failed to load GLB: {error}")));
                state.busy.set(None);
            }
            WorkerMessage::AnimationState {
                current,
                playing,
                time,
                duration,
                speed,
                looping,
            } => {
                state.anim_current.set(current);
                state.anim_playing.set(playing);
                if !state.scrubbing.get_untracked() {
                    state.anim_time.set(time);
                }
                state.anim_duration.set(duration);
                state.anim_speed.set(speed);
                state.anim_looping.set(looping);
            }
        }
    });
    worker.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
    onmessage.forget();

    let bridge = Bridge { worker };
    send_init(&bridge, offscreen, width, height);
    bridge
}

/// Forwards a message to the worker inside the `{ message }` envelope.
pub fn send(bridge: &Bridge, message: &ClientMessage) {
    let envelope = js_sys::Object::new();
    let value = serde_wasm_bindgen::to_value(message).unwrap_or(JsValue::NULL);
    let _ = js_sys::Reflect::set(&envelope, &JsValue::from_str(MESSAGE_KEY), &value);
    let _ = bridge.worker.post_message(&envelope);
}

/// Transfers a GLB `ArrayBuffer` to the worker for viewing.
pub fn send_glb(bridge: &Bridge, name: &str, buffer: &js_sys::ArrayBuffer) {
    let envelope = js_sys::Object::new();
    let message = ClientMessage::LoadGlb {
        name: name.to_string(),
    };
    let value = serde_wasm_bindgen::to_value(&message).unwrap_or(JsValue::NULL);
    let _ = js_sys::Reflect::set(&envelope, &JsValue::from_str(MESSAGE_KEY), &value);
    let _ = js_sys::Reflect::set(&envelope, &JsValue::from_str(GLB_KEY), buffer);
    let transfer = js_sys::Array::of1(buffer);
    let _ = bridge
        .worker
        .post_message_with_transfer(&envelope, &transfer);
}

fn send_init(bridge: &Bridge, canvas: OffscreenCanvas, width: f32, height: f32) {
    let envelope = js_sys::Object::new();
    let value = serde_wasm_bindgen::to_value(&ClientMessage::Init { width, height })
        .unwrap_or(JsValue::NULL);
    let _ = js_sys::Reflect::set(&envelope, &JsValue::from_str(MESSAGE_KEY), &value);
    let _ = js_sys::Reflect::set(&envelope, &JsValue::from_str(CANVAS_KEY), &canvas);
    let transfer = js_sys::Array::of1(&canvas);
    let _ = bridge
        .worker
        .post_message_with_transfer(&envelope, &transfer);
}
