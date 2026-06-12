#[cfg(target_arch = "wasm32")]
fn main() {
    console_error_panic_hook::set_once();
    leptos::prelude::mount_to_body(page::App);
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!("the page is a web app and only runs in the browser.");
    eprintln!("Serve it with `just run-web`, or run the desktop shell instead:");
    eprintln!("    just run");
    std::process::exit(1);
}
