//! Standalone shell: hosts the same web bundle the browser runs, served from
//! a local port into a native webview window, and exposes the native
//! conversion pipeline (FBX parsing via ufbx, GLB building, save dialogs)
//! under `/api/*` for the page. Debug builds read `../dist` from disk so a
//! fresh `trunk build` shows up on relaunch; release builds embed the bundle
//! into the executable.

use std::sync::Mutex;

use protocol::{BuildRequest, SaveRequest, SaveResponse};
use rust_embed::RustEmbed;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};
use wry::{WebView, WebViewBuilder};

#[derive(RustEmbed)]
#[folder = "../dist"]
struct Dist;

#[derive(Default)]
struct ConverterState {
    bundle: Mutex<Option<convert::Bundle>>,
    built_glb: Mutex<Option<(String, Vec<u8>)>>,
}

fn content_type(path: &str) -> &'static str {
    let extension = path.rsplit('.').next().unwrap_or_default();
    match extension {
        "html" => "text/html; charset=utf-8",
        "js" => "application/javascript",
        "wasm" => "application/wasm",
        "css" => "text/css",
        "png" => "image/png",
        "svg" => "image/svg+xml",
        "json" => "application/json",
        _ => "application/octet-stream",
    }
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 2 < bytes.len() => {
                let high = (bytes[index + 1] as char).to_digit(16);
                let low = (bytes[index + 2] as char).to_digit(16);
                if let (Some(high), Some(low)) = (high, low) {
                    decoded.push((high * 16 + low) as u8);
                    index += 3;
                } else {
                    decoded.push(bytes[index]);
                    index += 1;
                }
            }
            b'+' => {
                decoded.push(b' ');
                index += 1;
            }
            byte => {
                decoded.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

fn respond_text(request: tiny_http::Request, status: u16, body: &str) {
    let response = tiny_http::Response::from_string(body).with_status_code(status);
    let _ = request.respond(response);
}

fn respond_json(request: tiny_http::Request, body: String) {
    let header = tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
        .expect("static header is valid");
    let response = tiny_http::Response::from_string(body).with_header(header);
    let _ = request.respond(response);
}

fn respond_bytes(request: tiny_http::Request, body: Vec<u8>) {
    let header =
        tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/octet-stream"[..])
            .expect("static header is valid");
    let response = tiny_http::Response::from_data(body).with_header(header);
    let _ = request.respond(response);
}

fn handle_import(mut request: tiny_http::Request, state: &ConverterState) {
    let name = request
        .url()
        .split_once('?')
        .and_then(|(_, query)| {
            query
                .split('&')
                .find_map(|pair| pair.strip_prefix("name=").map(percent_decode))
        })
        .unwrap_or_else(|| "bundle.zip".to_string());

    let mut body = Vec::new();
    if let Err(error) = request.as_reader().read_to_end(&mut body) {
        respond_text(request, 400, &format!("Failed to read upload: {error}"));
        return;
    }

    match convert::import_bundle(&name, &body) {
        Ok((bundle, summary)) => {
            *state.bundle.lock().expect("bundle lock") = Some(bundle);
            *state.built_glb.lock().expect("glb lock") = None;
            match serde_json::to_string(&summary) {
                Ok(json) => respond_json(request, json),
                Err(error) => respond_text(request, 500, &format!("{error}")),
            }
        }
        Err(error) => respond_text(request, 400, &error),
    }
}

fn handle_build(mut request: tiny_http::Request, state: &ConverterState) {
    let mut body = String::new();
    if let Err(error) = request.as_reader().read_to_string(&mut body) {
        respond_text(request, 400, &format!("Failed to read request: {error}"));
        return;
    }
    let build_request: BuildRequest = match serde_json::from_str(&body) {
        Ok(value) => value,
        Err(error) => {
            respond_text(request, 400, &format!("Invalid build request: {error}"));
            return;
        }
    };

    let bundle_guard = state.bundle.lock().expect("bundle lock");
    let Some(bundle) = bundle_guard.as_ref() else {
        respond_text(request, 400, "No bundle imported yet");
        return;
    };

    let animation_indices: Vec<usize> = build_request
        .animation_indices
        .iter()
        .map(|&index| index as usize)
        .collect();

    match convert::build_glb(
        bundle,
        build_request.model_index as usize,
        &animation_indices,
        build_request.strip_root_motion,
    ) {
        Ok((glb, _log)) => {
            let file_name = bundle
                .models
                .get(build_request.model_index as usize)
                .map(|model| format!("{}.glb", model.name))
                .unwrap_or_else(|| "export.glb".to_string());
            *state.built_glb.lock().expect("glb lock") = Some((file_name, glb.clone()));
            respond_bytes(request, glb);
        }
        Err(error) => respond_text(request, 400, &error),
    }
}

fn handle_save(mut request: tiny_http::Request, state: &ConverterState) {
    let mut body = String::new();
    if let Err(error) = request.as_reader().read_to_string(&mut body) {
        respond_text(request, 400, &format!("Failed to read request: {error}"));
        return;
    }
    let save_request: SaveRequest = match serde_json::from_str(&body) {
        Ok(value) => value,
        Err(error) => {
            respond_text(request, 400, &format!("Invalid save request: {error}"));
            return;
        }
    };

    let built = state.built_glb.lock().expect("glb lock").clone();
    let Some((default_name, glb)) = built else {
        respond_text(request, 400, "No GLB built yet");
        return;
    };

    let file_name = if save_request.file_name.is_empty() {
        default_name
    } else {
        save_request.file_name
    };

    let picked = rfd::FileDialog::new()
        .add_filter("GLB", &["glb"])
        .set_file_name(&file_name)
        .save_file();

    match picked {
        Some(path) => {
            if let Err(error) = std::fs::write(&path, &glb) {
                respond_text(request, 500, &format!("Failed to write file: {error}"));
                return;
            }
            let response = SaveResponse {
                saved_path: Some(path.display().to_string()),
            };
            respond_json(
                request,
                serde_json::to_string(&response).expect("save response serializes"),
            );
        }
        None => {
            let response = SaveResponse { saved_path: None };
            respond_json(
                request,
                serde_json::to_string(&response).expect("save response serializes"),
            );
        }
    }
}

fn handle_request(request: tiny_http::Request, state: &ConverterState) {
    let path = request.url().split('?').next().unwrap_or("/").to_string();

    if request.method() == &tiny_http::Method::Post {
        match path.as_str() {
            "/api/import" => handle_import(request, state),
            "/api/build" => handle_build(request, state),
            "/api/save" => handle_save(request, state),
            _ => respond_text(request, 404, "Unknown endpoint"),
        }
        return;
    }

    let path = path.trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };
    match Dist::get(path) {
        Some(file) => {
            let header =
                tiny_http::Header::from_bytes(&b"Content-Type"[..], content_type(path).as_bytes())
                    .expect("static header is valid");
            let response =
                tiny_http::Response::from_data(file.data.into_owned()).with_header(header);
            let _ = request.respond(response);
        }
        None => {
            let _ = request.respond(tiny_http::Response::empty(404));
        }
    }
}

