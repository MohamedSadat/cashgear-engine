//! The web page around the game: events the page can listen for, so its own HTML (a
//! loading screen, an error panel) can follow the game. Off the web these do nothing.
//!
//! | Event on `window` | When | `detail` |
//! | --- | --- | --- |
//! | `cashgear-ready` | The first frame has been drawn | None |
//! | `cashgear-error` | The game cannot go on: no GPU, a panic, an exit with an error | The message |
//!
//! ```js
//! addEventListener("cashgear-ready", () => loading.remove());
//! addEventListener("cashgear-error", (e) => showError(e.detail));
//! ```

/// The event sent once the first frame is on screen.
pub const READY_EVENT: &str = "cashgear-ready";

/// The event sent when the game cannot go on.
pub const ERROR_EVENT: &str = "cashgear-error";

/// Tells the page the first frame is on screen (`cashgear-ready`), so it can take its
/// loading screen down. Only the first call sends the event. The renderer calls it.
pub fn ready() {
    #[cfg(target_arch = "wasm32")]
    {
        use core::sync::atomic::{AtomicBool, Ordering};
        static SENT: AtomicBool = AtomicBool::new(false);
        if !SENT.swap(true, Ordering::Relaxed) {
            web::dispatch(READY_EVENT, None);
        }
    }
}

/// Tells the page the game cannot go on (`cashgear-error`, with `message` as the
/// event's `detail`), so it can say so instead of showing a frozen canvas. The message is
/// logged as an error on every target.
pub fn fatal(message: &str) {
    log::error!("{message}");
    #[cfg(target_arch = "wasm32")]
    web::dispatch(ERROR_EVENT, Some(message));
}

#[cfg(target_arch = "wasm32")]
pub(crate) mod web {
    use wasm_bindgen::JsValue;

    /// Dispatches a `CustomEvent` on `window`. Failures are ignored: the page may be gone.
    pub(crate) fn dispatch(name: &str, detail: Option<&str>) {
        let Some(window) = web_sys::window() else {
            return;
        };
        let init = web_sys::CustomEventInit::new();
        if let Some(detail) = detail {
            init.set_detail(&JsValue::from_str(detail));
        }
        if let Ok(event) = web_sys::CustomEvent::new_with_event_init_dict(name, &init) {
            let _ = window.dispatch_event(&event);
        }
    }
}
