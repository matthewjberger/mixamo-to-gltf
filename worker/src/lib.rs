//! The wasm module inside the web worker. Owns the `OffscreenCanvas`, the
//! engine `World`, and the render loop. The page talks to it exclusively
//! through the `protocol` messages.

mod ecs;
mod state;
mod systems;

use std::cell::RefCell;
use std::rc::Rc;

use nightshade::prelude::*;
use nightshade::render::wgpu::create_wgpu_renderer;
use protocol::{CANVAS_KEY, ClientMessage, GLB_KEY, MESSAGE_KEY, WorkerMessage};
use wasm_bindgen::prelude::*;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::spawn_local;
use web_sys::{DedicatedWorkerGlobalScope, MessageEvent, OffscreenCanvas};

use crate::state::Viewer;

type AppSlot = Rc<RefCell<Option<App>>>;
type PendingMessages = Rc<RefCell<Vec<JsValue>>>;

struct App {
    world: World,
    renderer: WgpuRenderer,
    state: Viewer,
}

#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();

    let scope: DedicatedWorkerGlobalScope = js_sys::global().unchecked_into();
    let app_slot: AppSlot = Rc::new(RefCell::new(None));
    let pending: PendingMessages = Rc::new(RefCell::new(Vec::new()));

    let handler_scope = scope.clone();
    let onmessage = Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
        handle_data(&handler_scope, &app_slot, &pending, event.data());
    });
    scope.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
    onmessage.forget();
}

fn handle_data(
    scope: &DedicatedWorkerGlobalScope,
    app_slot: &AppSlot,
    pending: &PendingMessages,
    data: JsValue,
) {
    let Ok(payload) = js_sys::Reflect::get(&data, &JsValue::from_str(MESSAGE_KEY)) else {
        return;
    };
    let Ok(message) = serde_wasm_bindgen::from_value::<ClientMessage>(payload) else {
        return;
    };

    if !matches!(message, ClientMessage::Init { .. }) && app_slot.borrow().is_none() {
        pending.borrow_mut().push(data);
        return;
    }

    match message {
        ClientMessage::Init { width, height } => {
            let Some(canvas) = canvas_from(&data) else {
                return;
            };
            let scope = scope.clone();
            let app_slot = app_slot.clone();
            let pending = pending.clone();
            spawn_local(async move {
                let app = create_app(canvas, width, height).await;
                *app_slot.borrow_mut() = Some(app);
                let queued = std::mem::take(&mut *pending.borrow_mut());
                for data in queued {
                    handle_data(&scope, &app_slot, &pending, data);
                }
                post(&WorkerMessage::Ready {
                    adapter: "WebGPU".to_string(),
                });
                start_render_loop(app_slot);
            });
        }
        ClientMessage::Resize { width, height } => {
            if let Some(app) = app_slot.borrow_mut().as_mut() {
                let physical_width = (width as u32).max(1);
                let physical_height = (height as u32).max(1);
                resize_offscreen(
                    &mut app.world,
                    &mut app.renderer,
                    physical_width,
                    physical_height,
                );
                app.world.resources.window.active_viewport_rect =
                    Some(nightshade::ecs::window::resources::ViewportRect {
                        x: 0.0,
                        y: 0.0,
                        width: physical_width as f32,
                        height: physical_height as f32,
                    });
            }
        }
        ClientMessage::LoadGlb { name } => {
            if let Some(app) = app_slot.borrow_mut().as_mut() {
                let Some(bytes) = glb_from(&data) else {
                    post(&WorkerMessage::ModelLoadFailed {
                        error: "No GLB payload in message".to_string(),
                    });
                    return;
                };
                systems::viewer::load_glb(
                    &mut app.state.viewer_world,
                    &mut app.world,
                    &name,
                    &bytes,
                );
            }
        }
        other => {
            if let Some(app) = app_slot.borrow_mut().as_mut() {
                apply_client_message(&mut app.world, &mut app.state, other);
            }
        }
    }
}

