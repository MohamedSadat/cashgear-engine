//! Tweens: a property of an entity eased from one value to another over time.

use crate::ease::Ease;
use bevy_ecs::prelude::*;
use bevy_ecs::query::Has;
use cg_core::Time;
use cg_core::glam::Vec2;
use cg_render::{Camera2D, Color, Sprite};
use cg_world::{Interpolated, Transform2D};
use core::ops::DerefMut;

/// The property a [`Tween`] changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TweenProperty {
    /// [`Transform2D::translation`].
    Translation,
    /// [`Transform2D::scale`].
    Scale,
    /// [`Transform2D::rotation`], in radians. Tweens turn the way the numbers go, so
    /// several whole turns are possible.
    Rotation,
    /// [`Sprite::color`], blended in sRGB.
    Color,
    /// [`Sprite::flash`], for hit flashes.
    Flash,
    /// [`Camera2D::zoom`].
    Zoom,
}

impl TweenProperty {
    /// Whether the property lives in the transform.
    fn in_transform(self) -> bool {
        matches!(self, Self::Translation | Self::Scale | Self::Rotation)
    }
}

/// A value of a [`TweenProperty`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TweenValue {
    /// A translation.
    Translation(Vec2),
    /// A scale.
    Scale(Vec2),
    /// A rotation in radians.
    Rotation(f32),
    /// A sprite colour.
    Color(Color),
    /// A sprite flash colour.
    Flash(Color),
    /// A camera zoom.
    Zoom(f32),
}

impl TweenValue {
    /// The property this is a value of.
    pub fn property(&self) -> TweenProperty {
        match self {
            Self::Translation(_) => TweenProperty::Translation,
            Self::Scale(_) => TweenProperty::Scale,
            Self::Rotation(_) => TweenProperty::Rotation,
            Self::Color(_) => TweenProperty::Color,
            Self::Flash(_) => TweenProperty::Flash,
            Self::Zoom(_) => TweenProperty::Zoom,
        }
    }

    /// The value fraction `t` of the way from `self` to `to`; `t` may leave 0..=1 for
    /// overshooting curves. Values of different properties give `to`.
    pub fn lerp(self, to: Self, t: f32) -> Self {
        match (self, to) {
            (Self::Translation(a), Self::Translation(b)) => Self::Translation(a.lerp(b, t)),
            (Self::Scale(a), Self::Scale(b)) => Self::Scale(a.lerp(b, t)),
            (Self::Rotation(a), Self::Rotation(b)) => Self::Rotation(a + (b - a) * t),
            (Self::Color(a), Self::Color(b)) => Self::Color(lerp_color(a, b, t)),
            (Self::Flash(a), Self::Flash(b)) => Self::Flash(lerp_color(a, b, t)),
            (Self::Zoom(a), Self::Zoom(b)) => Self::Zoom(a + (b - a) * t),
            (_, to) => to,
        }
    }

    /// The same property's value from `value`, or `None` if `value` is the wrong type.
    fn with(self, value: TweenData) -> Option<Self> {
        Some(match (self.property(), value) {
            (TweenProperty::Translation, TweenData::Vec2(v)) => Self::Translation(v),
            (TweenProperty::Scale, TweenData::Vec2(v)) => Self::Scale(v),
            (TweenProperty::Rotation, TweenData::F32(v)) => Self::Rotation(v),
            (TweenProperty::Zoom, TweenData::F32(v)) => Self::Zoom(v),
            (TweenProperty::Color, TweenData::Color(c)) => Self::Color(c),
            (TweenProperty::Flash, TweenData::Color(c)) => Self::Flash(c),
            _ => return None,
        })
    }
}

/// Colours blend channel by channel, and overshooting curves stay in range.
fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    let c = a.lerp(b, t);
    Color::rgba(
        c.r.clamp(0.0, 1.0),
        c.g.clamp(0.0, 1.0),
        c.b.clamp(0.0, 1.0),
        c.a.clamp(0.0, 1.0),
    )
}

