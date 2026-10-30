//! The [`Time`] resource and the fixed-timestep accumulator behind the game loop.

use bevy_ecs::resource::Resource;
use core::time::Duration;

/// Default fixed-timestep rate, in steps per second.
pub const DEFAULT_FIXED_HZ: f64 = 60.0;

/// Default cap on fixed steps run in one frame. Backlog beyond it is dropped so a slow
/// frame cannot snowball into ever-longer frames.
pub const DEFAULT_MAX_FIXED_STEPS: u32 = 5;

/// Default cap on one frame's delta. Longer frames (a breakpoint, a dragged window,
/// a backgrounded tab) are clamped to it.
pub const DEFAULT_MAX_DELTA: Duration = Duration::from_millis(250);

/// Frame and fixed-step timing for the game loop.
///
/// The loop calls into this resource once per frame: the runner measures the real frame
/// delta, the time scale stretches it, and an accumulator turns it into a whole number of
/// fixed steps. Gameplay runs in the [`FixedUpdate`](crate::FixedUpdate) schedule with a
/// constant [`fixed_delta`](Self::fixed_delta); rendering uses [`alpha`](Self::alpha) to
/// interpolate between the last two fixed states.
///
/// | Value | Scaled by [`time_scale`](Self::time_scale) | Meaning |
/// | --- | --- | --- |
/// | [`delta`](Self::delta) | yes | Time since the previous frame |
/// | [`raw_delta`](Self::raw_delta) | no | Same, ignoring the time scale (for UI) |
/// | [`elapsed`](Self::elapsed) | yes | Sum of all deltas |
/// | [`fixed_delta`](Self::fixed_delta) | no | Length of one fixed step (1/60 s by default) |
/// | [`fixed_elapsed`](Self::fixed_elapsed) | yes | Simulated time: fixed steps run × fixed delta |
#[derive(Resource, Debug, Clone)]
pub struct Time {
    delta: Duration,
    raw_delta: Duration,
    elapsed: Duration,
    raw_elapsed: Duration,
    fixed_delta: Duration,
    fixed_elapsed: Duration,
    frame_count: u64,
    fixed_step_count: u64,
    fixed_steps_this_frame: u32,
    time_scale: f32,
    fixed_paused: bool,
    alpha: f32,
    accumulator: Duration,
    max_fixed_steps: u32,
    max_delta: Duration,
}

impl Default for Time {
    fn default() -> Self {
        Self {
            delta: Duration::ZERO,
            raw_delta: Duration::ZERO,
            elapsed: Duration::ZERO,
            raw_elapsed: Duration::ZERO,
            fixed_delta: Duration::from_secs_f64(1.0 / DEFAULT_FIXED_HZ),
            fixed_elapsed: Duration::ZERO,
            frame_count: 0,
            fixed_step_count: 0,
            fixed_steps_this_frame: 0,
            time_scale: 1.0,
            fixed_paused: false,
            alpha: 0.0,
            accumulator: Duration::ZERO,
            max_fixed_steps: DEFAULT_MAX_FIXED_STEPS,
            max_delta: DEFAULT_MAX_DELTA,
        }
    }
}

impl Time {
    /// Scaled time since the previous frame.
    pub fn delta(&self) -> Duration {
        self.delta
    }

    /// Scaled time since the previous frame, in seconds.
    pub fn delta_secs(&self) -> f32 {
        self.delta.as_secs_f32()
    }

    /// Unscaled time since the previous frame, after the [`max_delta`](Self::max_delta) clamp.
    pub fn raw_delta(&self) -> Duration {
        self.raw_delta
    }

    /// Unscaled time since the previous frame, in seconds.
    pub fn raw_delta_secs(&self) -> f32 {
        self.raw_delta.as_secs_f32()
    }

    /// Scaled time since the app started.
    pub fn elapsed(&self) -> Duration {
        self.elapsed
    }

    /// Scaled time since the app started, in seconds.
    pub fn elapsed_secs(&self) -> f64 {
        self.elapsed.as_secs_f64()
    }

    /// Unscaled time since the app started.
    pub fn raw_elapsed(&self) -> Duration {
        self.raw_elapsed
    }

    /// Length of one fixed step. Use it for all gameplay maths in
    /// [`FixedUpdate`](crate::FixedUpdate).
    pub fn fixed_delta(&self) -> Duration {
        self.fixed_delta
    }

    /// Length of one fixed step, in seconds.
    pub fn fixed_delta_secs(&self) -> f32 {
        self.fixed_delta.as_secs_f32()
    }

    /// Simulated time: the number of fixed steps run times the fixed delta.
    pub fn fixed_elapsed(&self) -> Duration {
        self.fixed_elapsed
    }

    /// Number of frames started so far. The first frame is frame 1.
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// Number of fixed steps run so far.
    pub fn fixed_step_count(&self) -> u64 {
        self.fixed_step_count
    }

    /// Number of fixed steps run in the current frame.
    pub fn fixed_steps_this_frame(&self) -> u32 {
        self.fixed_steps_this_frame
    }

