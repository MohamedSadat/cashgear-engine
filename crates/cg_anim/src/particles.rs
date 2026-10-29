//! CPU particles: emitters that spawn, move and fade small sprites, drawn through the
//! sprite batcher.

use crate::rng::Rng;
use bevy_ecs::prelude::*;
use cg_assets::{AssetId, AssetServer, Assets, Handle};
use cg_core::glam::{Affine2, Vec2, vec2};
use cg_core::{Rect, Time};
use cg_platform::Window;
use cg_render::{
    Color, FrameView, Image, InheritedVisibility, Layer, QueuedSprite, RenderStats, Sprites,
    Visibility,
};
use cg_world::{InterpolatedTransforms, PrefabComponent, Transform2D};
use core::sync::atomic::{AtomicU64, Ordering};
use serde::{Deserialize, Serialize};

/// Where in the emitter new particles appear, in its local space.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub enum EmitShape {
    /// At the emitter's origin.
    #[default]
    Point,
    /// Anywhere in a rectangle of this size, centred on the origin.
    Rect(Vec2),
    /// Anywhere in a circle of this radius.
    Circle(f32),
}

/// What an emitter emits and how the particles behave. Angles are in degrees (0 points
/// right, 90 up) because these settings are mostly written by hand in prefabs; ranges are
/// `(min, max)`, picked from at random per particle.
///
/// ```ron
/// // Dust kicked up by feet: a burst of grey specks that rise a little and fade.
/// (
///     burst: 6,
///     rate: 0,
///     lifetime: (0.25, 0.5),
///     speed: (10, 30),
///     direction: 90,
///     spread: 150,
///     gravity: (0, -40),
///     size: [3, 1],
///     color: ["#f0e8d8c0", "#f0e8d800"],
/// )
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ParticleSettings {
    /// Particles per second while emitting.
    pub rate: f32,
    /// Particles emitted at once when the emitter starts (and on each
    /// [`restart`](ParticleEmitter::restart)).
    pub burst: u32,
    /// Seconds of emission at `rate`; infinite by default (`inf` in RON).
    pub duration: f32,
    /// Seconds a particle lives.
    pub lifetime: (f32, f32),
    /// Starting speed in world units per second.
    pub speed: (f32, f32),
    /// The direction particles leave in, in degrees, relative to the emitter's rotation.
    pub direction: f32,
    /// The width of the cone around `direction`, in degrees: 0 is a beam, 360 every way.
    pub spread: f32,
    /// Acceleration in world units per second squared, such as `(0, -200)` to fall.
    pub gravity: Vec2,
    /// The share of speed lost per second, as air resistance.
    pub drag: f32,
    /// Size over the particle's life, as evenly spaced keys blended in between: one key is
    /// constant, `[3, 0]` shrinks from 3 to nothing. A size of 1 draws the texture (or its
    /// source rectangle) one texel per world unit; the default white texture is 1×1, so
    /// sizes are world units.
    pub size: Vec<f32>,
    /// Multiplies each particle's sizes by a random factor in this range.
    pub scale: (f32, f32),
    /// Colour over the particle's life, as evenly spaced keys blended in between.
    pub color: Vec<Color>,
    /// Starting rotation in degrees.
    pub rotation: (f32, f32),
    /// Spin in degrees per second.
    pub spin: (f32, f32),
    /// Where particles appear.
    pub shape: EmitShape,
    /// Particles move with the emitter (a jet's flame) instead of staying where they were
    /// emitted in the world (dust, smoke).
    pub local: bool,
    /// At most this many particles live at once from this emitter.
    pub max: u32,
}

impl Default for ParticleSettings {
    fn default() -> Self {
        Self {
            rate: 10.0,
            burst: 0,
            duration: f32::INFINITY,
            lifetime: (1.0, 1.0),
            speed: (20.0, 40.0),
            direction: 90.0,
            spread: 30.0,
            gravity: Vec2::ZERO,
            drag: 0.0,
            size: vec![2.0],
            scale: (1.0, 1.0),
            color: vec![Color::WHITE, Color::WHITE.with_alpha(0.0)],
            rotation: (0.0, 0.0),
            spin: (0.0, 0.0),
            shape: EmitShape::Point,
            local: false,
            max: 1024,
        }
    }
}

