use leptos::prelude::*;

use crate::state::AppState;

/// Full-screen highlight shown while a file is dragged over the window.
#[component]
pub fn DropOverlay(state: AppState) -> impl IntoView {
    view! {
        <Show when=move || state.drag_over.get() fallback=|| ()>
            <div class="drop-overlay">
                <div class="drop-overlay-card">
                    <div class="drop-overlay-icon">"⬇"</div>
                    "Drop the Mixamo .zip to convert"
                </div>
            </div>
        </Show>
    }
}
