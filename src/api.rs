//! Conversion flow: talks to the native `/api/*` endpoints served by the
//! desktop shell, updates the page state, and hands built GLBs to the worker.

use leptos::prelude::*;
use protocol::{BuildRequest, BundleSummary, SaveRequest, SaveResponse};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{RequestInit, Response};

use crate::bridge::{Bridge, send_glb};
use crate::state::AppState;

async fn post(url: &str, body: &JsValue) -> Result<Response, String> {
    let window = web_sys::window().ok_or("no window")?;
    let options = RequestInit::new();
    options.set_method("POST");
    options.set_body(body);
    let response = JsFuture::from(window.fetch_with_str_and_init(url, &options))
        .await
        .map_err(|_| {
            "Conversion service unreachable. Run the desktop app with `just run`.".to_string()
        })?;
    let response: Response = response
        .dyn_into()
        .map_err(|_| "Unexpected fetch response".to_string())?;
    if !response.ok() {
        let text = match response.text() {
            Ok(promise) => JsFuture::from(promise)
                .await
                .ok()
                .and_then(|value| value.as_string())
                .unwrap_or_default(),
            Err(_) => String::new(),
        };
        return Err(if text.is_empty() {
            format!("Request failed ({})", response.status())
        } else {
            text
        });
    }
    Ok(response)
}

async fn response_text(response: Response) -> Result<String, String> {
    let promise = response.text().map_err(|_| "Failed to read response")?;
    JsFuture::from(promise)
        .await
        .map_err(|_| "Failed to read response".to_string())?
        .as_string()
        .ok_or_else(|| "Failed to read response".to_string())
}

/// Uploads the dropped zip, stores the parsed bundle summary, then converts
/// and previews it with every animation selected.
pub async fn import_zip(
    state: AppState,
    bridge: StoredValue<Option<Bridge>, LocalStorage>,
    name: String,
    buffer: js_sys::ArrayBuffer,
) {
    state.error.set(None);
    state.busy.set(Some(format!("Importing {name}…")));
    state.status.set(format!("Importing {name}…"));

    let encoded_name = js_sys::encode_uri_component(&name);
    let url = format!("/api/import?name={encoded_name}");
    let result = post(&url, buffer.as_ref()).await;

    let summary: BundleSummary = match result {
        Ok(response) => match response_text(response).await {
            Ok(text) => match serde_json::from_str(&text) {
                Ok(summary) => summary,
                Err(error) => {
                    fail(state, format!("Bad import response: {error}"));
                    return;
                }
            },
            Err(error) => {
                fail(state, error);
                return;
            }
        },
        Err(error) => {
            fail(state, error);
            return;
        }
    };

    state.log.update(|log| log.extend(summary.log.clone()));
    state
        .animation_selected
        .set(vec![true; summary.animations.len()]);
    state.model_index.set(0);
    state.glb_size.set(None);
    state.status.set(format!(
        "Imported {}: {} models, {} animations",
        summary.name,
        summary.models.len(),
        summary.animations.len()
    ));
    state.bundle.set(Some(summary));

    build_and_preview(state, bridge).await;
}

/// Builds a GLB from the current selection and loads it straight into the
/// viewer so the export is verified visually.
pub async fn build_and_preview(state: AppState, bridge: StoredValue<Option<Bridge>, LocalStorage>) {
    let Some(bundle) = state.bundle.get_untracked() else {
        return;
    };
    let model_index = state.model_index.get_untracked();
    let animation_indices: Vec<u32> = state
        .animation_selected
        .get_untracked()
        .iter()
        .enumerate()
        .filter_map(|(index, &selected)| selected.then_some(index as u32))
        .collect();

    state.error.set(None);
    state.busy.set(Some("Converting to GLB…".to_string()));
    state.status.set(format!(
        "Converting with {} animations…",
        animation_indices.len()
    ));

    let request = BuildRequest {
        model_index: model_index as u32,
        animation_indices,
        strip_root_motion: state.strip_root_motion.get_untracked(),
    };
    let body = JsValue::from_str(&serde_json::to_string(&request).expect("request serializes"));

    let response = match post("/api/build", &body).await {
        Ok(response) => response,
        Err(error) => {
            fail(state, error);
            return;
        }
    };

    let buffer = match response.array_buffer() {
        Ok(promise) => match JsFuture::from(promise).await {
            Ok(value) => js_sys::ArrayBuffer::from(value),
            Err(_) => {
                fail(state, "Failed to read GLB bytes".to_string());
                return;
            }
        },
        Err(_) => {
            fail(state, "Failed to read GLB bytes".to_string());
            return;
        }
    };

    let size = buffer.byte_length() as usize;
    state.glb_size.set(Some(size));

    let model_name = bundle
        .models
        .get(model_index)
        .map(|model| model.name.clone())
        .unwrap_or_else(|| "export".to_string());

    state.log.update(|log| {
        log.push(format!(
            "Built {}.glb ({:.2} MB)",
            model_name,
            size as f64 / (1024.0 * 1024.0)
        ));
    });

    if let Some(bridge) = bridge.get_value() {
        send_glb(&bridge, &format!("{model_name}.glb"), &buffer);
        state.status.set("Loading GLB into the viewer…".to_string());
    }

    state.busy.set(None);
}

/// Asks the native shell to save the last built GLB through a file dialog.
pub async fn save_glb(state: AppState) {
    let Some(bundle) = state.bundle.get_untracked() else {
        return;
    };
    let model_name = bundle
        .models
        .get(state.model_index.get_untracked())
        .map(|model| model.name.clone())
        .unwrap_or_else(|| "export".to_string());

    state.error.set(None);
    state.busy.set(Some("Saving GLB…".to_string()));

    let request = SaveRequest {
        file_name: format!("{model_name}.glb"),
    };
    let body = JsValue::from_str(&serde_json::to_string(&request).expect("request serializes"));

    match post("/api/save", &body).await {
        Ok(response) => match response_text(response).await {
            Ok(text) => match serde_json::from_str::<SaveResponse>(&text) {
                Ok(SaveResponse {
                    saved_path: Some(path),
                }) => {
                    state.status.set(format!("Saved to {path}"));
                    state.log.update(|log| log.push(format!("Saved to {path}")));
                }
                Ok(SaveResponse { saved_path: None }) => {
                    state.status.set("Save cancelled".to_string());
                }
                Err(error) => fail(state, format!("Bad save response: {error}")),
            },
            Err(error) => fail(state, error),
        },
        Err(error) => fail(state, error),
    }

    state.busy.set(None);
}

fn fail(state: AppState, error: String) {
    state.error.set(Some(error.clone()));
    state.status.set("Failed".to_string());
    state.log.update(|log| log.push(format!("ERROR: {error}")));
    state.busy.set(None);
}