/// One live particle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Particle {
    /// Where it is: in the world, or relative to the emitter for local emitters.
    pub position: Vec2,
    /// Units per second.
    pub velocity: Vec2,
    /// Radians.
    pub rotation: f32,
    /// Radians per second.
    pub spin: f32,
    /// Seconds since it was emitted.
    pub age: f32,
    /// Seconds it lives.
    pub lifetime: f32,
    /// Its random size factor.
    pub scale: f32,
}

/// Emits particles from the entity's position: continuously at a rate, in bursts, or on
/// demand with [`emit`](Self::emit). Particles are simulated every frame on the CPU and
/// drawn on the entity's [`Layer`] through the sprite batcher, one sprite each, so an
/// emitter with a texture costs one draw call per layer.
///
/// ```
/// use cg_anim::{ParticleEmitter, ParticleSettings};
/// use cg_core::prelude::*;
/// use cg_render::{Color, Layer};
/// use cg_world::Transform2D;
///
/// fn spawn_fountain(mut commands: Commands) {
///     let settings = ParticleSettings {
///         rate: 200.0,
///         lifetime: (1.0, 1.5),
///         speed: (120.0, 160.0),
///         spread: 20.0,
///         gravity: vec2(0.0, -240.0),
///         color: vec![Color::hex(0x7fd4ff), Color::hex(0x3a86ff).with_alpha(0.0)],
///         ..default()
///     };
///     commands.spawn((ParticleEmitter::new(settings), Transform2D::from_xy(0.0, 0.0), Layer(5)));
/// }
/// ```
///
/// All emitters together keep at most [`ParticleBudget::max`] particles alive. In
/// prefabs: `"ParticleEmitter": (settings: (burst: 8, rate: 0), despawn_when_done: true)`,
/// with an optional `texture` path and `source` rectangle.
#[derive(Component, Debug, Clone)]
#[require(Transform2D, Visibility)]
pub struct ParticleEmitter {
    /// What it emits.
    pub settings: ParticleSettings,
    /// The particles' texture; the default white texture draws squares.
    pub texture: Handle<Image>,
    /// The part of the texture to draw, in texels; `None` for all of it.
    pub source: Option<Rect>,
    /// Whether it emits at its rate. Particles already out keep going when it stops.
    pub emitting: bool,
    /// Despawn the entity once it has finished emitting and its last particle has died.
    pub despawn_when_done: bool,
    particles: Vec<Particle>,
    /// Seconds since it started.
    time: f32,
    /// Particles owed at the rate but not emitted yet (the fraction carries over).
    owed: f32,
    /// Extra particles asked for with `emit` (the start burst too).
    pending: u32,
    /// Where it was at the previous update, so particles emitted during a frame spread
    /// along the way it moved.
    last: Option<Vec2>,
    rng: Rng,
    /// What the particles cover, for culling: world space, or local for local emitters.
    bounds: Rect,
}

/// Seeds for emitters made without one, so two emitters never move in step.
static NEXT_SEED: AtomicU64 = AtomicU64::new(1);

impl ParticleEmitter {
    /// An emitter that starts emitting at once, with the start burst.
    pub fn new(settings: ParticleSettings) -> Self {
        let pending = settings.burst;
        Self {
            settings,
            texture: Handle::default(),
            source: None,
            emitting: true,
            despawn_when_done: false,
            particles: Vec::new(),
            time: 0.0,
            owed: 0.0,
            pending,
            last: None,
            rng: Rng::new(NEXT_SEED.fetch_add(1, Ordering::Relaxed)),
            bounds: Rect::default(),
        }
    }

    /// Draws `texture` (or its `source` rectangle) for each particle.
    pub fn with_texture(mut self, texture: Handle<Image>, source: Option<Rect>) -> Self {
        self.texture = texture;
        self.source = source;
        self
    }

    /// Uses a fixed random seed, so the particles come out the same every run.
    pub fn with_seed(mut self, seed: u64) -> Self {
        self.rng = Rng::new(seed);
        self
    }

    /// A one-shot effect: despawns once done.
    pub fn once(mut self) -> Self {
        self.despawn_when_done = true;
        self
    }

