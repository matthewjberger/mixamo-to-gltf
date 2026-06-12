use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

use protocol::{AnimationCommand, ClientMessage};

use crate::api;
use crate::bridge::{Bridge, send};
use crate::components::dropzone::DropOverlay;
use crate::components::loader::Loader;
use crate::components::panel::Panel;
use crate::components::playbar::PlayBar;
use crate::components::viewport::Viewport;
use crate::state::AppState;

/// Application root: owns the shared state and bridge slot, forwards keyboard
/// input to the worker, accepts dropped Mixamo zips, and composes the
/// viewport and overlays. Falls back to a notice when the browser has no
/// WebGPU.
#[component]
pub fn App() -> impl IntoView {
    if !webgpu_supported() {
        return unsupported().into_any();
    }

    let state = AppState::new();
    let bridge = StoredValue::new_local(None::<Bridge>);

    let _ = window_event_listener(leptos::ev::keydown, move |event| {
        if typing_in_field(&event) {
            return;
        }
        if event.code() == "Space" && state.clips.with_untracked(|clips| !clips.is_empty()) {
            event.prevent_default();
            if let Some(bridge) = bridge.get_value() {
                let command = if state.anim_playing.get_untracked() {
                    AnimationCommand::Pause
                } else if state.anim_current.get_untracked().is_some() {
                    AnimationCommand::Resume
                } else {
                    AnimationCommand::Play { index: 0 }
                };
                send(&bridge, &ClientMessage::Animation { command });
            }
            return;
        }
        if let Some(bridge) = bridge.get_value() {
            let text = (event.key().chars().count() == 1).then(|| event.key());
            send(
                &bridge,
                &ClientMessage::Key {
                    code: event.code(),
                    pressed: true,
                    text,
                },
            );
        }
    });

    let _ = window_event_listener(leptos::ev::keyup, move |event| {
        if typing_in_field(&event) {
            return;
        }
        if let Some(bridge) = bridge.get_value() {
            send(
                &bridge,
                &ClientMessage::Key {
                    code: event.code(),
                    pressed: false,
                    text: None,
                },
            );
        }
    });

    let _ = window_event_listener(leptos::ev::dragover, move |event| {
        event.prevent_default();
        state.drag_over.set(true);
    });

    let _ = window_event_listener(leptos::ev::dragleave, move |event| {
        if event.related_target().is_none() {
            state.drag_over.set(false);
        }
    });

    let _ = window_event_listener(leptos::ev::drop, move |event| {
        event.prevent_default();
        state.drag_over.set(false);
        let Some(transfer) = event.data_transfer() else {
            return;
        };
        let Some(files) = transfer.files() else {
            return;
        };
        let Some(file) = files.get(0) else {
            return;
        };
        import_file(state, bridge, file);
    });

    view! {
        <div class="app-shell">
            <Viewport bridge state />
            <Panel bridge state />
            <PlayBar bridge state />
            <DropOverlay state />
            <Loader state />
        </div>
    }
    .into_any()
}

/// Reads a dropped or picked file and feeds it into the conversion flow.
pub fn import_file(
    state: AppState,
    bridge: StoredValue<Option<Bridge>, LocalStorage>,
    file: web_sys::File,
) {
    let name = file.name();
    if !name.to_lowercase().ends_with(".zip") {
        state
            .error
            .set(Some(format!("'{name}' is not a .zip archive")));
        return;
    }
    let promise = file.array_buffer();
    spawn_local(async move {
        match JsFuture::from(promise).await {
            Ok(value) => {
                let buffer = js_sys::ArrayBuffer::from(value);
                api::import_zip(state, bridge, name, buffer).await;
            }
            Err(_) => state.error.set(Some(format!("Failed to read '{name}'"))),
        }
    });
}

fn unsupported() -> impl IntoView {
    view! {
        <div class="unsupported">
            <div class="unsupported-card">
                <h1>"WebGPU not available"</h1>
                <p>
                    "This app runs the Nightshade engine in a web worker through WebGPU. Open it in a browser with WebGPU and OffscreenCanvas-in-workers support (Chromium 113+, Firefox 141+)."
                </p>
            </div>
        </div>
    }
}

fn typing_in_field(event: &web_sys::KeyboardEvent) -> bool {
    event
        .target()
        .and_then(|target| target.dyn_into::<web_sys::HtmlElement>().ok())
        .map(|element| {
            let tag = element.tag_name();
            tag.eq_ignore_ascii_case("input")
                || tag.eq_ignore_ascii_case("textarea")
                || tag.eq_ignore_ascii_case("select")
                || element.is_content_editable()
        })
        .unwrap_or(false)
}

fn webgpu_supported() -> bool {
    let Some(window) = web_sys::window() else {
        return false;
    };
    let Ok(navigator) = js_sys::Reflect::get(window.as_ref(), &JsValue::from_str("navigator"))
    else {
        return false;
    };
    js_sys::Reflect::get(&navigator, &JsValue::from_str("gpu"))
        .map(|gpu| !gpu.is_undefined() && !gpu.is_null())
        .unwrap_or(false)
}