fn apply_client_message(world: &mut World, viewer: &mut Viewer, message: ClientMessage) {
    match message {
        ClientMessage::PointerMove { x, y } => {
            input_inject_cursor_moved(world, Vec2::new(x, y));
        }
        ClientMessage::PointerButton { button, pressed } => {
            let state = if pressed {
                KeyState::Pressed
            } else {
                KeyState::Released
            };
            input_inject_mouse_button(world, mouse_button(button), state);
        }
        ClientMessage::Wheel { delta } => {
            input_inject_mouse_wheel(world, Vec2::new(0.0, -delta / 100.0));
        }
        ClientMessage::Touch { id, phase, x, y } => {
            input_inject_touch(world, id, touch_phase(phase), Vec2::new(x, y));
        }
        ClientMessage::Key {
            code,
            pressed,
            text,
        } => {
            if let Some(key_code) = key_code_from_dom(&code) {
                let state = if pressed {
                    KeyState::Pressed
                } else {
                    KeyState::Released
                };
                input_inject_keyboard(world, key_code, state, text.as_deref());
            }
        }
        ClientMessage::Pick { x, y } => {
            systems::picking::request(&mut viewer.viewer_world, world, x, y);
        }
        ClientMessage::Animation { command } => {
            systems::viewer::apply_animation_command(&mut viewer.viewer_world, world, command);
        }
        ClientMessage::ClearModel => {
            systems::viewer::clear_model(&mut viewer.viewer_world, world);
        }
        ClientMessage::Init { .. }
        | ClientMessage::Resize { .. }
        | ClientMessage::LoadGlb { .. } => {}
    }
}

async fn create_app(canvas: OffscreenCanvas, width: f32, height: f32) -> App {
    let physical_width = (width as u32).max(1);
    let physical_height = (height as u32).max(1);

    let surface_target = wgpu::SurfaceTarget::OffscreenCanvas(canvas);
    let mut renderer = create_wgpu_renderer(surface_target, physical_width, physical_height)
        .await
        .expect("failed to create renderer from offscreen canvas");

    let mut world = World::default();
    let mut state = Viewer::default();
    initialize_offscreen(
        &mut world,
        &mut state,
        &mut renderer,
        (physical_width, physical_height),
        1.0,
    );
    world.resources.window.active_viewport_rect =
        Some(nightshade::ecs::window::resources::ViewportRect {
            x: 0.0,
            y: 0.0,
            width: physical_width as f32,
            height: physical_height as f32,
        });

    App {
        world,
        renderer,
        state,
    }
}

fn start_render_loop(app_slot: AppSlot) {
    let last_push = Rc::new(RefCell::new(0.0_f64));

    spawn_animation_frame_loop(move || {
        if let Some(app) = app_slot.borrow_mut().as_mut() {
            tick_offscreen(&mut app.world, &mut app.state, &mut app.renderer);
            let scope: DedicatedWorkerGlobalScope = js_sys::global().unchecked_into();
            if let Some(performance) = scope.performance() {
                let now = performance.now();
                let mut last = last_push.borrow_mut();
                if now - *last > 500.0 {
                    *last = now;
                    let entity_count = app
                        .world
                        .core
                        .query_entities(
                            nightshade::ecs::world::LOCAL_TRANSFORM
                                | nightshade::ecs::world::GLOBAL_TRANSFORM,
                        )
                        .count() as u32;
                    post(&WorkerMessage::Stats {
                        fps: app.world.resources.window.timing.frames_per_second,
                        entity_count,
                    });
                }
            }
        }
    });
}

fn mouse_button(button: u8) -> MouseButton {
    match button {
        1 => MouseButton::Middle,
        2 => MouseButton::Right,
        _ => MouseButton::Left,
    }
}

fn touch_phase(phase: protocol::TouchPhase) -> TouchPhase {
    match phase {
        protocol::TouchPhase::Started => TouchPhase::Started,
        protocol::TouchPhase::Moved => TouchPhase::Moved,
        protocol::TouchPhase::Ended => TouchPhase::Ended,
        protocol::TouchPhase::Cancelled => TouchPhase::Cancelled,
    }
}