    /// Emits `count` more particles at the next update, emitting or not: a puff of dust
    /// at a footstep from an emitter with no rate.
    pub fn emit(&mut self, count: u32) {
        self.pending = self.pending.saturating_add(count);
    }

    /// Starts over: the burst again, and the rate for another `duration`.
    pub fn restart(&mut self) {
        self.time = 0.0;
        self.owed = 0.0;
        self.emitting = true;
        self.pending = self.pending.saturating_add(self.settings.burst);
    }

    /// The live particles.
    pub fn particles(&self) -> &[Particle] {
        &self.particles
    }

    /// The number of live particles.
    pub fn len(&self) -> usize {
        self.particles.len()
    }

    /// Whether no particle is alive.
    pub fn is_empty(&self) -> bool {
        self.particles.is_empty()
    }

    /// Whether it will emit nothing more and every particle has died.
    pub fn is_done(&self) -> bool {
        let emitting =
            self.emitting && self.settings.rate > 0.0 && self.time < self.settings.duration;
        !emitting && self.pending == 0 && self.particles.is_empty()
    }

    /// Ages, moves and removes particles.
    fn simulate(&mut self, dt: f32) {
        let settings = &self.settings;
        let drag = (-settings.drag * dt).exp();
        self.particles.retain_mut(|p| {
            p.age += dt;
            if p.age >= p.lifetime {
                return false;
            }
            p.velocity = (p.velocity + settings.gravity * dt) * drag;
            p.position += p.velocity * dt;
            p.rotation += p.spin * dt;
            true
        });
    }

    /// How many particles it wants to emit this frame, not counting the budget.
    fn wanted(&mut self, dt: f32) -> u32 {
        let settings = &self.settings;
        if self.emitting && settings.rate > 0.0 && self.time < settings.duration {
            let active = dt.min(settings.duration - self.time).max(0.0);
            self.owed += settings.rate * active;
        }
        self.time += dt;
        // A little slack, so float sums such as ten steps of 0.1 s at 10 per second
        // still come to ten.
        let from_rate = (self.owed + 1e-4).floor();
        self.owed -= from_rate;
        (from_rate as u32).saturating_add(core::mem::take(&mut self.pending))
    }

    /// Emits `count` particles spread over the frame: from `last` to `at`, and aged as if
    /// they had left at evenly spaced moments.
    fn spawn(&mut self, count: u32, at: Affine2, dt: f32) {
        let settings = &self.settings;
        let room = (settings.max as usize).saturating_sub(self.particles.len());
        let count = (count as usize).min(room);
        let from = self.last.unwrap_or(at.translation);
        for i in 0..count {
            let rng = &mut self.rng;
            // The fraction of the frame still to come after this particle left.
            let later = (i as f32 + rng.unit()) / count as f32;
            let offset = match settings.shape {
                EmitShape::Point => Vec2::ZERO,
                EmitShape::Rect(size) => (vec2(rng.unit(), rng.unit()) - 0.5) * size,
                EmitShape::Circle(radius) => {
                    let angle = rng.unit() * core::f32::consts::TAU;
                    Vec2::from_angle(angle) * radius * rng.unit().sqrt()
                }
            };
            let half = settings.spread / 2.0;
            let angle = (settings.direction + rng.range((-half, half))).to_radians();
            let speed = rng.range(settings.speed);
            let mut particle = Particle {
                position: offset,
                velocity: Vec2::from_angle(angle) * speed,
                rotation: rng.range(settings.rotation).to_radians(),
                spin: rng.range(settings.spin).to_radians(),
                age: 0.0,
                lifetime: rng.range(settings.lifetime).max(1e-3),
                scale: rng.range(settings.scale),
            };
            if !settings.local {
                // Into the world: placed by the emitter's transform at the moment it left.
                let origin = at.translation.lerp(from, later);
                particle.position = origin + at.matrix2 * offset;
                particle.velocity = (at.matrix2 * particle.velocity).normalize_or_zero() * speed;
            }
            // Catch up on the part of the frame since it left.
            let catch_up = later * dt;
            particle.age = catch_up;
            particle.velocity += settings.gravity * catch_up;
            particle.position += particle.velocity * catch_up;
            self.particles.push(particle);
        }
        self.last = Some(at.translation);
    }

