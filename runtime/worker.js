// Bootstraps the Rust web worker. The wasm module's #[wasm_bindgen(start)] entry
// installs the real onmessage handler, but that only runs once init() resolves.
// The worker's message queue is enabled as soon as this script finishes, so an
// Init posted by the page before the wasm is ready would be dropped. Register a
// synchronous handler that buffers early messages, then replay them once the
// wasm has taken over.
import init from "./engine.js";

const buffered = [];
self.onmessage = (event) => buffered.push(event);

init().then(() => {
  for (const event of buffered) {
    self.onmessage(event);
  }
});
