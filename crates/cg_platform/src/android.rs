//! Android: the app the entry point hands over, APK assets, the save folder, and the state
//! that the engine's Java activity keeps for the game.
//!
//! A game's Android library starts with `cashgear_engine::android_main!(my_game::run)`,
//! which exports the `android_main` function that `android-activity` calls on its own
//! thread and passes the [`AndroidApp`] to [`run`]. Everything else (the window, assets,
//! saves, sound) then works as on desktop.
//!
//! The APK's activity is `com.cashgear.engine.CashGearActivity`, a small `NativeActivity`
//! in `crates/cg_platform/android/java`. It hides the system bars (immersive mode), keeps
//! the screen on, counts presses of the back button or gesture, and records the display
//! cutout's safe insets. The runner reads those two each frame through JNI, so the back
//! button reaches the game as [`KeyCode::BrowserBack`](crate::input::KeyCode::BrowserBack)
//! and the view stays clear of notches ([`Window::safe_area`](crate::Window::safe_area)).
//! A plain `NativeActivity` works too, without those extras.

use crate::io::{BUNDLE_FILE, Bundle, IoError};
use jni::objects::{JObject, JString};
use jni::{Env, JavaVM, jni_sig, jni_str};
use std::ffi::CString;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, OnceLock};

/// The Android app handle from `android-activity`: the activity, its window, input queue,
/// asset manager and data folders.
pub use winit::platform::android::activity::AndroidApp;

/// Android's key code for the back button (`KeyEvent.KEYCODE_BACK`).
pub(crate) const KEYCODE_BACK: u32 = 4;

static APP: OnceLock<AndroidApp> = OnceLock::new();

/// Runs a game on Android: keeps `app` for the engine, then calls `game`, which builds the
/// [`App`](cg_core::App) and runs it as on desktop. Returns when the game exits; the
/// activity then finishes.
///
/// Called by `cashgear_engine::android_main!`; games do not call it themselves.
pub fn run(app: AndroidApp, game: impl FnOnce()) {
    crate::log::install_panic_hook();
    if APP.set(app).is_err() {
        // `CashGearActivity` ends the process when it is destroyed, so this only happens
        // with another activity. winit cannot start a second event loop in one process.
        log::error!("the game was started twice in one process; it may not run");
    }
    game();
    log::info!("the game has exited; finishing the activity");
}

/// The app handed over by the entry point, for games that call Android APIs themselves
/// (through JNI, say). `None` before [`run`].
pub fn app() -> Option<&'static AndroidApp> {
    APP.get()
}

/// Reads a file from the APK's `assets/` folder.
pub(crate) fn read_asset(path: &str) -> Result<Vec<u8>, IoError> {
    let app = APP.get().ok_or_else(|| {
        IoError::Unsupported("APK assets need the app from `android_main!`".to_owned())
    })?;
    let name = CString::new(path).map_err(|_| IoError::NotFound(path.to_owned()))?;
    let mut asset = app
        .asset_manager()
        .open(&name)
        .ok_or_else(|| IoError::NotFound(path.to_owned()))?;
    // Stored (uncompressed) assets are mapped from the APK; others are inflated here.
    asset
        .buffer()
        .map(<[u8]>::to_vec)
        .map_err(|err| IoError::Other {
            path: path.to_owned(),
            message: err.to_string(),
        })
}

/// The bundle in the APK's assets ([`BUNDLE_FILE`]), read once per process and shared by
/// every [`AssetIo`](crate::io::AssetIo).
pub(crate) fn apk_bundle() -> Option<Arc<Bundle>> {
    static BUNDLE: OnceLock<Option<Arc<Bundle>>> = OnceLock::new();
    BUNDLE
        .get_or_init(|| match read_asset(BUNDLE_FILE) {
            Ok(bytes) => crate::io::accept_bundle(BUNDLE_FILE, Bundle::parse(bytes)).map(Arc::new),
            Err(IoError::NotFound(_)) => {
                log::info!("no {BUNDLE_FILE} in the APK; reading assets one by one");
                None
            }
            Err(err) => {
                log::warn!("cannot read {BUNDLE_FILE} from the APK: {err}");
                None
            }
        })
        .clone()
}