/// A plain value for [`Tween::from`]: a number, a vector or a colour.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TweenData {
    /// For rotations and zooms.
    F32(f32),
    /// For translations and scales.
    Vec2(Vec2),
    /// For colours and flashes.
    Color(Color),
}

impl From<f32> for TweenData {
    fn from(value: f32) -> Self {
        Self::F32(value)
    }
}

impl From<Vec2> for TweenData {
    fn from(value: Vec2) -> Self {
        Self::Vec2(value)
    }
}

impl From<Color> for TweenData {
    fn from(value: Color) -> Self {
        Self::Color(value)
    }
}

/// Eases one property of an entity to a value over time, optionally repeating, going
/// back and forth, or followed by another tween. Start tweens through the entity's
/// [`Tweens`], or with [`TweenCommandsExt::tween`]:
///
/// ```
/// use cg_anim::prelude::*;
/// use cg_core::prelude::*;
/// use cg_render::Color;
///
/// fn land(mut commands: Commands, hero: Single<Entity, With<Name>>) {
///     // Squash on landing, then spring back.
///     commands
///         .entity(*hero)
///         .tween(Tween::scale(Vec2::ONE, 0.25).from(vec2(1.3, 0.7)).ease(Ease::BackOut))
///         // A white flash that fades out over a fifth of a second.
///         .tween(Tween::flash(Color::WHITE, 0.2))
///         // Bob for ever.
///         .tween(Tween::translation(vec2(0.0, 4.0), 0.6).ease(Ease::SineInOut).yoyo().forever());
/// }
/// ```
///
/// A tween without a [`from`](Self::from) value starts from the property's value when
/// it starts. Tweens of the transform on [`Interpolated`] entities advance in fixed steps,
/// so interpolation keeps them smooth; all others advance every frame. Either way they
/// follow the time scale.
#[derive(Debug, Clone, PartialEq)]
pub struct Tween {
    /// The value it ends at.
    pub to: TweenValue,
    /// The value it starts at; `None` takes the property's value at the start.
    pub from: Option<TweenValue>,
    /// Seconds per play.
    pub duration: f32,
    /// The curve.
    pub ease: Ease,
    /// Seconds to wait before the first play.
    pub delay: f32,
    /// Plays before it finishes; `None` repeats for ever.
    pub plays: Option<u32>,
    /// Every second play runs backwards, from `to` to the start.
    pub yoyo: bool,
    /// A name sent with [`TweenFinished`].
    pub tag: Option<String>,
    /// Starts when this one finishes.
    pub next: Option<Box<Tween>>,
    elapsed: f32,
    played: u32,
    start: Option<TweenValue>,
}

impl Tween {
    /// Eases to `to` over `seconds`, once, linearly.
    pub fn new(to: TweenValue, seconds: f32) -> Self {
        Self {
            to,
            from: None,
            duration: seconds,
            ease: Ease::Linear,
            delay: 0.0,
            plays: Some(1),
            yoyo: false,
            tag: None,
            next: None,
            elapsed: 0.0,
            played: 0,
            start: None,
        }
    }

    /// Moves the entity to `to`.
    pub fn translation(to: Vec2, seconds: f32) -> Self {
        Self::new(TweenValue::Translation(to), seconds)
    }

    /// Scales the entity to `to`.
    pub fn scale(to: Vec2, seconds: f32) -> Self {
        Self::new(TweenValue::Scale(to), seconds)
    }

    /// Turns the entity to `to` radians.
    pub fn rotation(to: f32, seconds: f32) -> Self {
        Self::new(TweenValue::Rotation(to), seconds)
    }

    /// Blends the sprite's colour to `to`.
    pub fn color(to: Color, seconds: f32) -> Self {
        Self::new(TweenValue::Color(to), seconds)
    }

