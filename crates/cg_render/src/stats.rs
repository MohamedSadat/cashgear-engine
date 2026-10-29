//! The [`RenderStats`] resource.

use bevy_ecs::resource::Resource;
use core::time::Duration;

/// How often the frame-time average is refreshed.
const SAMPLE_PERIOD: Duration = Duration::from_millis(500);

/// What the renderer drew last frame, the average frame time, and which GPU it runs on.
///
/// ```
/// use cg_core::prelude::*;
/// use cg_render::RenderStats;
///
/// fn report(stats: Res<RenderStats>) {
///     info!(
///         "{:.0} fps, {} sprites in {} draw calls on {}",
///         stats.fps(),
///         stats.sprites(),
///         stats.draw_calls(),
///         stats.adapter(),
///     );
/// }
/// ```
#[derive(Resource, Debug, Clone, Default)]
pub struct RenderStats {
    pub(crate) draw_calls: u32,
    pub(crate) sprites: u32,
    pub(crate) batches: u32,
    pub(crate) shapes: u32,
    pub(crate) adapter: String,
    pub(crate) backend: String,
    frame_time: Duration,
    sample_frames: u32,
    sample_time: Duration,
}

impl RenderStats {
    /// Draw calls issued last frame, including the pixel-perfect upscale and debug shapes.
    pub fn draw_calls(&self) -> u32 {
        self.draw_calls
    }

    /// Sprites drawn last frame. Sprites with a missing texture or no area are skipped.
    pub fn sprites(&self) -> u32 {
        self.sprites
    }

    /// Sprite batches last frame: runs of sprites sharing a layer and texture, one draw call
    /// each.
    pub fn batches(&self) -> u32 {
        self.batches
    }

    /// Debug shapes drawn last frame.
    pub fn shapes(&self) -> u32 {
        self.shapes
    }

    /// The average time between frames, refreshed twice a second. Zero until then.
    pub fn frame_time(&self) -> Duration {
        self.frame_time
    }

    /// Frames per second, from the average [`frame_time`](Self::frame_time). Zero until
    /// the first average.
    pub fn fps(&self) -> f32 {
        if self.frame_time.is_zero() {
            0.0
        } else {
            1.0 / self.frame_time.as_secs_f32()
        }
    }

    /// The GPU's name ("an unnamed GPU" when the browser hides it), or an empty string
    /// before the GPU is ready.
    pub fn adapter(&self) -> &str {
        &self.adapter
    }

    /// The graphics API in use (`Vulkan`, `Dx12`, `Metal`, `Gl`, `BrowserWebGpu`), or an
    /// empty string before the GPU is ready.
    pub fn backend(&self) -> &str {
        &self.backend
    }

    /// Adds one frame's unscaled delta to the running average.
    pub(crate) fn record_frame(&mut self, delta: Duration) {
        if delta.is_zero() {
            return;
        }
        self.sample_frames += 1;
        self.sample_time += delta;
        if self.sample_time >= SAMPLE_PERIOD {
            self.frame_time = self.sample_time / self.sample_frames;
            self.sample_frames = 0;
            self.sample_time = Duration::ZERO;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_time_averages_over_the_period() {
        let mut stats = RenderStats::default();
        assert_eq!(stats.fps(), 0.0);
        for _ in 0..49 {
            stats.record_frame(Duration::from_millis(10));
        }
        assert_eq!(stats.frame_time(), Duration::ZERO);
        stats.record_frame(Duration::from_millis(10));
        assert_eq!(stats.frame_time(), Duration::from_millis(10));
        for _ in 0..25 {
            stats.record_frame(Duration::from_millis(20));
        }
        assert_eq!(stats.frame_time(), Duration::from_millis(20));
        assert!((stats.fps() - 50.0).abs() < 0.01);
    }
}
