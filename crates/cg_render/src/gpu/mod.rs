//! GPU code: device setup, the window surface, the offscreen renderer and the painter they
//! share.

pub(crate) mod offscreen;
pub(crate) mod painter;
pub(crate) mod window;

use core::fmt;

/// Why the renderer could not start or finish a frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderError {
    /// No GPU adapter fits: no supported graphics API, or none that can draw to the window.
    NoAdapter(String),
    /// The GPU device could not be created.
    Device(String),
    /// The window surface could not be created or has no usable format.
    Surface(String),
    /// Reading a rendered image back from the GPU failed.
    Readback(String),
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoAdapter(err) => write!(f, "no suitable GPU adapter: {err}"),
            Self::Device(err) => write!(f, "cannot create the GPU device: {err}"),
            Self::Surface(err) => write!(f, "cannot draw to the window: {err}"),
            Self::Readback(err) => write!(f, "cannot read the image back: {err}"),
        }
    }
}

impl std::error::Error for RenderError {}

/// Requests a device that works everywhere the engine runs, WebGL2 included, while using
/// the adapter's real texture size limit.
async fn request_device(
    adapter: &wgpu::Adapter,
) -> Result<(wgpu::Device, wgpu::Queue), RenderError> {
    adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("cg_render device"),
            required_limits: wgpu::Limits::downlevel_webgl2_defaults()
                .using_resolution(adapter.limits()),
            ..Default::default()
        })
        .await
        .map_err(|err| RenderError::Device(err.to_string()))
}

/// The adapter's name and graphics API, for [`RenderStats`](crate::RenderStats). Browsers
/// may hide the name behind WebGPU.
fn describe(info: &wgpu::AdapterInfo) -> (String, String) {
    let name = if info.name.is_empty() {
        "an unnamed GPU".to_owned()
    } else {
        info.name.clone()
    };
    (name, format!("{:?}", info.backend))
}