    /// A flash in `color` that fades out: the sprite's flash goes from `color` at its own
    /// alpha to transparent.
    pub fn flash(color: Color, seconds: f32) -> Self {
        Self::new(TweenValue::Flash(color.with_alpha(0.0)), seconds).from(color)
    }

    /// Zooms the camera to `to`.
    pub fn zoom(to: f32, seconds: f32) -> Self {
        Self::new(TweenValue::Zoom(to), seconds)
    }

    /// Starts from `value` (a number for rotations and zooms, a vector for translations
    /// and scales, a colour for colours and flashes) instead of the current value. A value
    /// of the wrong type is ignored with a warning.
    pub fn from(mut self, value: impl Into<TweenData>) -> Self {
        let value = value.into();
        match self.to.with(value) {
            Some(from) => self.from = Some(from),
            None => log::warn!(
                "a tween of {:?} cannot start from {value:?}",
                self.to.property()
            ),
        }
        self
    }

    /// Uses `ease`.
    pub fn ease(mut self, ease: Ease) -> Self {
        self.ease = ease;
        self
    }

    /// Waits `seconds` before starting.
    pub fn delay(mut self, seconds: f32) -> Self {
        self.delay = seconds;
        self
    }

    /// Plays `times` times.
    pub fn repeat(mut self, times: u32) -> Self {
        self.plays = Some(times);
        self
    }

    /// Repeats for ever.
    pub fn forever(mut self) -> Self {
        self.plays = None;
        self
    }

    /// Runs every second play backwards.
    pub fn yoyo(mut self) -> Self {
        self.yoyo = true;
        self
    }

    /// There and back once: two plays, the second backwards.
    pub fn ping_pong(self) -> Self {
        self.yoyo().repeat(2)
    }

    /// Names the tween for its [`TweenFinished`] message.
    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = Some(tag.into());
        self
    }

    /// Starts `next` when this tween finishes. Chains of any length work:
    /// `a.then(b.then(c))`.
    pub fn then(mut self, next: Tween) -> Self {
        match &mut self.next {
            Some(after) => **after = after.as_ref().clone().then(next),
            None => self.next = Some(Box::new(next)),
        }
        self
    }

    /// The property it changes.
    pub fn property(&self) -> TweenProperty {
        self.to.property()
    }

    /// Whether it has started, its delay over.
    pub fn is_started(&self) -> bool {
        self.start.is_some()
    }

    /// The value at the current time, once started.
    fn value(&self) -> Option<TweenValue> {
        let start = self.start?;
        let t = if self.duration > 0.0 {
            (self.elapsed / self.duration).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let backwards = self.yoyo && self.played % 2 == 1;
        let progress = if backwards {
            self.ease.apply(1.0 - t)
        } else {
            self.ease.apply(t)
        };
        Some(start.lerp(self.to, progress))
    }
}

/// The tweens running on an entity. Add tweens with [`start`](Self::start) (replacing
/// one on the same property) or [`add`](Self::add), or through
/// [`TweenCommandsExt::tween`]. Finished tweens are removed.
#[derive(Component, Debug, Clone, Default, PartialEq)]
pub struct Tweens {
    list: Vec<Tween>,
}

impl Tweens {
    /// Starts `tween`, stopping any tween of the same property.
    pub fn start(&mut self, tween: Tween) {
        self.stop(tween.property());
        self.list.push(tween);
    }

    /// Starts `tween` alongside the others. Of two tweens of one property, the one added
    /// later wins.
    pub fn add(&mut self, tween: Tween) {
        self.list.push(tween);
    }

    /// Stops the tweens of `property`, leaving it where it is.
    pub fn stop(&mut self, property: TweenProperty) {
        self.list.retain(|t| t.property() != property);
    }

    /// Stops every tween.
    pub fn clear(&mut self) {
        self.list.clear();
    }

    /// Whether a tween of `property` is running (or waiting out its delay).
    pub fn is_running(&self, property: TweenProperty) -> bool {
        self.list.iter().any(|t| t.property() == property)
    }

