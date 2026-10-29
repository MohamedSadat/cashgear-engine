//! Fading the screen to a colour and back, and fading between game states.

use crate::ease::Ease;
use bevy_ecs::prelude::*;
use cg_assets::AssetId;
use cg_core::glam::Vec2;
use cg_core::{App, Time, Update};
use cg_platform::Window;
use cg_render::{Color, FrameView, QueuedSprite, Sprites};
use cg_world::{NextState, StateChange, States};

/// A colour drawn over the whole view, above every layer: fade out to black between
/// levels, flash the screen red on a hit. Fades run on real time, so they work while the
/// game is paused.
///
/// ```
/// use cg_anim::ScreenFade;
/// use cg_core::prelude::*;
///
/// fn game_over(mut fade: ResMut<ScreenFade>) {
///     fade.fade_out(0.5);
/// }
/// ```
///
/// To fade between game states, see [`StateFade`].
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct ScreenFade {
    /// The colour faded to. Its alpha scales the fade's.
    pub color: Color,
    /// The curve fades follow.
    pub ease: Ease,
    alpha: f32,
    from: f32,
    to: f32,
    duration: f32,
    elapsed: f32,
}

impl Default for ScreenFade {
    fn default() -> Self {
        Self {
            color: Color::BLACK,
            ease: Ease::QuadInOut,
            alpha: 0.0,
            from: 0.0,
            to: 0.0,
            duration: 0.0,
            elapsed: 0.0,
        }
    }
}

impl ScreenFade {
    /// Fades to fully covered over `seconds`.
    pub fn fade_out(&mut self, seconds: f32) {
        self.fade_to(1.0, seconds);
    }

    /// Fades to clear over `seconds`.
    pub fn fade_in(&mut self, seconds: f32) {
        self.fade_to(0.0, seconds);
    }

    /// Fades to `alpha` (0 clear, 1 covered) over `seconds`, from where it is now.
    pub fn fade_to(&mut self, alpha: f32, seconds: f32) {
        self.from = self.alpha;
        self.to = alpha.clamp(0.0, 1.0);
        self.duration = seconds.max(0.0);
        self.elapsed = 0.0;
        if self.duration == 0.0 {
            self.alpha = self.to;
        }
    }

    /// Covers the screen to `alpha` at once.
    pub fn set(&mut self, alpha: f32) {
        self.fade_to(alpha, 0.0);
    }

    /// How covered the screen is, from 0 to 1.
    pub fn alpha(&self) -> f32 {
        self.alpha
    }

    /// Whether a fade is under way.
    pub fn is_fading(&self) -> bool {
        self.alpha != self.to
    }

    /// Whether the screen is fully covered and stays so.
    pub fn is_covered(&self) -> bool {
        self.alpha >= 1.0 && !self.is_fading()
    }

    fn advance(&mut self, dt: f32) {
        if !self.is_fading() {
            return;
        }
        self.elapsed += dt;
        let t = if self.duration > 0.0 {
            self.elapsed / self.duration
        } else {
            1.0
        };
        self.alpha = if t >= 1.0 {
            self.to
        } else {
            self.from + (self.to - self.from) * self.ease.apply(t)
        };
    }
}

pub(crate) fn advance_fade(time: Res<Time>, mut fade: ResMut<ScreenFade>) {
    if fade.is_fading() {
        fade.advance(time.raw_delta_secs());
    }
}

/// Queues the fade's colour over the whole window, in screen space on top of everything,
/// so menus and other UI fade with the game.
pub(crate) fn queue_fade(
    fade: Res<ScreenFade>,
    view: Res<FrameView>,
    window: Res<Window>,
    mut sprites: ResMut<Sprites>,
) {
    let alpha = fade.alpha * fade.color.a;
    if alpha <= 0.0 {
        return;
    }
    let size = window.physical_size();
    let space = view
        .0
        .camera
        .screen_space(size, window.scale_factor() as f32);
    // A little larger than the window, so rounding never shows an edge.
    let rect = space.target_rect(size);
    let rect = cg_core::Rect::from_corners(rect.min - Vec2::ONE, rect.max + Vec2::ONE);
    sprites.push_screen(QueuedSprite::screen_rect(
        AssetId::DEFAULT,
        None,
        rect,
        fade.color.with_alpha(alpha),
        i32::MAX,
    ));
}

