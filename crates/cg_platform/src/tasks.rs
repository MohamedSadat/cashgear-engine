//! Running async work without threads: the engine's small task abstraction.
//!
//! Some platform APIs only exist as `async` functions, such as requesting the GPU adapter
//! and device in the browser. Engine crates hand that work to [`spawn_local`] instead of
//! blocking or spawning threads, which `wasm32-unknown-unknown` does not have.

use core::future::Future;

/// Runs a future on the current thread.
///
/// - Desktop: blocks until the future completes, then returns.
/// - Web: queues it on the browser's event loop and returns at once. It completes during a
///   later frame.
///
/// Callers should not rely on either: hand the result back through shared state and check
/// for it each frame.
///
/// ```
/// use std::sync::{Arc, Mutex};
///
/// let slot = Arc::new(Mutex::new(None));
/// let sender = slot.clone();
/// cg_platform::tasks::spawn_local(async move {
///     *sender.lock().unwrap() = Some(42);
/// });
/// // On desktop the task has already run; on the web, poll `slot` on later frames.
/// assert_eq!(*slot.lock().unwrap(), Some(42));
/// ```
pub fn spawn_local(future: impl Future<Output = ()> + 'static) {
    #[cfg(not(target_arch = "wasm32"))]
    pollster::block_on(future);
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_futures::spawn_local(future);
}