    /// The running tweens.
    pub fn iter(&self) -> impl Iterator<Item = &Tween> {
        self.list.iter()
    }

    /// The number of running tweens.
    pub fn len(&self) -> usize {
        self.list.len()
    }

    /// Whether no tween is running.
    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }
}

/// A tween played to its end. Tweens that repeat for ever never finish.
#[derive(Message, Debug, Clone, PartialEq)]
pub struct TweenFinished {
    /// The tweened entity.
    pub entity: Entity,
    /// The property.
    pub property: TweenProperty,
    /// The tween's [`tag`](Tween::tag).
    pub tag: Option<String>,
}

/// Starting tweens from [`EntityCommands`].
pub trait TweenCommandsExt {
    /// Starts `tween` on the entity (adding [`Tweens`] if needed), stopping any tween of
    /// the same property.
    fn tween(&mut self, tween: Tween) -> &mut Self;
}

impl TweenCommandsExt for EntityCommands<'_> {
    fn tween(&mut self, tween: Tween) -> &mut Self {
        self.queue(
            move |mut entity: EntityWorldMut| match entity.get_mut::<Tweens>() {
                Some(mut tweens) => tweens.start(tween),
                None => {
                    let mut tweens = Tweens::default();
                    tweens.start(tween);
                    entity.insert(tweens);
                }
            },
        )
    }
}

/// The parts of an entity that tweens can change.
type Targets<'a> = (
    Entity,
    &'a mut Tweens,
    Option<&'a mut Transform2D>,
    Option<&'a mut Sprite>,
    Option<&'a mut Camera2D>,
    Has<Interpolated>,
);

/// Advances the tweens that run in fixed steps: transform tweens on interpolated entities.
pub(crate) fn advance_fixed_tweens(
    time: Res<Time>,
    mut targets: Query<Targets>,
    mut finished: MessageWriter<TweenFinished>,
) {
    advance(time.fixed_delta_secs(), true, &mut targets, &mut finished);
}

/// Advances the tweens that run every frame: everything else.
pub(crate) fn advance_frame_tweens(
    time: Res<Time>,
    mut targets: Query<Targets>,
    mut finished: MessageWriter<TweenFinished>,
) {
    advance(time.delta_secs(), false, &mut targets, &mut finished);
}

fn advance(
    dt: f32,
    fixed: bool,
    targets: &mut Query<Targets>,
    finished: &mut MessageWriter<TweenFinished>,
) {
    for (entity, mut tweens, mut transform, mut sprite, mut camera, interpolated) in targets {
        if tweens.is_empty() {
            continue;
        }
        let runs_here = |t: &Tween| (t.property().in_transform() && interpolated) == fixed;
        if !tweens.list.iter().any(runs_here) {
            continue;
        }
        let mut done = Vec::new();
        for (index, tween) in tweens.list.iter_mut().enumerate() {
            if !runs_here(tween) {
                continue;
            }
            // Borrowed as `DerefMut`, so a part is marked changed only when written.
            let mut parts = Parts {
                transform: transform
                    .as_mut()
                    .map(|t| t as &mut dyn DerefMut<Target = _>),
                sprite: sprite.as_mut().map(|s| s as &mut dyn DerefMut<Target = _>),
                camera: camera.as_mut().map(|c| c as &mut dyn DerefMut<Target = _>),
            };
            match step(tween, dt, &mut parts) {
                Step::Running => {}
                Step::Finished => done.push(index),
                Step::NoTarget => {
                    log::warn!(
                        "entity {entity}: nothing to tween for {:?}; stopping the tween",
                        tween.property()
                    );
                    tween.next = None;
                    done.push(index);
                }
            }
        }
        // Finished tweens leave, replaced by their next tween if they have one.
        for &index in done.iter().rev() {
            let tween = tweens.list.remove(index);
            if tween.start.is_some() {
                finished.write(TweenFinished {
                    entity,
                    property: tween.property(),
                    tag: tween.tag.clone(),
                });
            }
            if let Some(next) = tween.next {
                tweens.list.insert(index, *next);
            }
        }
    }
}