/// Serves the bundle and the conversion API on an ephemeral localhost port
/// from a background thread and returns the port. Localhost is a secure
/// context, so WebGPU and module workers behave exactly as they do in a
/// browser tab.
fn serve_dist() -> u16 {
    let server = tiny_http::Server::http("127.0.0.1:0").expect("failed to bind localhost");
    let port = server
        .server_addr()
        .to_ip()
        .expect("expected an ip address")
        .port();
    std::thread::spawn(move || {
        let state = ConverterState::default();
        for request in server.incoming_requests() {
            handle_request(request, &state);
        }
    });
    port
}

struct App {
    port: u16,
    window: Option<Window>,
    webview: Option<WebView>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attributes = Window::default_attributes()
            .with_title("Mixamo to glTF")
            .with_maximized(true);
        let window = event_loop
            .create_window(attributes)
            .expect("failed to create window");

        let builder = WebViewBuilder::new()
            .with_url(format!("http://127.0.0.1:{}/", self.port))
            .with_navigation_handler(|url| {
                url.starts_with("http://127.0.0.1") || url.starts_with("https://127.0.0.1")
            });
        #[cfg(target_os = "windows")]
        let builder = {
            use wry::WebViewBuilderExtWindows;
            builder.with_additional_browser_args("--enable-features=WebGPU")
        };
        let webview = builder.build(&window).expect("failed to create webview");

        self.window = Some(window);
        self.webview = Some(webview);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        if let WindowEvent::CloseRequested = event {
            event_loop.exit();
        }
    }
}

fn main() {
    if Dist::get("index.html").is_none() {
        eprintln!("the web bundle is missing, build it first with `just dist`");
        std::process::exit(1);
    }
    let port = serve_dist();
    let event_loop = EventLoop::new().expect("failed to create event loop");
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App {
        port,
        window: None,
        webview: None,
    };
    event_loop.run_app(&mut app).expect("event loop failed");
}
