//! The Leptos UI on the main thread.
//!
//! ## Architecture
//!
//! - `src/app.rs` composes the components, forwards keyboard input, and
//!   accepts dropped Mixamo zip bundles.
//! - `src/api.rs` drives the conversion flow against the desktop shell's
//!   native `/api/*` endpoints (import zip, build GLB, save GLB).
//! - `src/bridge.rs` spawns the worker and converts `WorkerMessage`s into
//!   signal writes, and `ClientMessage`s into `postMessage` envelopes
//!   (including the transferred GLB buffer).
//! - `src/state.rs` is all page state, grouped as `Copy` signals.
//! - `src/components/` holds the components: the viewport canvas, the
//!   conversion panel, the animation playback bar, and the overlays.
mod api;
mod app;
mod bridge;
mod components;
mod state;

pub use app::App;