/// Mutable access to the parts of one entity.
struct Parts<'a> {
    transform: Option<&'a mut dyn DerefMut<Target = Transform2D>>,
    sprite: Option<&'a mut dyn DerefMut<Target = Sprite>>,
    camera: Option<&'a mut dyn DerefMut<Target = Camera2D>>,
}

impl Parts<'_> {
    fn read(&self, property: TweenProperty) -> Option<TweenValue> {
        Some(match property {
            TweenProperty::Translation => {
                TweenValue::Translation(self.transform.as_ref()?.translation)
            }
            TweenProperty::Scale => TweenValue::Scale(self.transform.as_ref()?.scale),
            TweenProperty::Rotation => TweenValue::Rotation(self.transform.as_ref()?.rotation),
            TweenProperty::Color => TweenValue::Color(self.sprite.as_ref()?.color),
            TweenProperty::Flash => TweenValue::Flash(self.sprite.as_ref()?.flash),
            TweenProperty::Zoom => TweenValue::Zoom(self.camera.as_ref()?.zoom),
        })
    }

    /// Writes `value`; `false` if the entity lacks the part. An unchanged value is not
    /// written, so change detection stays quiet.
    fn write(&mut self, value: TweenValue) -> bool {
        match self.read(value.property()) {
            None => return false,
            Some(current) if current == value => return true,
            Some(_) => {}
        }
        match value {
            TweenValue::Translation(v) => {
                set(self.transform.as_mut().map(|t| &mut t.translation), v)
            }
            TweenValue::Scale(v) => set(self.transform.as_mut().map(|t| &mut t.scale), v),
            TweenValue::Rotation(v) => set(self.transform.as_mut().map(|t| &mut t.rotation), v),
            TweenValue::Color(c) => set(self.sprite.as_mut().map(|s| &mut s.color), c),
            TweenValue::Flash(c) => set(self.sprite.as_mut().map(|s| &mut s.flash), c),
            TweenValue::Zoom(v) => set(self.camera.as_mut().map(|c| &mut c.zoom), v),
        }
    }
}

/// Writes `value` into `slot`; `false` if there is no slot.
fn set<T>(slot: Option<&mut T>, value: T) -> bool {
    match slot {
        Some(slot) => {
            *slot = value;
            true
        }
        None => false,
    }
}

enum Step {
    Running,
    Finished,
    NoTarget,
}

