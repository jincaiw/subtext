//! The application's HTTP client.
//!
//! `gpui_kit::application()` installs a client only on the web: on macOS,
//! Windows and Linux it builds the application with `NullHttpClient`, whose
//! `send` fails with "No HttpClient available". Every request the application
//! makes — the update check today, and anything GPUI Kit itself fetches from a
//! URL — reaches that client unless the application replaces it.
//!
//! Installing one here, before the first window opens, is what makes the
//! updater work at all. [`reqwest_client::ReqwestClient`] is the client GPUI
//! Kit's own examples use: Zed's reqwest fork, a tokio runtime it owns, and
//! rustls against the platform's certificate store.

use std::sync::Arc;

use gpui_kit::App;
use gpui_kit::http_client::HttpClient;
use reqwest_client::ReqwestClient;

/// Give `cx` a working HTTP client for the rest of the process.
///
/// A failure leaves the platform's null client in place and is logged rather
/// than fatal: an editor without network access still edits files, and the
/// update check reports its own error when it needs one.
pub fn install(cx: &mut App) {
    let agent = format!("SubText/{}", env!("CARGO_PKG_VERSION"));
    match ReqwestClient::user_agent(&agent) {
        Ok(client) => cx.set_http_client(Arc::new(client) as Arc<dyn HttpClient>),
        Err(error) => log::error!("could not install the HTTP client: {error:#}"),
    }
}