/// Changes the game state behind a fade: the screen fades out, the change is made while
/// it is covered (so the old state's `OnExit` and the new one's `OnEnter` happen out of
/// sight), then it fades back in. Ask with the same methods as [`NextState`]; a request
/// made during a fade waits for it to finish.
///
/// Add it with [`AppFadeExt::add_state_fade`]:
///
/// ```
/// use cg_anim::prelude::*;
/// use cg_core::prelude::*;
/// use cg_world::prelude::*;
///
/// #[derive(Clone, PartialEq, Eq, Hash, Debug)]
/// enum Screen {
///     Title,
///     Playing,
/// }
///
/// fn start(mut fade: ResMut<StateFade<Screen>>) {
///     fade.set(Screen::Playing);
/// }
///
/// let mut app = App::new();
/// app.add_plugins(WorldPlugin)
///     .add_plugins(AnimPlugin::default())
///     .init_state(Screen::Title)
///     .add_state_fade::<Screen>();
/// ```
#[derive(Resource, Debug)]
pub struct StateFade<S: States> {
    /// Seconds to fade out.
    pub out_seconds: f32,
    /// Seconds to fade back in.
    pub in_seconds: f32,
    queue: Vec<StateChange<S>>,
    phase: Phase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Idle,
    /// Fading out, the change to make when covered.
    Out,
    /// The change is requested; fade in once it has applied.
    Changed,
}

impl<S: States> Default for StateFade<S> {
    fn default() -> Self {
        Self {
            out_seconds: 0.3,
            in_seconds: 0.3,
            queue: Vec::new(),
            phase: Phase::Idle,
        }
    }
}

impl<S: States> StateFade<S> {
    /// Replaces the whole stack with `state`, behind a fade.
    pub fn set(&mut self, state: S) {
        self.queue.push(StateChange::Set(state));
    }

    /// Pushes `state` on top, behind a fade.
    pub fn push(&mut self, state: S) {
        self.queue.push(StateChange::Push(state));
    }

    /// Pops the top state, behind a fade.
    pub fn pop(&mut self) {
        self.queue.push(StateChange::Pop);
    }

    /// Replaces the top state with `state`, behind a fade.
    pub fn replace(&mut self, state: S) {
        self.queue.push(StateChange::Replace(state));
    }

    /// Whether a change is waiting or a fade is under way.
    pub fn is_busy(&self) -> bool {
        !self.queue.is_empty() || self.phase != Phase::Idle
    }
}

/// Drives [`StateFade<S>`]: fades out for a waiting change, makes it when covered, fades
/// in a frame later, once the new state has entered.
fn run_state_fade<S: States>(
    mut fades: ResMut<StateFade<S>>,
    mut screen: ResMut<ScreenFade>,
    mut next: ResMut<NextState<S>>,
) {
    match fades.phase {
        Phase::Idle if !fades.queue.is_empty() => {
            screen.fade_out(fades.out_seconds);
            fades.phase = Phase::Out;
        }
        Phase::Out if screen.is_covered() => {
            match fades.queue.remove(0) {
                StateChange::Set(state) => next.set(state),
                StateChange::Push(state) => next.push(state),
                StateChange::Pop => next.pop(),
                StateChange::Replace(state) => next.replace(state),
            }
            fades.phase = Phase::Changed;
        }
        Phase::Changed => {
            screen.fade_in(fades.in_seconds);
            fades.phase = Phase::Idle;
        }
        _ => {}
    }
}

/// Adding [`StateFade`] for a state type.
pub trait AppFadeExt {
    /// Adds the [`StateFade<S>`] resource and the system that runs it. Needs
    /// [`AnimPlugin`](crate::AnimPlugin) and `init_state::<S>` first.
    fn add_state_fade<S: States>(&mut self) -> &mut Self;
}

impl AppFadeExt for App {
    fn add_state_fade<S: States>(&mut self) -> &mut Self {
        self.init_resource::<StateFade<S>>().add_systems(
            Update,
            run_state_fade::<S>.after(crate::AnimSystems::Effects),
        );
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fades_ease_to_their_target_and_stop() {
        let mut fade = ScreenFade {
            ease: Ease::Linear,
            ..ScreenFade::default()
        };
        fade.fade_out(1.0);
        assert!(fade.is_fading());
        fade.advance(0.25);
        assert_eq!(fade.alpha(), 0.25);
        fade.advance(1.0);
        assert_eq!(fade.alpha(), 1.0);
        assert!(fade.is_covered());
        fade.fade_in(0.5);
        fade.advance(0.25);
        assert_eq!(fade.alpha(), 0.5, "from where it was");
        fade.set(0.0);
        assert!(!fade.is_fading() && fade.alpha() == 0.0);
    }
}