fn step(tween: &mut Tween, dt: f32, parts: &mut Parts) -> Step {
    let mut dt = dt.max(0.0);
    if tween.start.is_none() {
        if tween.delay > dt {
            tween.delay -= dt;
            return Step::Running;
        }
        dt -= tween.delay;
        tween.delay = 0.0;
        let Some(current) = parts.read(tween.property()) else {
            return Step::NoTarget;
        };
        tween.start = Some(tween.from.unwrap_or(current));
    }
    tween.elapsed += dt;
    let mut finished = false;
    if tween.duration <= 0.0 {
        tween.played = tween.played.max(1);
        finished = true;
    } else {
        while tween.elapsed >= tween.duration {
            if tween.plays.is_some_and(|plays| tween.played + 1 >= plays) {
                tween.elapsed = tween.duration;
                finished = true;
                break;
            }
            tween.elapsed -= tween.duration;
            tween.played += 1;
        }
    }
    let Some(value) = tween.value() else {
        return Step::NoTarget;
    };
    if !parts.write(value) {
        return Step::NoTarget;
    }
    if finished {
        Step::Finished
    } else {
        Step::Running
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cg_core::glam::vec2;

    fn run(tween: &mut Tween, mut transform: &mut Transform2D, dt: f32) -> bool {
        let mut parts = Parts {
            transform: Some(&mut transform),
            sprite: None,
            camera: None,
        };
        matches!(step(tween, dt, &mut parts), Step::Finished)
    }

    #[test]
    fn tweens_ease_from_the_current_value() {
        let mut transform = Transform2D::from_xy(10.0, 0.0);
        let mut tween = Tween::translation(vec2(20.0, 10.0), 1.0).ease(Ease::QuadIn);
        assert!(!run(&mut tween, &mut transform, 0.5));
        assert_eq!(transform.translation, vec2(12.5, 2.5));
        assert!(
            run(&mut tween, &mut transform, 0.6),
            "finished past the end"
        );
        assert_eq!(transform.translation, vec2(20.0, 10.0), "lands exactly");
    }

    #[test]
    fn delays_and_from_values() {
        let mut transform = Transform2D::IDENTITY;
        let mut tween = Tween::rotation(1.0, 1.0).from(-1.0).delay(0.5);
        run(&mut tween, &mut transform, 0.25);
        assert_eq!(transform.rotation, 0.0, "still waiting");
        run(&mut tween, &mut transform, 0.5);
        assert_eq!(
            transform.rotation, -0.5,
            "a quarter of the way after the delay"
        );
        let ignored = Tween::rotation(1.0, 1.0).from(Vec2::ONE);
        assert_eq!(ignored.from, None, "a vector cannot start a rotation");
    }

    #[test]
    fn yoyo_repeats_go_back_and_forth() {
        let mut transform = Transform2D::IDENTITY;
        let mut tween = Tween::scale(vec2(2.0, 2.0), 1.0)
            .from(Vec2::ONE)
            .ping_pong();
        run(&mut tween, &mut transform, 0.5);
        assert_eq!(transform.scale, vec2(1.5, 1.5));
        run(&mut tween, &mut transform, 0.75);
        assert_eq!(transform.scale, vec2(1.75, 1.75), "on the way back");
        assert!(run(&mut tween, &mut transform, 1.0));
        assert_eq!(transform.scale, Vec2::ONE, "back where it started");

        let mut forever = Tween::rotation(1.0, 0.1).from(0.0).forever();
        for _ in 0..1000 {
            assert!(!run(&mut forever, &mut transform, 0.033));
        }
    }

    #[test]
    fn zero_length_tweens_jump_and_missing_targets_stop() {
        let mut transform = Transform2D::IDENTITY;
        let mut tween = Tween::translation(vec2(5.0, 5.0), 0.0);
        assert!(run(&mut tween, &mut transform, 0.0));
        assert_eq!(transform.translation, vec2(5.0, 5.0));
        let mut zoom = Tween::zoom(2.0, 1.0);
        let mut transform = &mut transform;
        let mut parts = Parts {
            transform: Some(&mut transform),
            sprite: None,
            camera: None,
        };
        assert!(matches!(step(&mut zoom, 0.1, &mut parts), Step::NoTarget));
    }

    #[test]
    fn flashes_fade_out_and_colours_stay_in_range() {
        let flash = Tween::flash(Color::WHITE, 0.2);
        assert_eq!(flash.from, Some(TweenValue::Flash(Color::WHITE)));
        assert_eq!(flash.to, TweenValue::Flash(Color::WHITE.with_alpha(0.0)));
        let over = lerp_color(Color::BLACK, Color::WHITE, 1.3);
        assert_eq!(over, Color::WHITE);
    }

    #[test]
    fn chains_append_at_the_end() {
        let chain = Tween::zoom(1.0, 1.0)
            .then(Tween::zoom(2.0, 1.0))
            .then(Tween::zoom(3.0, 1.0));
        let second = chain.next.as_ref().unwrap();
        assert_eq!(second.to, TweenValue::Zoom(2.0));
        assert_eq!(second.next.as_ref().unwrap().to, TweenValue::Zoom(3.0));
    }
}