    /// Recomputes the bounds of the live particles, drawn `texels` large at size 1.
    fn update_bounds(&mut self, texels: Vec2) {
        let largest = self
            .settings
            .size
            .iter()
            .fold(0.0f32, |a, &b| a.max(b.abs()))
            * self.settings.scale.0.abs().max(self.settings.scale.1.abs());
        // Half the diagonal: enough for any rotation.
        let reach = largest * texels.length() / 2.0;
        let Some(first) = self.particles.first() else {
            self.bounds = Rect::default();
            return;
        };
        let (mut min, mut max) = (first.position, first.position);
        for p in &self.particles {
            min = min.min(p.position);
            max = max.max(p.position);
        }
        self.bounds = Rect::from_corners(min - reach, max + reach);
    }
}

/// The colour or size at `t` (0 to 1) of keys spread evenly over a life.
fn sample<T: Copy>(keys: &[T], t: f32, blend: impl Fn(T, T, f32) -> T, empty: T) -> T {
    match keys {
        [] => empty,
        [only] => *only,
        _ => {
            let position = t.clamp(0.0, 1.0) * (keys.len() - 1) as f32;
            let index = (position as usize).min(keys.len() - 2);
            blend(keys[index], keys[index + 1], position - index as f32)
        }
    }
}

/// The prefab form of a [`ParticleEmitter`]: the texture by its asset path.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ParticleEmitterData {
    /// What it emits.
    pub settings: ParticleSettings,
    /// The texture's asset path; empty for the white texture.
    pub texture: String,
    /// See [`ParticleEmitter::source`].
    pub source: Option<Rect>,
    /// See [`ParticleEmitter::emitting`].
    pub emitting: bool,
    /// See [`ParticleEmitter::despawn_when_done`].
    pub despawn_when_done: bool,
}

impl Default for ParticleEmitterData {
    fn default() -> Self {
        Self {
            settings: ParticleSettings::default(),
            texture: String::new(),
            source: None,
            emitting: true,
            despawn_when_done: false,
        }
    }
}

impl PrefabComponent for ParticleEmitter {
    type Data = ParticleEmitterData;

    fn from_data(data: &ParticleEmitterData, world: &World) -> Result<Self, String> {
        let texture = if data.texture.is_empty() {
            Handle::default()
        } else {
            world
                .get_resource::<AssetServer>()
                .ok_or_else(|| format!("no asset server to load `{}`", data.texture))?
                .load(data.texture.as_str())
        };
        let mut emitter = Self::new(data.settings.clone()).with_texture(texture, data.source);
        emitter.emitting = data.emitting;
        emitter.despawn_when_done = data.despawn_when_done;
        Ok(emitter)
    }
}

/// The most particles alive at once across every emitter, and how many are. Emitters
/// asking for more get none until others die; the first time that happens it is logged.
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct ParticleBudget {
    /// The limit. `AnimPlugin::particle_budget` sets it; 16,384 by default.
    pub max: u32,
    live: u32,
    dropped: u64,
    warned: bool,
}

impl ParticleBudget {
    /// A budget of `max` particles.
    pub fn new(max: u32) -> Self {
        Self {
            max,
            live: 0,
            dropped: 0,
            warned: false,
        }
    }

    /// Particles alive after the latest update.
    pub fn live(&self) -> u32 {
        self.live
    }

    /// Particles not emitted because the budget was full, since the start.
    pub fn dropped(&self) -> u64 {
        self.dropped
    }
}

impl Default for ParticleBudget {
    fn default() -> Self {
        Self::new(16_384)
    }
}