/// The app's private files folder (`Context.getFilesDir()`), where saves go.
pub(crate) fn data_dir() -> Option<std::path::PathBuf> {
    APP.get()?.internal_data_path()
}

/// What `CashGearActivity` has recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ActivityState {
    /// Back presses since the previous call.
    pub back_presses: u32,
    /// The display cutout's safe insets in physical pixels: left, top, right, bottom.
    pub safe_insets: [u32; 4],
}

/// Reads the activity's `backPresses` (a count since it started) and `safeInsets` fields.
/// `None` when there is no app yet or the activity is not a `CashGearActivity` (logged
/// once; it is not asked again).
pub(crate) fn activity_state() -> Option<ActivityState> {
    static UNAVAILABLE: AtomicBool = AtomicBool::new(false);
    static BACK_PRESSES: AtomicU32 = AtomicU32::new(0);
    if UNAVAILABLE.load(Ordering::Relaxed) {
        return None;
    }
    let read = with_activity(|env, activity| {
        let back = env
            .get_field(activity, jni_str!("backPresses"), jni_sig!(jint))?
            .i()?;
        let insets = env
            .get_field(activity, jni_str!("safeInsets"), jni_sig!(jlong))?
            .j()?;
        let seen = BACK_PRESSES.swap(back as u32, Ordering::Relaxed);
        Ok(ActivityState {
            back_presses: (back as u32).wrapping_sub(seen),
            safe_insets: unpack_insets(insets),
        })
    })?;
    match read {
        Ok(state) => Some(state),
        Err(err) => {
            log::warn!(
                "the activity is not a CashGearActivity ({err}); the back button closes the \
                 app and display cutouts are not avoided"
            );
            UNAVAILABLE.store(true, Ordering::Relaxed);
            None
        }
    }
}

/// The launching intent's `args` extra, which `CashGearActivity` keeps in `launchArgs`.
/// Empty without one, or with another activity.
pub(crate) fn launch_args() -> String {
    let read = with_activity(|env, activity| {
        let args = env
            .get_field(activity, jni_str!("launchArgs"), jni_sig!(java.lang.String))?
            .l()?;
        let args = env.cast_local::<JString>(args)?;
        if args.is_null() {
            return Ok(String::new());
        }
        args.try_to_string(env)
    });
    match read {
        Some(Ok(args)) => args,
        Some(Err(err)) => {
            log::debug!("no launch arguments from the activity: {err}");
            String::new()
        }
        None => String::new(),
    }
}

/// Runs `f` on this thread's JNI environment (attaching the thread to the Java VM the first
/// time) with the activity object. `None` before [`run`].
fn with_activity<T>(
    f: impl FnOnce(&mut Env<'_>, &JObject<'_>) -> jni::errors::Result<T>,
) -> Option<jni::errors::Result<T>> {
    static VM: OnceLock<JavaVM> = OnceLock::new();
    let app = APP.get()?;
    // SAFETY: android-activity's pointer to the process's Java VM, valid for its life.
    let vm = VM.get_or_init(|| unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) });
    Some(vm.attach_current_thread(|env| {
        // SAFETY: a global reference that android-activity holds for the activity's life.
        // JNI accepts any kind of reference where a local one is expected, and the wrapper
        // never deletes it.
        let activity = unsafe { JObject::from_raw(env, app.activity_as_ptr().cast()) };
        f(env, &activity)
    }))
}

/// `CashGearActivity` packs the four insets into one `long`, 16 bits each from the lowest:
/// left, top, right, bottom.
fn unpack_insets(packed: i64) -> [u32; 4] {
    let packed = packed as u64;
    [0, 16, 32, 48].map(|shift| ((packed >> shift) & 0xffff) as u32)
}
