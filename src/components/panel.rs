use leptos::html;
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api;
use crate::app::import_file;
use crate::bridge::Bridge;
use crate::state::AppState;

/// The control panel: import, model and animation selection, conversion, and
/// saving, with a status line and conversion log.
#[component]
pub fn Panel(bridge: StoredValue<Option<Bridge>, LocalStorage>, state: AppState) -> impl IntoView {
    let file_input = NodeRef::<html::Input>::new();

    let on_browse = move |_| {
        if let Some(input) = file_input.get() {
            input.click();
        }
    };

    let on_file_change = move |_| {
        if let Some(input) = file_input.get() {
            if let Some(file) = input.files().and_then(|files| files.get(0)) {
                import_file(state, bridge, file);
            }
            input.set_value("");
        }
    };

    let selected_count = move || {
        state
            .animation_selected
            .with(|selected| selected.iter().filter(|on| **on).count())
    };

    let set_all = move |value: bool| {
        state
            .animation_selected
            .update(|selected| selected.iter_mut().for_each(|slot| *slot = value));
    };

    let on_convert = move |_| {
        spawn_local(api::build_and_preview(state, bridge));
    };

    let on_save = move |_| {
        spawn_local(api::save_glb(state));
    };

    let has_bundle = move || state.bundle.with(|bundle| bundle.is_some());
    let is_busy = move || state.busy.with(|busy| busy.is_some());

    view! {
        <div class="panel">
            <input
                type="file"
                accept=".zip"
                class="hidden-input"
                node_ref=file_input
                on:change=on_file_change
            />

            <div class="panel-header">
                <div class="panel-title">"Mixamo " <span class="panel-arrow">"→"</span> " glTF"</div>
                <div class="panel-subtitle">
                    "Drop a Mixamo bundle, pick animations, get a single GLB"
                </div>
            </div>

            <div class="panel-body">
                <Show when=move || !has_bundle()>
                    <div class="drop-card" on:click=on_browse>
                        <div class="drop-card-icon">"📦"</div>
                        <div class="drop-card-title">"Drop a Mixamo .zip here"</div>
                        <div class="drop-card-hint">
                            "A character FBX plus its animation FBX files"
                        </div>
                        <button class="button button-primary">"Browse…"</button>
                    </div>
                </Show>

                <Show when=has_bundle>
                    <div class="section">
                        <div class="section-header">
                            <span class="section-title">"Bundle"</span>
                            <button class="button button-ghost" on:click=on_browse>
                                "Import another…"
                            </button>
                        </div>
                        <div class="bundle-name">
                            {move || state.bundle.with(|bundle| {
                                bundle.as_ref().map(|bundle| bundle.name.clone()).unwrap_or_default()
                            })}
                        </div>
                    </div>

                    <div class="section">
                        <div class="section-header">
                            <span class="section-title">"Character"</span>
                        </div>
                        <For
                            each=move || {
                                state
                                    .bundle
                                    .get()
                                    .map(|bundle| bundle.models.into_iter().enumerate().collect::<Vec<_>>())
                                    .unwrap_or_default()
                            }
                            key=|(index, model)| (*index, model.name.clone())
                            children=move |(index, model)| {
                                let is_active = move || state.model_index.get() == index;
                                let on_select = move |_| state.model_index.set(index);
                                view! {
                                    <div
                                        class=move || {
                                            if is_active() { "model-row model-row-active" } else { "model-row" }
                                        }
                                        on:click=on_select
                                    >
                                        <div class="model-name">{model.name.clone()}</div>
                                        <div class="model-meta">
                                            {format!(
                                                "{} meshes · {} skins · {} textures · {} nodes",
                                                model.mesh_count,
                                                model.skin_count,
                                                model.texture_count,
                                                model.node_count,
                                            )}
                                        </div>
                                    </div>
                                }
                            }
                        />
                    </div>

                    <div class="section section-grow">
                        <div class="section-header">
                            <span class="section-title">
                                {move || {
                                    let total = state
                                        .bundle
                                        .with(|bundle| {
                                            bundle.as_ref().map(|bundle| bundle.animations.len()).unwrap_or(0)
                                        });
                                    format!("Animations ({}/{})", selected_count(), total)
                                }}
                            </span>
                            <span class="section-actions">
                                <button class="button button-ghost" on:click=move |_| set_all(true)>
                                    "All"
                                </button>
                                <button class="button button-ghost" on:click=move |_| set_all(false)>
                                    "None"
                                </button>
                            </span>
                        </div>
                        <label class="check-row strip-row">
                            <input
                                type="checkbox"
                                prop:checked=move || state.strip_root_motion.get()
                                on:change=move |event| {
                                    state.strip_root_motion.set(event_target_checked(&event));
                                }
                            />
                            <span>"Strip root motion"</span>
                        </label>
                        <div class="anim-list">
                            <For
                                each=move || {
                                    state
                                        .bundle
                                        .get()
                                        .map(|bundle| {
                                            bundle.animations.into_iter().enumerate().collect::<Vec<_>>()
                                        })
                                        .unwrap_or_default()
                                }
                                key=|(index, animation)| (*index, animation.name.clone())
                                children=move |(index, animation)| {
                                    let checked = move || {
                                        state
                                            .animation_selected
                                            .with(|selected| selected.get(index).copied().unwrap_or(false))
                                    };
                                    let on_toggle = move |event: web_sys::Event| {
                                        let value = event_target_checked(&event);
                                        state
                                            .animation_selected
                                            .update(|selected| {
                                                if let Some(slot) = selected.get_mut(index) {
                                                    *slot = value;
                                                }
                                            });
                                    };
                                    view! {
                                        <label class="check-row anim-row">
                                            <input type="checkbox" prop:checked=checked on:change=on_toggle />
                                            <span class="anim-name">{animation.name.clone()}</span>
                                            <span class="anim-duration">
                                                {format!("{:.1}s", animation.duration)}
                                            </span>
                                        </label>
                                    }
                                }
                            />
                        </div>
                    </div>

                    <div class="section panel-actions">
                        <button
                            class="button button-primary"
                            disabled=is_busy
                            on:click=on_convert
                        >
                            {move || {
                                if is_busy() { "Working…".to_string() }
                                else { format!("Convert & Preview ({})", selected_count()) }
                            }}
                        </button>
                        <button
                            class="button"
                            disabled=move || is_busy() || state.glb_size.with(|size| size.is_none())
                            on:click=on_save
                        >
                            {move || match state.glb_size.get() {
                                Some(size) => {
                                    format!("Save GLB… ({:.2} MB)", size as f64 / (1024.0 * 1024.0))
                                }
                                None => "Save GLB…".to_string(),
                            }}
                        </button>
                    </div>
                </Show>
            </div>

            <div class="panel-status">
                <Show when=is_busy>
                    <span class="spinner spinner-small"></span>
                </Show>
                <span class="status-text">
                    {move || match state.error.get() {
                        Some(error) => error,
                        None => state.status.get(),
                    }}
                </span>
            </div>

            <Show when=move || state.log.with(|log| !log.is_empty())>
                <details class="panel-log">
                    <summary>
                        {move || format!("Log ({})", state.log.with(|log| log.len()))}
                    </summary>
                    <div class="log-lines">
                        {move || {
                            state
                                .log
                                .get()
                                .into_iter()
                                .map(|line| view! { <div class="log-line">{line}</div> })
                                .collect_view()
                        }}
                    </div>
                </details>
            </Show>

            <div class="panel-footer">
                <span>{move || state.adapter.get()}</span>
                <span>{move || format!("{:.0} FPS", state.fps.get())}</span>
                <span>{move || format!("{} entities", state.entity_count.get())}</span>
            </div>
        </div>
    }
}