    /// Interpolation factor in `0.0..1.0` between the previous and the current fixed state,
    /// valid from [`Update`](crate::Update) on. Render `prev.lerp(current, alpha)`.
    pub fn alpha(&self) -> f32 {
        self.alpha
    }

    /// Multiplier applied to the frame delta. `1.0` is normal speed and `0.0` pauses gameplay.
    pub fn time_scale(&self) -> f32 {
        self.time_scale
    }

    /// Sets the time scale. Negative and non-finite values are treated as `0.0`.
    pub fn set_time_scale(&mut self, scale: f32) {
        self.time_scale = if scale.is_finite() {
            scale.max(0.0)
        } else {
            0.0
        };
    }

    /// Whether fixed steps are paused (see [`set_fixed_paused`](Self::set_fixed_paused)).
    pub fn fixed_paused(&self) -> bool {
        self.fixed_paused
    }

    /// Pauses or resumes the fixed steps alone. While paused, no
    /// [`FixedUpdate`](crate::FixedUpdate) step runs, so gameplay, physics and fixed-step
    /// animation stand still, but frames go on with scaled time: effects in
    /// [`Update`](crate::Update) such as particles keep moving. An editor pauses them while
    /// a scene is being edited. Time spent paused is not caught up afterwards.
    pub fn set_fixed_paused(&mut self, paused: bool) {
        self.fixed_paused = paused;
        if paused {
            self.accumulator = Duration::ZERO;
        }
    }

    /// Sets the fixed-step rate in steps per second.
    ///
    /// # Panics
    ///
    /// Panics if `hz` is not a positive, finite number.
    pub fn set_fixed_hz(&mut self, hz: f64) {
        assert!(
            hz.is_finite() && hz > 0.0,
            "fixed rate must be positive, got {hz}"
        );
        self.set_fixed_delta(Duration::from_secs_f64(1.0 / hz));
    }

    /// Sets the length of one fixed step.
    ///
    /// # Panics
    ///
    /// Panics if `delta` is zero.
    pub fn set_fixed_delta(&mut self, delta: Duration) {
        assert!(!delta.is_zero(), "fixed delta must not be zero");
        self.fixed_delta = delta;
    }

    /// Most fixed steps run in one frame.
    pub fn max_fixed_steps(&self) -> u32 {
        self.max_fixed_steps
    }

    /// Sets the most fixed steps run in one frame (at least 1).
    pub fn set_max_fixed_steps(&mut self, steps: u32) {
        self.max_fixed_steps = steps.max(1);
    }

    /// Longest frame delta accepted; longer frames are clamped to it.
    pub fn max_delta(&self) -> Duration {
        self.max_delta
    }

    /// Sets the longest frame delta accepted.
    pub fn set_max_delta(&mut self, max: Duration) {
        self.max_delta = max;
    }

    /// Starts a frame: clamps and scales `raw` and feeds it to the fixed-step accumulator.
    pub(crate) fn begin_frame(&mut self, raw: Duration) {
        let raw = raw.min(self.max_delta);
        self.raw_delta = raw;
        self.raw_elapsed += raw;
        self.delta = raw.mul_f64(f64::from(self.time_scale));
        self.elapsed += self.delta;
        self.frame_count += 1;
        self.fixed_steps_this_frame = 0;
        if !self.fixed_paused {
            self.accumulator += self.delta;
        }
    }

    /// Takes one fixed step from the accumulator. Returns `false` when the accumulator holds
    /// less than one step or the per-frame cap is reached; in the latter case the backlog is
    /// dropped, keeping only the partial step.
    pub(crate) fn expend_fixed_step(&mut self) -> bool {
        if self.accumulator < self.fixed_delta {
            return false;
        }
        if self.fixed_steps_this_frame >= self.max_fixed_steps {
            let step = self.fixed_delta.as_nanos();
            let dropped = self.accumulator.as_nanos() / step;
            log::debug!("game loop is behind: dropped {dropped} fixed steps");
            self.accumulator = duration_from_nanos(self.accumulator.as_nanos() % step);
            return false;
        }
        self.accumulator -= self.fixed_delta;
        self.fixed_elapsed += self.fixed_delta;
        self.fixed_step_count += 1;
        self.fixed_steps_this_frame += 1;
        true
    }

    /// Ends the fixed steps of a frame and computes the interpolation alpha.
    pub(crate) fn end_fixed_steps(&mut self) {
        let alpha = self.accumulator.as_secs_f64() / self.fixed_delta.as_secs_f64();
        self.alpha = (alpha as f32).clamp(0.0, 1.0);
    }
}

