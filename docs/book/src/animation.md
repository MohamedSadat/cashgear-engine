# Animation and effects

## Sprite sheets from Aseprite

Export a sprite sheet from [Aseprite](https://www.aseprite.org/) with its JSON data ("Hash" or "Array", tags included) and load the `.json` as a `SpriteSheet`. Each tag becomes a clip, with its direction (forward, reverse, ping-pong) and repeat count. A tag's user data can name events: `0:footstep, 3:footstep`.

```rust,ignore
commands.spawn((
    SpriteAnimation::new(server.load("images/alien.json"), "run").with_pivot(FEET),
    Transform2D::default(),
));
```

`SpriteAnimation` plays a clip; `play("jump")` switches. `AnimationEvent` messages carry the clip events (footstep sounds), and `AnimationFinished` reports clips that ended. Clips advance in fixed steps, so they are frame-rate independent.

## State machines

An `AnimationGraph` (a `.anim.ron` file, reloaded when saved) lists states and the transitions between them, with conditions on parameters:

```ron
(
    start: "idle",
    states: [(name: "idle"), (name: "run", speed_param: "speed", speed: 0.008), (name: "jump")],
    transitions: [
        (from: ["idle", "run"], to: "jump", when: [("grounded", False)]),
        (from: ["idle"], to: "run", when: [("speed", Gt(5))]),
        (from: ["run"], to: "idle", when: [("speed", Le(5))]),
    ],
)
```

An `Animator` runs it: set its parameters every fixed step (`set("speed", vx.abs())`, `set_bool("grounded", on_ground)`, `trigger("hurt")`) and it picks the state and plays its clip. The template's player works this way.

## Tweens

`Tween`s animate a transform, a sprite's colour or flash, or the camera's zoom over time with easing curves (`Ease::QuadOut`, `BackOut`, `ElasticOut`, `BounceOut`...): squash and stretch on landing, a hit flash, a button popping in.

```rust,ignore
commands.entity(sprite).tween(Tween::scale(Vec2::ONE, 0.3).from(vec2(1.3, 0.7)).ease(Ease::BackOut));
```

## Particles

A `ParticleEmitter` emits sprites over time or in bursts (`emit(n)`), with lifetimes, speeds, spread, gravity, drag, and size and colour over life. It suits dust, sparks, snow and fire; a budget caps the total.

## The camera

`CameraFollow::new(target)` on the camera follows an entity with a dead zone, smoothing and level bounds. `CameraShake` adds trauma-based shake (`add_trauma(0.5)`). `ScreenFade` and `add_state_fade` fade the screen between states.