/// Simulates every emitter for one frame, emits within the budget, and despawns finished
/// one-shot emitters.
pub(crate) fn update_particles(
    mut commands: Commands,
    time: Res<Time>,
    mut budget: ResMut<ParticleBudget>,
    images: Res<Assets<Image>>,
    transforms: InterpolatedTransforms,
    mut emitters: Query<(Entity, &mut ParticleEmitter)>,
) {
    let dt = time.delta_secs();
    let mut live = 0u32;
    for (_, mut emitter) in &mut emitters {
        if !emitter.particles.is_empty() {
            emitter.simulate(dt);
        }
        live += emitter.particles.len() as u32;
    }
    for (entity, mut emitter) in &mut emitters {
        let wanted = emitter.wanted(dt);
        if wanted > 0 {
            let room = budget.max.saturating_sub(live);
            let granted = wanted.min(room);
            if granted < wanted {
                budget.dropped += u64::from(wanted - granted);
                if !budget.warned {
                    budget.warned = true;
                    log::warn!(
                        "the particle budget of {} is full; emitters get fewer particles",
                        budget.max
                    );
                }
            }
            if granted > 0 {
                let at = transforms.get(entity).unwrap_or(Affine2::IDENTITY);
                let before = emitter.particles.len();
                emitter.spawn(granted, at, dt);
                live += (emitter.particles.len() - before) as u32;
            }
        } else if !emitter.settings.local
            && let Some(at) = transforms.get(entity)
        {
            emitter.last = Some(at.translation);
        }
        let texels = match emitter.source {
            Some(source) => source.size(),
            None => images
                .get(&emitter.texture)
                .map_or(Vec2::ONE, |i| i.size().as_vec2()),
        };
        emitter.update_bounds(texels);
        if emitter.despawn_when_done && emitter.is_done() {
            commands.entity(entity).despawn();
        }
    }
    budget.live = live;
}

/// Queues the particles of every visible emitter whose particles reach into the view.
pub(crate) fn queue_particles(
    emitters: Query<(
        Entity,
        &ParticleEmitter,
        &InheritedVisibility,
        Option<&Layer>,
    )>,
    transforms: InterpolatedTransforms,
    view: Res<FrameView>,
    window: Res<Window>,
    mut sprites: ResMut<Sprites>,
    mut stats: ResMut<RenderStats>,
) {
    let visible = view.0.visible_rect(&window);
    let mut culled = 0;
    for (entity, emitter, shown, layer) in &emitters {
        if emitter.particles.is_empty() || !shown.get() {
            continue;
        }
        let local = emitter.settings.local;
        let to_world = if local {
            transforms.get(entity).unwrap_or(Affine2::IDENTITY)
        } else {
            Affine2::IDENTITY
        };
        let bounds = if local {
            transform_rect(to_world, emitter.bounds)
        } else {
            emitter.bounds
        };
        if !bounds.overlaps(&visible) {
            culled += emitter.particles.len() as u32;
            continue;
        }
        let settings = &emitter.settings;
        let template = QueuedSprite {
            texture: AssetId::from(&emitter.texture),
            size: None,
            source: emitter.source,
            pivot: Vec2::splat(0.5),
            color: Color::WHITE,
            flip_x: false,
            flip_y: false,
            flash: Color::TRANSPARENT,
            transform: Affine2::IDENTITY,
            layer: layer.map_or(0, |l| l.0),
        };
        sprites.extend(emitter.particles.iter().map(|p| {
            let t = p.age / p.lifetime;
            let size = sample(&settings.size, t, |a, b, t| a + (b - a) * t, 1.0) * p.scale;
            let color = sample(&settings.color, t, Color::lerp, Color::WHITE);
            let placed =
                Affine2::from_scale_angle_translation(Vec2::splat(size), p.rotation, p.position);
            QueuedSprite {
                color,
                transform: to_world * placed,
                ..template
            }
        }));
    }
    stats.add_culled(culled);
}

