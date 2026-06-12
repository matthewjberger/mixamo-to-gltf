use leptos::prelude::*;
use protocol::{AnimationCommand, ClientMessage};

use crate::bridge::{Bridge, send};
use crate::state::AppState;

/// Floating playback controls for the loaded GLB: clip selection, transport,
/// timeline scrubbing, speed, and looping.
#[component]
pub fn PlayBar(
    bridge: StoredValue<Option<Bridge>, LocalStorage>,
    state: AppState,
) -> impl IntoView {
    let send_command = move |command: AnimationCommand| {
        if let Some(bridge) = bridge.get_value() {
            send(&bridge, &ClientMessage::Animation { command });
        }
    };

    let on_clip_change = move |event: web_sys::Event| {
        if let Ok(index) = event_target_value(&event).parse::<u32>() {
            send_command(AnimationCommand::Play { index });
        }
    };

    let on_toggle_play = move |_| {
        if state.anim_playing.get_untracked() {
            send_command(AnimationCommand::Pause);
        } else if state.anim_current.get_untracked().is_some() {
            send_command(AnimationCommand::Resume);
        } else {
            send_command(AnimationCommand::Play { index: 0 });
        }
    };

    let on_stop = move |_| send_command(AnimationCommand::Stop);
    let on_play_all = move |_| send_command(AnimationCommand::PlayAll);

    let on_scrub_input = move |event: web_sys::Event| {
        if let Ok(time) = event_target_value(&event).parse::<f32>() {
            state.anim_time.set(time);
            send_command(AnimationCommand::Seek { time });
        }
    };

    let on_speed_change = move |event: web_sys::Event| {
        if let Ok(speed) = event_target_value(&event).parse::<f32>() {
            send_command(AnimationCommand::SetSpeed { speed });
        }
    };

    let on_loop_change = move |event: web_sys::Event| {
        send_command(AnimationCommand::SetLooping {
            looping: event_target_checked(&event),
        });
    };

    let speed_value = move || {
        let speed = state.anim_speed.get();
        if (speed - speed.round()).abs() < 0.001 {
            format!("{}", speed.round() as i32)
        } else {
            format!("{speed}")
        }
    };

    view! {
        <Show
            when=move || state.clips.with(|clips| !clips.is_empty())
            fallback=|| ()
        >
            <div class="playbar">
                <select
                    class="playbar-select"
                    prop:value=move || {
                        state
                            .anim_current
                            .get()
                            .map(|index| index.to_string())
                            .unwrap_or_default()
                    }
                    on:change=on_clip_change
                >
                    <For
                        each=move || {
                            state.clips.get().into_iter().enumerate().collect::<Vec<_>>()
                        }
                        key=|(index, clip)| (*index, clip.name.clone())
                        children=move |(index, clip)| {
                            view! {
                                <option value=index.to_string()>
                                    {format!("{} ({:.1}s)", clip.name, clip.duration)}
                                </option>
                            }
                        }
                    />
                </select>

                <button class="playbar-button" title="Play all clips in sequence" on:click=on_play_all>
                    "⏩"
                </button>
                <button class="playbar-button playbar-toggle" on:click=on_toggle_play>
                    {move || if state.anim_playing.get() { "⏸" } else { "▶" }}
                </button>
                <button class="playbar-button" title="Stop" on:click=on_stop>
                    "⏹"
                </button>

                <input
                    type="range"
                    class="playbar-scrubber"
                    min="0"
                    step="0.01"
                    max=move || format!("{:.3}", state.anim_duration.get().max(0.001))
                    prop:value=move || format!("{:.3}", state.anim_time.get())
                    on:pointerdown=move |_| state.scrubbing.set(true)
                    on:input=on_scrub_input
                    on:pointerup=move |_| state.scrubbing.set(false)
                    on:pointercancel=move |_| state.scrubbing.set(false)
                />

                <span class="playbar-time">
                    {move || {
                        format!("{:.2} / {:.2}s", state.anim_time.get(), state.anim_duration.get())
                    }}
                </span>

                <select class="playbar-select playbar-speed" prop:value=speed_value on:change=on_speed_change>
                    <option value="0.25">"0.25×"</option>
                    <option value="0.5">"0.5×"</option>
                    <option value="0.75">"0.75×"</option>
                    <option value="1">"1×"</option>
                    <option value="1.5">"1.5×"</option>
                    <option value="2">"2×"</option>
                </select>

                <label class="playbar-loop">
                    <input
                        type="checkbox"
                        prop:checked=move || state.anim_looping.get()
                        on:change=on_loop_change
                    />
                    "Loop"
                </label>
            </div>
        </Show>
    }
}
