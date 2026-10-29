//! Easing curves for tweens and fades.

use core::f32::consts::PI;
use serde::{Deserialize, Serialize};

/// How a tween moves from its start to its end: maps the fraction of time passed (0 to 1)
/// to the fraction of the way travelled.
///
/// `In` curves start slowly, `Out` curves end slowly, and `InOut` curves run the `In`
/// curve over the first half and the `Out` curve over the second. `Back` overshoots,
/// `Elastic` wobbles and `Bounce` bounces; the shapes are Robert Penner's. Every curve
/// starts at exactly 0 and ends at exactly 1.
///
/// ```
/// use cg_anim::Ease;
///
/// assert_eq!(Ease::Linear.apply(0.25), 0.25);
/// assert_eq!(Ease::QuadIn.apply(0.5), 0.25);
/// assert_eq!(Ease::QuadOut.apply(0.5), 0.75);
/// ```
///
/// In RON files a curve is its name: `ease: CubicOut`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum Ease {
    /// Constant speed.
    #[default]
    Linear,
    /// Speeds up: t².
    QuadIn,
    /// Slows down: the mirror of `QuadIn`.
    QuadOut,
    /// Speeds up, then slows down.
    QuadInOut,
    /// Speeds up: t³.
    CubicIn,
    /// Slows down.
    CubicOut,
    /// Speeds up, then slows down.
    CubicInOut,
    /// Speeds up: t⁴.
    QuartIn,
    /// Slows down.
    QuartOut,
    /// Speeds up, then slows down.
    QuartInOut,
    /// Speeds up: t⁵.
    QuintIn,
    /// Slows down.
    QuintOut,
    /// Speeds up, then slows down.
    QuintInOut,
    /// Speeds up along a quarter sine wave.
    SineIn,
    /// Slows down along a quarter sine wave.
    SineOut,
    /// Speeds up, then slows down, along half a sine wave.
    SineInOut,
    /// Starts very slowly: 2^(10t − 10).
    ExpoIn,
    /// Ends very slowly.
    ExpoOut,
    /// Starts and ends very slowly.
    ExpoInOut,
    /// Speeds up along a quarter circle.
    CircIn,
    /// Slows down along a quarter circle.
    CircOut,
    /// Speeds up, then slows down.
    CircInOut,
    /// Pulls back a little before moving.
    BackIn,
    /// Overshoots the end a little, then settles.
    BackOut,
    /// Pulls back, then overshoots.
    BackInOut,
    /// Wobbles with growing swings before moving.
    ElasticIn,
    /// Overshoots and wobbles into place, like a spring.
    ElasticOut,
    /// Wobbles at both ends.
    ElasticInOut,
    /// Bounces with growing hops before moving.
    BounceIn,
    /// Falls into place and bounces, like a dropped ball.
    BounceOut,
    /// Bounces at both ends.
    BounceInOut,
}

/// The shape behind a group of `In`, `Out` and `InOut` curves.
#[derive(Clone, Copy)]
enum Curve {
    Power(i32),
    Sine,
    Expo,
    Circ,
    Back,
    Elastic,
    Bounce,
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    In,
    Out,
    InOut,
}

impl Ease {
    /// Every curve, in declaration order.
    pub const ALL: [Ease; 31] = [
        Ease::Linear,
        Ease::QuadIn,
        Ease::QuadOut,
        Ease::QuadInOut,
        Ease::CubicIn,
        Ease::CubicOut,
        Ease::CubicInOut,
        Ease::QuartIn,
        Ease::QuartOut,
        Ease::QuartInOut,
        Ease::QuintIn,
        Ease::QuintOut,
        Ease::QuintInOut,
        Ease::SineIn,
        Ease::SineOut,
        Ease::SineInOut,
        Ease::ExpoIn,
        Ease::ExpoOut,
        Ease::ExpoInOut,
        Ease::CircIn,
        Ease::CircOut,
        Ease::CircInOut,
        Ease::BackIn,
        Ease::BackOut,
        Ease::BackInOut,
        Ease::ElasticIn,
        Ease::ElasticOut,
        Ease::ElasticInOut,
        Ease::BounceIn,
        Ease::BounceOut,
        Ease::BounceInOut,
    ];

    /// The fraction of the way travelled after fraction `t` of the time. `t` is clamped
    /// to 0..=1; the result leaves that range only for `Back` and `Elastic` curves.
    pub fn apply(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        let Some((curve, mode)) = self.parts() else {
            return t;
        };
        // Exact ends, whatever rounding the curve's formula does.
        if t == 0.0 || t == 1.0 {
            return t;
        }
        match mode {
            Mode::In => curve.ease_in(t),
            Mode::Out => 1.0 - curve.ease_in(1.0 - t),
            Mode::InOut if t < 0.5 => curve.ease_in(2.0 * t) / 2.0,
            Mode::InOut => 1.0 - curve.ease_in(2.0 - 2.0 * t) / 2.0,
        }
    }

