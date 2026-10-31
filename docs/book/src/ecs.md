# The app, ECS and the game loop

A game is an `App`: an ECS world (from [`bevy_ecs`](https://docs.rs/bevy_ecs)), the schedules that run every frame, and plugins that add systems and resources to them.

```rust,ignore
use cashgear_engine::prelude::*;

#[derive(Component)]
struct Spin(f32);

fn setup(mut commands: Commands) {
    commands.spawn(Camera2D::default());
    commands.spawn((
        Sprite::solid(Color::hex(0xffbe0b), vec2(32.0, 32.0)),
        Transform2D::from_xy(0.0, 0.0),
        Spin(2.0),
    ));
}

fn spin(time: Res<Time>, mut query: Query<(&mut Transform2D, &Spin)>) {
    for (mut transform, spin) in &mut query {
        transform.rotate(spin.0 * time.fixed_delta_secs());
    }
}

fn main() -> AppExit {
    App::new()
        .add_plugins(DefaultPlugins::default())
        .add_systems(Startup, setup)
        .add_systems(FixedUpdate, spin)
        .run()
}
```

`DefaultPlugins` adds the whole engine; its fields configure each part (`window`, `render`, `assets`, `physics`, `audio`, `ui`, `storage`...). Entities are spawned with components, systems are functions of queries and resources, and messages (`MessageWriter`, `MessageReader`) pass events between systems.

## The frame

Each frame runs these schedules in order:

| Schedule | What runs there |
| --- | --- |
| `Startup` | Once, in the first frame, after the window exists |
| `First` | Asset loading, message buffers |
| `PreUpdate` | Input and actions, UI pointers and focus, Tiled maps being built |
| `StateTransition` | State changes and their `OnEnter`/`OnExit` schedules |
| `FixedPreUpdate`, `FixedUpdate`, `FixedPostUpdate` | 0 or more times: gameplay, then physics and animation |
| `Update` | Per-frame effects: particles, tweens, menus |
| `PostUpdate` | Transforms, visibility, the UI layout, audio |
| `Render` | Queueing sprites, then drawing |
| `Last` | Saving settings, clearing input |

**Gameplay goes in `FixedUpdate`.** It runs at 60 steps a second whatever the frame rate (a frame can run several steps, or none), so a game behaves the same on a 30 Hz phone and a 144 Hz monitor, and a bot's run is repeatable. Give moving entities `Interpolated` and the renderer draws them blended between steps, so they move smoothly on fast screens. Physics runs in `FixedPostUpdate`, after your gameplay, and its results (contacts, collision messages) are ready in the next `FixedUpdate`.

`Time` has the frame's delta, the fixed delta (`fixed_delta_secs()`), elapsed time, the frame and step counts and a time scale. Setting the scale to 0 pauses fixed steps; menus keep working on real time.

## Transforms and hierarchy

`Transform2D` is an entity's position, rotation (radians, counter-clockwise) and scale relative to its parent (`ChildOf`); `GlobalTransform2D` is computed from it in `PostUpdate` and around fixed steps. A child moves, turns and hides with its parent, and despawning a parent despawns its children.

## States

A game's states are its own enum, kept as a stack:

```rust,ignore
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Screen { Loading, Title, Playing, Paused }

app.init_state(Screen::Loading)
    .add_loading_state(Screen::Loading, Screen::Title)
    .add_systems(OnEnter(Screen::Playing), spawn_level)
    .add_systems(FixedUpdate, move_player.run_if(in_state(Screen::Playing)));
```

`NextState<S>` changes it: `push` (a pause menu over the level), `pop`, `replace` and `set` (replace the whole stack). `OnEnter`, `OnExit`, `OnPause` and `OnResume` run on changes; `in_state` is true for the top state only, `state_in_stack` for any. Entities with `StateScoped(state)` are despawned when that state exits. `add_state_fade` fades the screen out and in around changes.

## Prefabs

A prefab is a RON file that lists components by name, with children:

```ron
(
    components: {
        "Sprite": (texture: "images/coin.png"),
        "Collider": (shape: Circle(6)),
        "Sensor": (),
    },
    children: [],
)
```

`app.load_prefab("Coin", "prefabs/coin.prefab.ron")` loads one (and reloads it when it changes); `commands.spawn_prefab("Coin")` spawns it. Your own components join with `app.register_component::<T>("Name")` (any `Clone + Serialize + Deserialize` component). Tiled objects whose class names a prefab get it automatically; see [Tiled maps](tiled.md).

## Plugins

A plugin is a type with a `build(&self, app: &mut App)` method. Write your game as one (or several), so tests and tools can build it without a window, as the template does.