fn key_code_from_dom(code: &str) -> Option<KeyCode> {
    Some(match code {
        "KeyA" => KeyCode::KeyA,
        "KeyB" => KeyCode::KeyB,
        "KeyC" => KeyCode::KeyC,
        "KeyD" => KeyCode::KeyD,
        "KeyE" => KeyCode::KeyE,
        "KeyF" => KeyCode::KeyF,
        "KeyG" => KeyCode::KeyG,
        "KeyH" => KeyCode::KeyH,
        "KeyI" => KeyCode::KeyI,
        "KeyJ" => KeyCode::KeyJ,
        "KeyK" => KeyCode::KeyK,
        "KeyL" => KeyCode::KeyL,
        "KeyM" => KeyCode::KeyM,
        "KeyN" => KeyCode::KeyN,
        "KeyO" => KeyCode::KeyO,
        "KeyP" => KeyCode::KeyP,
        "KeyQ" => KeyCode::KeyQ,
        "KeyR" => KeyCode::KeyR,
        "KeyS" => KeyCode::KeyS,
        "KeyT" => KeyCode::KeyT,
        "KeyU" => KeyCode::KeyU,
        "KeyV" => KeyCode::KeyV,
        "KeyW" => KeyCode::KeyW,
        "KeyX" => KeyCode::KeyX,
        "KeyY" => KeyCode::KeyY,
        "KeyZ" => KeyCode::KeyZ,
        "Digit0" => KeyCode::Digit0,
        "Digit1" => KeyCode::Digit1,
        "Digit2" => KeyCode::Digit2,
        "Digit3" => KeyCode::Digit3,
        "Digit4" => KeyCode::Digit4,
        "Digit5" => KeyCode::Digit5,
        "Digit6" => KeyCode::Digit6,
        "Digit7" => KeyCode::Digit7,
        "Digit8" => KeyCode::Digit8,
        "Digit9" => KeyCode::Digit9,
        "Escape" => KeyCode::Escape,
        "Enter" => KeyCode::Enter,
        "NumpadEnter" => KeyCode::NumpadEnter,
        "Tab" => KeyCode::Tab,
        "Space" => KeyCode::Space,
        "Delete" => KeyCode::Delete,
        "Backspace" => KeyCode::Backspace,
        "Home" => KeyCode::Home,
        "End" => KeyCode::End,
        "ArrowLeft" => KeyCode::ArrowLeft,
        "ArrowRight" => KeyCode::ArrowRight,
        "ArrowUp" => KeyCode::ArrowUp,
        "ArrowDown" => KeyCode::ArrowDown,
        "ShiftLeft" => KeyCode::ShiftLeft,
        "ShiftRight" => KeyCode::ShiftRight,
        "ControlLeft" => KeyCode::ControlLeft,
        "ControlRight" => KeyCode::ControlRight,
        "AltLeft" => KeyCode::AltLeft,
        "AltRight" => KeyCode::AltRight,
        "F1" => KeyCode::F1,
        "F2" => KeyCode::F2,
        "F3" => KeyCode::F3,
        "F4" => KeyCode::F4,
        "F5" => KeyCode::F5,
        "F6" => KeyCode::F6,
        "F7" => KeyCode::F7,
        "F8" => KeyCode::F8,
        "F9" => KeyCode::F9,
        "F10" => KeyCode::F10,
        "F11" => KeyCode::F11,
        "F12" => KeyCode::F12,
        "Comma" => KeyCode::Comma,
        "Period" => KeyCode::Period,
        "Minus" => KeyCode::Minus,
        "Equal" => KeyCode::Equal,
        _ => return None,
    })
}

fn canvas_from(data: &JsValue) -> Option<OffscreenCanvas> {
    js_sys::Reflect::get(data, &JsValue::from_str(CANVAS_KEY))
        .ok()
        .and_then(|value| value.dyn_into::<OffscreenCanvas>().ok())
}

fn glb_from(data: &JsValue) -> Option<Vec<u8>> {
    js_sys::Reflect::get(data, &JsValue::from_str(GLB_KEY))
        .ok()
        .and_then(|value| value.dyn_into::<js_sys::ArrayBuffer>().ok())
        .map(|buffer| js_sys::Uint8Array::new(&buffer).to_vec())
}

pub(crate) fn post(message: &WorkerMessage) {
    let scope: DedicatedWorkerGlobalScope = js_sys::global().unchecked_into();
    if let Ok(value) = serde_wasm_bindgen::to_value(message) {
        let envelope = js_sys::Object::new();
        if js_sys::Reflect::set(&envelope, &JsValue::from_str(MESSAGE_KEY), &value).is_ok() {
            drop(scope.post_message(&envelope));
        }
    }
}