/// The bounding box of `rect` under `transform`.
fn transform_rect(transform: Affine2, rect: Rect) -> Rect {
    let corners = [
        rect.min,
        rect.max,
        vec2(rect.min.x, rect.max.y),
        vec2(rect.max.x, rect.min.y),
    ];
    Rect::from_points(corners.map(|c| transform.transform_point2(c))).unwrap_or(rect)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn emitter(settings: ParticleSettings) -> ParticleEmitter {
        ParticleEmitter::new(settings).with_seed(1)
    }

    #[test]
    fn rates_emit_the_right_number_over_time() {
        let mut e = emitter(ParticleSettings {
            rate: 30.0,
            lifetime: (10.0, 10.0),
            ..ParticleSettings::default()
        });
        let mut total = 0;
        for _ in 0..60 {
            total += e.wanted(1.0 / 60.0);
        }
        assert_eq!(total, 30, "one second at 30 per second");
    }

    #[test]
    fn bursts_durations_and_done() {
        let mut e = emitter(ParticleSettings {
            rate: 10.0,
            burst: 5,
            duration: 0.5,
            lifetime: (0.25, 0.25),
            ..ParticleSettings::default()
        });
        let first = e.wanted(0.1);
        assert_eq!(first, 6, "the burst and one from the rate");
        e.spawn(first, Affine2::IDENTITY, 0.1);
        let mut total = first;
        for _ in 0..20 {
            e.simulate(0.1);
            let n = e.wanted(0.1);
            e.spawn(n, Affine2::IDENTITY, 0.1);
            total += n;
        }
        assert_eq!(total, 10, "five in the burst and five over half a second");
        assert!(e.is_done(), "{} particles left", e.len());
        e.emit(3);
        assert!(!e.is_done(), "asked for more");
        assert_eq!(e.wanted(0.1), 3);
    }

    #[test]
    fn particles_fly_fall_and_die() {
        let mut e = emitter(ParticleSettings {
            rate: 0.0,
            burst: 1,
            lifetime: (1.0, 1.0),
            speed: (100.0, 100.0),
            direction: 0.0,
            spread: 0.0,
            gravity: vec2(0.0, -100.0),
            ..ParticleSettings::default()
        });
        let n = e.wanted(0.0);
        e.spawn(n, Affine2::from_translation(vec2(10.0, 0.0)), 0.0);
        let p = e.particles()[0];
        assert_eq!(p.position, vec2(10.0, 0.0));
        assert!((p.velocity - vec2(100.0, 0.0)).length() < 1e-3);
        for _ in 0..10 {
            e.simulate(0.05);
        }
        let p = e.particles()[0];
        assert!((p.position.x - 60.0).abs() < 1e-3, "{p:?}");
        assert!(p.position.y < -10.0, "fell: {p:?}");
        e.simulate(0.6);
        assert!(e.is_empty(), "died of old age");
    }

    #[test]
    fn rotated_emitters_aim_their_particles() {
        let mut e = emitter(ParticleSettings {
            rate: 0.0,
            burst: 1,
            speed: (10.0, 10.0),
            direction: 0.0,
            spread: 0.0,
            ..ParticleSettings::default()
        });
        let n = e.wanted(0.0);
        e.spawn(n, Affine2::from_angle(core::f32::consts::FRAC_PI_2), 0.0);
        let v = e.particles()[0].velocity;
        assert!((v - vec2(0.0, 10.0)).length() < 1e-4, "{v}");
    }

    #[test]
    fn the_emitters_own_cap_holds() {
        let mut e = emitter(ParticleSettings {
            max: 4,
            lifetime: (5.0, 5.0),
            ..ParticleSettings::default()
        });
        e.spawn(10, Affine2::IDENTITY, 0.0);
        assert_eq!(e.len(), 4);
    }

    #[test]
    fn keys_blend_over_life() {
        let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
        assert_eq!(sample(&[], 0.5, lerp, 1.0), 1.0);
        assert_eq!(sample(&[3.0], 0.5, lerp, 1.0), 3.0);
        assert_eq!(sample(&[3.0, 1.0], 0.5, lerp, 1.0), 2.0);
        assert_eq!(sample(&[0.0, 10.0, 0.0], 0.25, lerp, 1.0), 5.0);
        assert_eq!(sample(&[0.0, 10.0, 0.0], 1.0, lerp, 1.0), 0.0);
    }

    #[test]
    fn settings_read_from_ron() {
        let settings: ParticleSettings = ron::from_str(
            r##"(burst: 6, rate: 0, size: [3, 1], color: ["#ffffff", "#ffffff00"], shape: Rect((8, 2)), duration: inf)"##,
        )
        .unwrap();
        assert_eq!(settings.burst, 6);
        assert_eq!(settings.shape, EmitShape::Rect(vec2(8.0, 2.0)));
        assert_eq!(settings.color[1], Color::WHITE.with_alpha(0.0));
        assert!(settings.duration.is_infinite());
        assert_eq!(settings.lifetime, (1.0, 1.0), "defaults fill the rest");
    }
}
