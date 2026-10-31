# Lua scripting

Gameplay can be written in Lua 5.4 and changed while the game runs. Scripts run on desktop and Android; **browsers run no Lua** (the Lua runtime cannot build for the web's target), so a game that ships on the web keeps Rust versions of behaviours it needs there and picks by `Scripting::runs`, as the sample platformer does. Turn scripting on with the engine's `script` feature (the template does).

## A script

A `.lua` file returns a table of hooks; a `Script` component runs it on its entity:

```lua
local patrol = {}

function patrol.on_start(self)
  self.dir = 1
  self.speed = self:props().speed or 40
end

function patrol.on_update(self, dt)
  local c = self:contacts()
  if c.wall_left or c.wall_right then self.dir = -self.dir end
  self:set_velocity(self.dir * self.speed, nil)
end

function patrol.on_collision(self, other)
  if other:has("Player") then events.emit("hurt", { by = self }) end
end

return patrol
```

```rust,ignore
commands.spawn((Script::new(server.load("scripts/patrol.lua")), KinematicBody::default(), Collider::rect(16.0, 16.0)));
```

In prefabs and Tiled properties a script is its path: `"Script": "scripts/patrol.lua"`.

Every scripted entity has its own `self` table, which keeps its fields when the file reloads. Hooks run in fixed steps (`ScriptSystems::Run` in `FixedUpdate`): `on_start`, `on_reload`, `on_event(name, data, from)`, `on_collision(other)`, `on_collision_end(other)` and `on_update(dt)`, with timers in between.

## The API

- **The entity:** `position`, `set_position`, `rotation`, `scale`, `velocity`, `set_velocity`, `contacts`, `flip_x`, `set_color`, `play` (a clip), `set_param` and `trigger` (the animator), `tween`, `props` (Tiled properties), `name`, `children`, `parent`, `has`, `send`, `despawn`, `after` and `every` (timers).
- **`world`:** `spawn(prefab, x, y)`, `find(name)`, `raycast`, `tile_at`, `overlap_point`.
- **`input`:** actions by name (`input.pressed("jump")`).
- **`audio`:** `play`, `stop`, `music`, `stop_music`.
- **`timer`, `events`, `state`, `time`, `log`.**
- **Your game's own functions:** `app.add_script_function("game", "set_checkpoint", |world, (x, y): (f32, f32)| ...)`, and components scripts may check with `add_script_component`.

`cargo doc -p cg_script --open` lists every function. `self:contacts()` refills the same table each step; copy values you want to keep.

## Shared code

`require` loads another file once and returns what it returned, the same value to every file that asks:

```lua
local tuning = require("scripts.tuning")   -- scripts/tuning.lua
```

Names with dots map to folders (`enemies.common` is `enemies/common.lua`); a name with a `/` or `.lua` is a path as it is. A required file that changes reloads the files that require it.

## The sandbox

Scripts get `string`, `table`, `math`, `utf8`, `coroutine` and the safe base functions, but no `os`, `io`, `package`, `debug`, `load` or `dofile`: they cannot touch files or the system. Each file has its own globals. `math.random` starts from a fixed seed, so runs repeat. A hook that runs too long (an endless loop) is stopped, and Lua's memory is capped.

## Errors and hot reload

Saving a script while the game runs on desktop reloads it at once; every entity running it keeps its `self` and gets `on_reload`. A file that does not compile keeps the previous version running. An error in a hook names the file and line, and stops that entity's script until the file changes. The debug overlay's Scripts window lists every script and error.

## Cost

An engine call from Lua costs about half a microsecond; a few hundred scripted entities fit easily in a frame. The `cg_bench` scene with 500 scripted bodies (each reading and writing its transform and velocity every step) runs at 2.7 ms a frame in a release build, physics included.
