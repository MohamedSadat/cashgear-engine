// The page's loading screen, wired into trunk as the game's `data-initializer`.
//
// - Downloads the asset bundle (`assets.cgpak`, made by `cg_pack trunk-bundle` in release
//   builds) while the game's code downloads, with one progress bar for both, and hands it
//   to the engine as `window.cashgearBundle`. Debug builds (`trunk serve`) have no bundle;
//   the engine then fetches loose files.
// - Stays up until the engine has drawn its first frame (`cashgear-ready`) and the bundle
//   has arrived, then fades out.
// - Shows the engine's fatal errors (`cashgear-error`) and failed downloads in a panel with
//   a Reload button instead of a frozen canvas.
// - Turns phones sideways: a notice in portrait, which also takes the focus off the game so
//   it pauses; and landscape lock where the browser allows it in fullscreen.

const BUNDLE_URL = "assets.cgpak";
const MAGIC = [0x43, 0x47, 0x50, 0x41, 0x4b, 0x00]; // "CGPAK\0"

const loading = document.getElementById("loading");
const bar = document.getElementById("loading-bar");
const status = document.getElementById("loading-status");
const canvas = document.getElementById("cashgear");

const wasm = { current: 0, total: 0 };
const bundle = { current: 0, total: 0, done: false };
let ready = false;
let failed = false;

// Progress counts unpacked bytes (the browser unzips the code as it arrives), so it is
// shown as a percentage: megabytes would overstate a gzipped download.
function show() {
  if (failed) return;
  const total = wasm.total + bundle.total;
  const current = Math.min(wasm.current, wasm.total) + Math.min(bundle.current, bundle.total);
  const fraction = total > 0 ? current / total : 0;
  bar.style.width = `${(fraction * 100).toFixed(1)}%`;
  if (current < total || total === 0) {
    status.textContent = total > 0 ? `Loading ${Math.floor(fraction * 100)}%` : "Loading";
  } else {
    status.textContent = "Starting…";
  }
}

function finish() {
  if (!ready || !bundle.done || failed) return;
  loading.classList.add("done");
  loading.addEventListener("transitionend", () => loading.remove(), { once: true });
  canvas.focus();
}

function fail(title, detail) {
  if (failed) return;
  failed = true;
  console.error(title, detail);
  const panel = document.getElementById("error");
  document.getElementById("error-title").textContent = title;
  let hint = "";
  if (/adapter|GPU|WebGL|WebGPU|surface/i.test(String(detail))) {
    hint =
      "This browser could not start the game's graphics. Try an up-to-date Chrome, Edge, " +
      "Firefox or Safari, and check that hardware acceleration is on.";
  }
  document.getElementById("error-hint").textContent = hint;
  document.getElementById("error-detail").textContent = String(detail ?? "");
  panel.hidden = false;
  loading.remove();
}

/// Starts the bundle download, as early as this module runs (the page preloads it too).
function downloadBundle() {
  return fetch(BUNDLE_URL)
    .then(async (response) => {
      const html = (response.headers.get("content-type") ?? "").startsWith("text/html");
      if (!response.ok || html || !response.body) {
        // No bundle: a debug build, whose assets are loose files.
        return null;
      }
      const reader = response.body.getReader();
      const chunks = [];
      let received = 0;
      let header = new Uint8Array(0);
      for (;;) {
        const { done, value } = await reader.read();
        if (done) break;
        chunks.push(value);
        received += value.byteLength;
        if (header.length < 12) {
          // The header's first 12 bytes: the magic, the version and the total size.
          const joined = new Uint8Array(header.length + value.byteLength);
          joined.set(header);
          joined.set(value, header.length);
          header = joined.subarray(0, 12);
          if (header.length >= MAGIC.length && MAGIC.some((byte, i) => header[i] !== byte)) {
            await reader.cancel();
            return null;
          }
          if (header.length === 12) {
            bundle.total = new DataView(header.buffer, header.byteOffset).getUint32(8, true);
          }
        }
        bundle.current = received;
        show();
      }
      const bytes = new Uint8Array(received);
      let at = 0;
      for (const chunk of chunks) {
        bytes.set(chunk, at);
        at += chunk.byteLength;
      }
      return bytes.buffer;
    })
    .catch((error) => {
      fail("Could not download the game", error);
      return null;
    })
    .then((buffer) => {
      bundle.done = true;
      if (!buffer) bundle.total = bundle.current = 0;
      show();
      finish();
      return buffer;
    });
}

function turnSideways() {
  // In portrait on a phone the notice covers the game; blurring the canvas pauses it.
  const portrait = matchMedia("(orientation: portrait) and (pointer: coarse)");
  const update = () => {
    if (portrait.matches) canvas.blur();
  };
  portrait.addEventListener("change", update);
  update();
  document.addEventListener("fullscreenchange", () => {
    if (document.fullscreenElement && matchMedia("(pointer: coarse)").matches) {
      screen.orientation?.lock?.("landscape").catch(() => {});
    }
  });
}

addEventListener("cashgear-ready", () => {
  ready = true;
  finish();
});
addEventListener("cashgear-error", (event) => fail("The game stopped", event.detail));
// No context menu on a long press, and no page zoom by pinching on iOS.
canvas.addEventListener("contextmenu", (event) => event.preventDefault());
document.addEventListener("gesturestart", (event) => event.preventDefault());
turnSideways();

window.cashgearBundle = downloadBundle();

export default function initializer() {
  return {
    onProgress({ current, total }) {
      wasm.current = current;
      wasm.total = total;
      show();
    },
    onFailure(error) {
      fail("The game could not start", error);
    },
  };
}