    fn parts(self) -> Option<(Curve, Mode)> {
        use Ease::*;
        let (curve, mode) = match self {
            Linear => return None,
            QuadIn => (Curve::Power(2), Mode::In),
            QuadOut => (Curve::Power(2), Mode::Out),
            QuadInOut => (Curve::Power(2), Mode::InOut),
            CubicIn => (Curve::Power(3), Mode::In),
            CubicOut => (Curve::Power(3), Mode::Out),
            CubicInOut => (Curve::Power(3), Mode::InOut),
            QuartIn => (Curve::Power(4), Mode::In),
            QuartOut => (Curve::Power(4), Mode::Out),
            QuartInOut => (Curve::Power(4), Mode::InOut),
            QuintIn => (Curve::Power(5), Mode::In),
            QuintOut => (Curve::Power(5), Mode::Out),
            QuintInOut => (Curve::Power(5), Mode::InOut),
            SineIn => (Curve::Sine, Mode::In),
            SineOut => (Curve::Sine, Mode::Out),
            SineInOut => (Curve::Sine, Mode::InOut),
            ExpoIn => (Curve::Expo, Mode::In),
            ExpoOut => (Curve::Expo, Mode::Out),
            ExpoInOut => (Curve::Expo, Mode::InOut),
            CircIn => (Curve::Circ, Mode::In),
            CircOut => (Curve::Circ, Mode::Out),
            CircInOut => (Curve::Circ, Mode::InOut),
            BackIn => (Curve::Back, Mode::In),
            BackOut => (Curve::Back, Mode::Out),
            BackInOut => (Curve::Back, Mode::InOut),
            ElasticIn => (Curve::Elastic, Mode::In),
            ElasticOut => (Curve::Elastic, Mode::Out),
            ElasticInOut => (Curve::Elastic, Mode::InOut),
            BounceIn => (Curve::Bounce, Mode::In),
            BounceOut => (Curve::Bounce, Mode::Out),
            BounceInOut => (Curve::Bounce, Mode::InOut),
        };
        Some((curve, mode))
    }

    /// Whether the curve goes below 0 or above 1 on the way (`Back` and `Elastic`).
    pub fn overshoots(self) -> bool {
        matches!(self.parts(), Some((Curve::Back | Curve::Elastic, _)))
    }
}

impl Curve {
    /// The `In` form, for `t` strictly between 0 and 1.
    fn ease_in(self, t: f32) -> f32 {
        match self {
            Curve::Power(n) => t.powi(n),
            Curve::Sine => 1.0 - (t * PI / 2.0).cos(),
            Curve::Expo => 2f32.powf(10.0 * t - 10.0),
            Curve::Circ => 1.0 - (1.0 - t * t).max(0.0).sqrt(),
            Curve::Back => {
                const C1: f32 = 1.70158;
                const C3: f32 = C1 + 1.0;
                C3 * t * t * t - C1 * t * t
            }
            Curve::Elastic => {
                const C4: f32 = 2.0 * PI / 3.0;
                -(2f32.powf(10.0 * t - 10.0)) * ((10.0 * t - 10.75) * C4).sin()
            }
            Curve::Bounce => 1.0 - bounce_out(1.0 - t),
        }
    }
}

/// A ball dropped onto the floor at t = 0 lands for good at t = 1.
fn bounce_out(t: f32) -> f32 {
    const N1: f32 = 7.5625;
    const D1: f32 = 2.75;
    if t < 1.0 / D1 {
        N1 * t * t
    } else if t < 2.0 / D1 {
        let t = t - 1.5 / D1;
        N1 * t * t + 0.75
    } else if t < 2.5 / D1 {
        let t = t - 2.25 / D1;
        N1 * t * t + 0.9375
    } else {
        let t = t - 2.625 / D1;
        N1 * t * t + 0.984375
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_curve_starts_at_zero_and_ends_at_one() {
        for ease in Ease::ALL {
            assert_eq!(ease.apply(0.0), 0.0, "{ease:?}");
            assert_eq!(ease.apply(1.0), 1.0, "{ease:?}");
            assert_eq!(ease.apply(-1.0), 0.0, "{ease:?} clamps");
            assert_eq!(ease.apply(2.0), 1.0, "{ease:?} clamps");
        }
    }

    #[test]
    fn in_out_curves_are_symmetric_and_pass_the_middle() {
        for ease in Ease::ALL {
            if !matches!(ease.parts(), Some((_, Mode::InOut))) {
                continue;
            }
            assert!((ease.apply(0.5) - 0.5).abs() < 1e-6, "{ease:?}");
            for i in 1..10 {
                let t = i as f32 / 10.0;
                let sum = ease.apply(t) + ease.apply(1.0 - t);
                assert!((sum - 1.0).abs() < 1e-5, "{ease:?} at {t}");
            }
        }
    }

    #[test]
    fn plain_curves_never_go_backwards_or_overshoot() {
        let bounces = |e: Ease| matches!(e.parts(), Some((Curve::Bounce, _)));
        for ease in Ease::ALL.into_iter().filter(|e| !e.overshoots()) {
            let mut last = 0.0;
            for i in 1..=100 {
                let value = ease.apply(i as f32 / 100.0);
                // Bounces fall back between hops, but stay in range.
                assert!(
                    bounces(ease) || value >= last - 1e-6,
                    "{ease:?} goes back at {i}"
                );
                assert!((0.0..=1.0).contains(&value), "{ease:?} leaves 0..1 at {i}");
                last = value;
            }
        }
        assert!(Ease::BackOut.apply(0.6) > 1.0, "back overshoots");
        assert!(Ease::BackIn.apply(0.2) < 0.0, "back pulls back first");
    }

    #[test]
    fn known_values() {
        assert_eq!(Ease::CubicIn.apply(0.5), 0.125);
        assert!((Ease::SineOut.apply(0.5) - (PI / 4.0).sin()).abs() < 1e-6);
        assert!(
            (Ease::BounceOut.apply(1.0 / 2.75) - 1.0).abs() < 1e-6,
            "first landing"
        );
        assert!((Ease::ExpoIn.apply(0.5) - 2f32.powi(-5)).abs() < 1e-7);
    }

    #[test]
    fn curves_read_from_ron_by_name() {
        let ease: Ease = ron::from_str("ElasticOut").unwrap();
        assert_eq!(ease, Ease::ElasticOut);
    }
}