fn duration_from_nanos(nanos: u128) -> Duration {
    // The remainder of a division by a `Duration` always fits back into one.
    Duration::new(
        (nanos / 1_000_000_000) as u64,
        (nanos % 1_000_000_000) as u32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs one frame and returns the number of fixed steps it took.
    fn frame(time: &mut Time, delta: Duration) -> u32 {
        time.begin_frame(delta);
        while time.expend_fixed_step() {}
        time.end_fixed_steps();
        time.fixed_steps_this_frame()
    }

    #[test]
    fn defaults_to_60_hz() {
        let time = Time::default();
        assert_eq!(time.fixed_delta(), Duration::from_secs_f64(1.0 / 60.0));
        assert_eq!(time.time_scale(), 1.0);
        assert_eq!(time.max_fixed_steps(), DEFAULT_MAX_FIXED_STEPS);
    }

    #[test]
    fn one_step_per_frame_at_the_fixed_rate() {
        let mut time = Time::default();
        let step = time.fixed_delta();
        for _ in 0..120 {
            assert_eq!(frame(&mut time, step), 1);
        }
        assert_eq!(time.fixed_step_count(), 120);
        assert_eq!(time.frame_count(), 120);
        assert_eq!(time.alpha(), 0.0);
    }

    #[test]
    fn faster_frames_accumulate_to_the_fixed_rate() {
        let mut time = Time::default();
        let delta = Duration::from_secs_f64(1.0 / 144.0);
        let steps: u32 = (0..144).map(|_| frame(&mut time, delta)).sum();
        assert!((59..=60).contains(&steps), "ran {steps} steps in 1 s");
        assert!((0.0..1.0).contains(&time.alpha()));
    }

    #[test]
    fn slow_frames_catch_up_with_several_steps() {
        let mut time = Time::default();
        let steps = frame(&mut time, Duration::from_secs_f64(3.5 / 60.0));
        assert_eq!(steps, 3);
        assert!((time.alpha() - 0.5).abs() < 1e-3, "alpha {}", time.alpha());
    }

    #[test]
    fn catch_up_is_capped_and_the_backlog_dropped() {
        let mut time = Time::default();
        time.set_max_delta(Duration::from_secs(10));
        let steps = frame(&mut time, Duration::from_secs_f64(20.25 / 60.0));
        assert_eq!(steps, DEFAULT_MAX_FIXED_STEPS);
        // Only the partial step survives, so the next frame runs at most one step.
        assert!((time.alpha() - 0.25).abs() < 1e-3, "alpha {}", time.alpha());
        assert_eq!(frame(&mut time, Duration::from_secs_f64(0.5 / 60.0)), 0);
    }

    #[test]
    fn long_frames_are_clamped() {
        let mut time = Time::default();
        frame(&mut time, Duration::from_secs(5));
        assert_eq!(time.raw_delta(), DEFAULT_MAX_DELTA);
        assert_eq!(time.raw_elapsed(), DEFAULT_MAX_DELTA);
    }

    #[test]
    fn time_scale_slows_gameplay_but_not_raw_time() {
        let mut time = Time::default();
        time.set_time_scale(0.5);
        // A step that halves exactly, so the counts below are exact.
        let step = Duration::from_millis(16);
        time.set_fixed_delta(step);
        let steps: u32 = (0..60).map(|_| frame(&mut time, step)).sum();
        assert_eq!(steps, 30);
        assert_eq!(time.raw_elapsed(), step * 60);
        assert_eq!(time.elapsed(), step * 30);
        assert_eq!(time.fixed_elapsed(), step * 30);
    }

    #[test]
    fn zero_time_scale_pauses_fixed_steps() {
        let mut time = Time::default();
        time.set_time_scale(0.0);
        assert_eq!(frame(&mut time, Duration::from_millis(100)), 0);
        assert_eq!(time.delta(), Duration::ZERO);
        time.set_time_scale(-3.0);
        assert_eq!(time.time_scale(), 0.0);
        time.set_time_scale(f32::NAN);
        assert_eq!(time.time_scale(), 0.0);
    }

    #[test]
    fn paused_fixed_steps_keep_frame_time_going() {
        let mut time = Time::default();
        // A step that halves exactly.
        let step = Duration::from_millis(16);
        time.set_fixed_delta(step);
        assert_eq!(frame(&mut time, step / 2), 0);
        time.set_fixed_paused(true);
        assert_eq!(frame(&mut time, step * 3), 0);
        assert_eq!(time.delta(), step * 3);
        assert_eq!(time.fixed_step_count(), 0);
        // Nothing is caught up, not even the half step from before the pause.
        time.set_fixed_paused(false);
        assert_eq!(frame(&mut time, step / 2), 0);
        assert_eq!(frame(&mut time, step / 2), 1);
    }

    #[test]
    fn fixed_rate_can_change() {
        let mut time = Time::default();
        time.set_fixed_hz(30.0);
        let steps: u32 = (0..60)
            .map(|_| frame(&mut time, Duration::from_secs_f64(1.0 / 60.0)))
            .sum();
        assert!((29..=30).contains(&steps), "ran {steps} steps in 1 s");
    }

    #[test]
    #[should_panic(expected = "fixed rate must be positive")]
    fn zero_fixed_rate_panics() {
        Time::default().set_fixed_hz(0.0);
    }
}
