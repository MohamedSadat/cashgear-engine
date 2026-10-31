//! Tests that run scripts in an app, without a window.

use crate::prelude::*;
use crate::{ScriptStatus, ScriptValue};
use bevy_ecs::prelude::*;
use cg_assets::{AssetPlugin, AssetServer, Handle};
use cg_core::glam::vec2;
use cg_core::{App, Time};
use cg_physics::{Collider, KinematicBody, PhysicsPlugin, Sensor};
use cg_render::{Color, Sprite};
use cg_world::{AppWorldExt, Prefabs, State, StateScoped, Transform2D};
use core::time::Duration;

/// What scripts report back through `game.note(...)`.
#[derive(Resource, Default, Debug)]
struct Notes(Vec<String>);

fn app() -> App {
    let mut app = App::new();
    app.add_plugins(AssetPlugin {
        hot_reload: Some(false),
        ..Default::default()
    })
    // Physics brings the world (transforms, prefabs).
    .add_plugins(PhysicsPlugin::default())
    .add_plugins(ScriptPlugin {
        show_errors: false,
        ..Default::default()
    })
    .init_resource::<Notes>()
    .add_script_function("game", "note", |world, (text,): (ScriptValue,)| {
        world.resource_mut::<Notes>().0.push(text.to_string());
        Ok(())
    });
    app
}

fn script(app: &App, path: &str, source: &str) -> Handle<LuaScript> {
    app.world()
        .resource::<AssetServer>()
        .add(path, LuaScript::new(source))
}

/// Runs `steps` frames of one fixed step each.
fn run(app: &mut App, steps: u32) {
    let dt = app.world().resource::<Time>().fixed_delta();
    for _ in 0..steps {
        app.update(dt);
    }
}

fn notes(app: &mut App) -> Vec<String> {
    core::mem::take(&mut app.world_mut().resource_mut::<Notes>().0)
}

fn errors(app: &App) -> Vec<String> {
    app.world()
        .resource::<ScriptErrors>()
        .iter()
        .map(|e| e.summary().to_owned())
        .collect()
}

const COUNTER: &str = r#"
local counter = {}
function counter.on_start(self)
  self.n = 0
  game.note("start")
end
function counter.on_update(self, dt)
  self.n = self.n + 1
  game.note(("update %d %.4f step %d"):format(self.n, dt, time.step))
end
return counter
"#;

#[test]
fn hooks_start_once_then_update_every_step() {
    let mut app = app();
    let handle = script(&app, "scripts/counter.lua", COUNTER);
    app.world_mut().spawn(Script::new(handle));
    run(&mut app, 3);
    assert_eq!(
        notes(&mut app),
        [
            "start",
            "update 1 0.0167 step 1",
            "update 2 0.0167 step 2",
            "update 3 0.0167 step 3"
        ]
    );
    let status = app.world().resource::<ScriptStatus>();
    assert_eq!(status.scripts.len(), 1);
    assert_eq!(status.scripts[0].path, "scripts/counter.lua");
    assert_eq!(status.scripts[0].instances, 1);
    assert!(errors(&app).is_empty());
}

#[test]
fn every_entity_has_its_own_self_and_despawned_ones_stop() {
    let mut app = app();
    let handle = script(&app, "scripts/counter.lua", COUNTER);
    let a = app.world_mut().spawn(Script::new(handle.clone())).id();
    run(&mut app, 2);
    app.world_mut().spawn(Script::new(handle));
    run(&mut app, 1);
    let notes_now = notes(&mut app);
    assert!(notes_now.contains(&"update 3 0.0167 step 3".to_owned()));
    assert!(notes_now.contains(&"update 1 0.0167 step 3".to_owned()));
    app.world_mut().despawn(a);
    run(&mut app, 1);
    assert_eq!(notes(&mut app), ["update 2 0.0167 step 4"]);
}

#[test]
fn a_reload_keeps_self_and_calls_on_reload() {
    let mut app = app();
    let handle = script(&app, "scripts/counter.lua", COUNTER);
    app.world_mut().spawn(Script::new(handle));
    run(&mut app, 2);
    notes(&mut app);
    script(
        &app,
        "scripts/counter.lua",
        r#"
        return {
          on_reload = function(self) game.note("reloaded at " .. self.n) end,
          on_update = function(self) self.n = self.n + 10; game.note("n " .. self.n) end,
        }
        "#,
    );
    run(&mut app, 2);
    assert_eq!(notes(&mut app), ["reloaded at 2", "n 12", "n 22"]);
    let status = app.world().resource::<ScriptStatus>();
    assert_eq!(status.scripts[0].loads, 2);
}

#[test]
fn an_error_stops_the_entity_until_the_file_changes() {
    let mut app = app();
    let handle = script(
        &app,
        "scripts/broken.lua",
        "local s = {}\nfunction s.on_update(self)\n  game.note('tick')\n  local t = nil\n  return t.x\nend\nreturn s\n",
    );
    let entity = app.world_mut().spawn(Script::new(handle)).id();
    run(&mut app, 3);
    assert_eq!(notes(&mut app), ["tick"], "stopped after the first error");
    let errors_now = errors(&app);
    assert_eq!(errors_now.len(), 1);
    assert!(
        errors_now[0].starts_with("scripts/broken.lua:5: attempt to index a nil value"),
        "{errors_now:?}"
    );
    let error = app
        .world()
        .resource::<ScriptErrors>()
        .last()
        .unwrap()
        .clone();
    assert_eq!(error.entity, Some(entity));
    assert_eq!(error.during, "on_update");
    assert!(error.message.contains("stack traceback"));
    assert_eq!(app.world().resource::<ScriptStatus>().scripts[0].failed, 1);
    // A fixed file runs again.
    script(
        &app,
        "scripts/broken.lua",
        "return { on_update = function() game.note('fixed') end }",
    );
    run(&mut app, 1);
    assert_eq!(notes(&mut app), ["fixed"]);
}

#[test]
fn engine_errors_name_the_scripts_line() {
    let mut app = app();
    let handle = script(
        &app,
        "scripts/body.lua",
        "return {\n  on_update = function(self)\n    self:set_velocity(1, 2)\n  end,\n}\n",
    );
    app.world_mut().spawn(Script::new(handle));
    run(&mut app, 1);
    let errors_now = errors(&app);
    assert_eq!(errors_now.len(), 1);
    assert!(
        errors_now[0].starts_with("scripts/body.lua:3: set_velocity: entity ")
            && errors_now[0].ends_with("has no KinematicBody"),
        "{errors_now:?}"
    );
}

#[test]
fn a_file_that_does_not_compile_keeps_the_version_before() {
    let mut app = app();
    let handle = script(&app, "scripts/counter.lua", COUNTER);
    app.world_mut().spawn(Script::new(handle));
    run(&mut app, 1);
    notes(&mut app);
    script(
        &app,
        "scripts/counter.lua",
        "return { on_update = function(self) oops( end }",
    );
    run(&mut app, 1);
    assert_eq!(notes(&mut app), ["update 2 0.0167 step 2"]);
    let errors_now = errors(&app);
    assert_eq!(errors_now.len(), 1);
    assert!(
        errors_now[0].starts_with("scripts/counter.lua:1:"),
        "{errors_now:?}"
    );
    let status = app.world().resource::<ScriptStatus>();
    assert!(status.scripts[0].error.is_some());
    // A file that returns no table is an error too.
    script(&app, "scripts/counter.lua", "local x = 1");
    run(&mut app, 1);
    assert!(errors(&app)[1].contains("must return a table of hooks, not nil"));
}

#[test]
fn an_endless_loop_is_stopped() {
    let mut app = app();
    app.world_mut().remove_resource::<ScriptErrors>();
    app.init_resource::<ScriptErrors>();
    let handle = script(
        &app,
        "scripts/loop.lua",
        "return { on_update = function(self)\n  while true do end\nend }",
    );
    app.world_mut().spawn(Script::new(handle));
    run(&mut app, 1);
    let errors_now = errors(&app);
    assert_eq!(errors_now.len(), 1);
    assert!(
        errors_now[0].starts_with("scripts/loop.lua:2: ran more than 20000000 instructions"),
        "{errors_now:?}"
    );
}

#[test]
fn the_sandbox_has_no_files_or_system() {
    let mut app = app();
    let handle = script(
        &app,
        "scripts/sandbox.lua",
        r#"
        leaked = "from the sandbox file"
        return { on_start = function(self)
          for _, name in ipairs({"os", "io", "require", "package", "debug", "load", "dofile", "loadfile", "collectgarbage"}) do
            game.note(name .. " " .. tostring(_G[name]))
          end
          game.note("dump " .. tostring(string.dump))
          game.note("random " .. math.random(1000000))
        end }
        "#,
    );
    let other = script(
        &app,
        "scripts/other.lua",
        "return { on_start = function() game.note('leaked ' .. tostring(leaked)) end }",
    );
    app.world_mut().spawn(Script::new(handle));
    run(&mut app, 1);
    app.world_mut().spawn(Script::new(other));
    run(&mut app, 1);
    let notes_now = notes(&mut app);
    // `require` is the engine's own, reading script files through the asset server.
    assert!(
        notes_now.iter().any(|n| n.starts_with("require function")),
        "{notes_now:?}"
    );
    for name in [
        "os",
        "io",
        "package",
        "debug",
        "load",
        "dofile",
        "loadfile",
        "collectgarbage",
        "dump",
    ] {
        assert!(
            notes_now.contains(&format!("{name} nil")),
            "{name}: {notes_now:?}"
        );
    }
    assert!(
        notes_now.contains(&"leaked nil".to_owned()),
        "globals are per file"
    );
    // The seed is fixed: the same number every run.
    let random = notes_now
        .iter()
        .find(|n| n.starts_with("random"))
        .unwrap()
        .clone();
    let mut again = self::app();
    let handle = script(
        &again,
        "scripts/sandbox.lua",
        "return { on_start = function() game.note('random ' .. math.random(1000000)) end }",
    );
    again.world_mut().spawn(Script::new(handle));
    run(&mut again, 1);
    assert_eq!(notes(&mut again), [random]);
}

#[test]
fn the_api_works_only_in_hooks() {
    let mut app = app();
    let handle = script(&app, "scripts/top.lua", "game.note('top level')\nreturn {}");
    app.world_mut().spawn(Script::new(handle));
    run(&mut app, 1);
    let errors_now = errors(&app);
    assert_eq!(errors_now.len(), 1);
    assert!(
        errors_now[0].contains("only inside hooks"),
        "{errors_now:?}"
    );
}

#[test]
fn events_go_both_ways() {
    let mut app = app();
    let handle = script(
        &app,
        "scripts/events.lua",
        r#"
        return {
          on_event = function(self, name, data, from)
            game.note(name .. " " .. tostring(data and data.n) .. " from " .. tostring(from and from:name()))
            if name == "ping" then
              events.emit("pong", { n = data.n + 1, who = self })
              world.find("other"):send("hello", { n = 7 })
            end
          end,
        }
        "#,
    );
    let one = app
        .world_mut()
        .spawn((Name::new("one"), Script::new(handle.clone())))
        .id();
    let other = app
        .world_mut()
        .spawn((Name::new("other"), Script::new(handle)))
        .id();
    run(&mut app, 1);
    let mut events = app.world_mut().resource_mut::<ScriptEvents>();
    events.send(
        one,
        "ping",
        ScriptValue::Map([("n".to_owned(), ScriptValue::Int(1))].into()),
    );
    run(&mut app, 1);
    assert_eq!(notes(&mut app), ["ping 1 from nil", "hello 7 from one"]);
    let messages = app
        .world()
        .resource::<bevy_ecs::message::Messages<ScriptMessage>>();
    let sent: Vec<&ScriptMessage> = messages.iter_current_update_messages().collect();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].name, "pong");
    assert_eq!(sent[0].entity, Some(one));
    assert_eq!(sent[0].data.get("n"), &ScriptValue::Int(2));
    assert_eq!(sent[0].data.get("who"), &ScriptValue::Entity(one));
    app.world_mut()
        .resource_mut::<ScriptEvents>()
        .broadcast("all", ScriptValue::Nil);
    run(&mut app, 1);
    let mut notes_now = notes(&mut app);
    notes_now.sort();
    assert_eq!(notes_now, ["all nil from nil", "all nil from nil"]);
    let _ = other;
}

#[test]
fn timers_count_fixed_steps() {
    let mut app = app();
    let handle = script(
        &app,
        "scripts/timers.lua",
        r#"
        return { on_start = function(self)
          self.ticks = 0
          self:after(0.05, function(me) game.note("after at " .. time.step .. " " .. tostring(me == self)) end)
          self.every = self:every(1 / 60, function(me)
            me.ticks = me.ticks + 1
            game.note("tick " .. me.ticks)
            if me.ticks == 3 then me.every:cancel() end
          end)
          local t = timer.after(0.02, function() game.note("global") end)
          timer.after(0.01, function() game.note("never") end):cancel()
        end }
        "#,
    );
    app.world_mut().spawn(Script::new(handle));
    run(&mut app, 6);
    assert_eq!(
        notes(&mut app),
        // Timers due in the same step fire in the order they were made.
        ["tick 1", "tick 2", "global", "after at 4 true", "tick 3"]
    );
}

#[test]
fn timers_of_a_despawned_entity_are_dropped() {
    let mut app = app();
    let handle = script(
        &app,
        "scripts/late.lua",
        "return { on_start = function(self) self:after(0.1, function() game.note('late') end) end }",
    );
    let entity = app.world_mut().spawn(Script::new(handle)).id();
    run(&mut app, 1);
    app.world_mut().despawn(entity);
    run(&mut app, 10);
    assert!(notes(&mut app).is_empty());
}

#[test]
fn collisions_reach_both_scripts() {
    let mut app = app();
    let handle = script(
        &app,
        "scripts/touch.lua",
        r#"
        return {
          on_collision = function(self, other) game.note(self:name() .. " touched " .. other:name()) end,
          on_collision_end = function(self, other) game.note(self:name() .. " left " .. other:name()) end,
        }
        "#,
    );
    app.world_mut().spawn((
        Name::new("coin"),
        Script::new(handle.clone()),
        Collider::rect(10.0, 10.0),
        Sensor,
        Transform2D::from_xy(0.0, 0.0),
    ));
    let body = app
        .world_mut()
        .spawn((
            Name::new("body"),
            Script::new(handle),
            Collider::rect(10.0, 10.0),
            KinematicBody::default().with_gravity_scale(0.0),
            Transform2D::from_xy(0.0, 4.0),
        ))
        .id();
    run(&mut app, 3);
    let mut notes_now = notes(&mut app);
    notes_now.sort();
    assert_eq!(notes_now, ["body touched coin", "coin touched body"]);
    app.world_mut()
        .get_mut::<Transform2D>(body)
        .unwrap()
        .translation = vec2(100.0, 0.0);
    run(&mut app, 3);
    let mut notes_now = notes(&mut app);
    notes_now.sort();
    assert_eq!(notes_now, ["body left coin", "coin left body"]);
}

#[test]
fn entities_move_turn_tint_and_spawn() {
    let mut app = app();
    app.world_mut().resource_mut::<Prefabs>().add_ron(
        "Crate",
        r#"(components: { "Transform2D": (), "Name": "crate" })"#,
    );
    let handle = script(
        &app,
        "scripts/mover.lua",
        r#"
        return {
          on_start = function(self)
            self:set_position(10, 20)
            self:set_rotation(0.5)
            self:set_scale(2)
            self:set_flip_x(true)
            self:set_color(0xff0000)
            self.crate = world.spawn("Crate", 5, 6)
            game.note(tostring(self:has("Sprite")) .. " " .. tostring(self:has("KinematicBody")))
          end,
          on_update = function(self)
            local x, y = self:position()
            local cx, cy = self.crate:position()
            local r, g, b = self:color()
            game.note(("%g %g %g %g %s %g %g %g %s"):format(x, y, cx, cy, self.crate:name(), r, g, b, tostring(self:flip_x())))
            local ok, err = pcall(function() return self:has("Wings") end)
            game.note(tostring(ok) .. " " .. tostring(err):match("no component `Wings`"))
          end,
        }
        "#,
    );
    let entity = app
        .world_mut()
        .spawn((
            Script::new(handle),
            Transform2D::default(),
            Sprite::solid(Color::WHITE, vec2(4.0, 4.0)),
        ))
        .id();
    run(&mut app, 1);
    assert_eq!(
        notes(&mut app),
        [
            "true false",
            "10 20 5 6 crate 1 0 0 true",
            "false no component `Wings`"
        ]
    );
    let transform = app.world().get::<Transform2D>(entity).unwrap();
    assert_eq!((transform.rotation, transform.scale), (0.5, vec2(2.0, 2.0)));
}

#[test]
fn raycasts_hit_colliders() {
    let mut app = app();
    let wall = app
        .world_mut()
        .spawn((Collider::rect(10.0, 100.0), Transform2D::from_xy(50.0, 0.0)))
        .id();
    let handle = script(
        &app,
        "scripts/ray.lua",
        r#"
        return { on_update = function(self)
          -- Queries see the latest physics step, so the wall is there from step 2.
          if time.step < 2 then return end
          local hit = world.raycast(0, 0, 1, 0, 200)
          game.note(("%g %g %g %g"):format(hit.x, hit.y, hit.nx, hit.distance))
          game.note(tostring(hit.entity:id()))
          game.note(tostring(world.raycast(0, 0, -1, 0, 200)))
          game.note(tostring(#world.overlap_point(50, 0)))
        end }
        "#,
    );
    app.world_mut().spawn(Script::new(handle));
    run(&mut app, 2);
    assert_eq!(
        notes(&mut app),
        [
            "45 0 -1 45",
            (wall.to_bits() as i64).to_string().as_str(),
            "nil",
            "1"
        ]
    );
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Screen {
    Title,
    Playing,
    Secret,
}

#[test]
fn scripts_change_states_they_are_allowed_to() {
    let mut app = app();
    app.init_state(Screen::Title)
        .add_script_states(&[Screen::Title, Screen::Playing]);
    let handle = script(
        &app,
        "scripts/states.lua",
        r#"
        return { on_update = function(self)
          game.note(state.current())
          if state.current() == "Title" then state.push("Playing") end
          if time.step == 2 then
            local ok, err = pcall(state.set, "Secret")
            game.note(tostring(err):match("no state `Secret`[^)]*%)"))
          end
        end }
        "#,
    );
    app.world_mut()
        .spawn((Script::new(handle), StateScoped(Screen::Title)));
    run(&mut app, 2);
    assert_eq!(
        notes(&mut app),
        [
            "Title",
            "Playing",
            "no state `Secret` for scripts (they can enter Title, Playing)"
        ]
    );
    assert_eq!(
        *app.world().resource::<State<Screen>>().current(),
        Screen::Playing
    );
    let _ = Screen::Secret;
}

#[test]
fn game_function_errors_stop_the_hook() {
    let mut app = app();
    app.add_script_function("game", "half", |_, (n,): (u32,)| {
        if n % 2 == 1 {
            Err(format!("{n} is odd"))
        } else {
            Ok(n / 2)
        }
    });
    let handle = script(
        &app,
        "scripts/half.lua",
        "return { on_update = function()\n  game.note(game.half(8))\n  game.note(game.half(3))\nend }",
    );
    app.world_mut().spawn(Script::new(handle));
    run(&mut app, 2);
    assert_eq!(notes(&mut app), ["4"]);
    assert_eq!(errors(&app), ["scripts/half.lua:3: game.half: 3 is odd"]);
}

#[test]
fn tweens_start_from_scripts() {
    let mut app = app();
    let handle = script(
        &app,
        "scripts/tween.lua",
        r#"return { on_start = function(self)
          self:tween("scale", {2, 3}, 0.5, { from = 1, ease = "BackOut", tag = "grow" })
          local ok, err = pcall(function() self:tween("scale", 1, 1, { ease = "Bouncy" }) end)
          game.note(tostring(err):match("no easing curve called `Bouncy`"))
        end }"#,
    );
    let entity = app
        .world_mut()
        .spawn((Script::new(handle), Transform2D::default()))
        .id();
    run(&mut app, 1);
    assert_eq!(notes(&mut app), ["no easing curve called `Bouncy`"]);
    assert!(app.world().get::<cg_anim::Tweens>(entity).is_some());
}

#[test]
fn turned_off_scripts_do_not_run() {
    let mut app = App::new();
    app.add_plugins(AssetPlugin {
        hot_reload: Some(false),
        ..Default::default()
    })
    .add_plugins(ScriptPlugin {
        enabled: false,
        ..Default::default()
    })
    .init_resource::<Notes>()
    .add_script_function("game", "note", |world, (text,): (String,)| {
        world.resource_mut::<Notes>().0.push(text);
        Ok(())
    });
    assert!(!app.world().resource::<Scripting>().runs());
    let handle = script(&app, "scripts/counter.lua", COUNTER);
    app.world_mut().spawn(Script::new(handle));
    run(&mut app, 2);
    assert!(notes(&mut app).is_empty());
    let _ = Duration::ZERO;
}

#[test]
fn require_shares_a_files_result_between_files() {
    let mut app = app();
    // A counter module: both files get the same table, so they count together. (Files made
    // in code stay while their handles do; files `require` loads itself, it keeps.)
    let _counter = script(
        &app,
        "shared/counter.lua",
        "local m = { n = 0 }\nfunction m.bump() m.n = m.n + 1 return m.n end\nreturn m",
    );
    let _flag = script(&app, "shared/flag.lua", "local x = 1");
    let first = script(
        &app,
        "scripts/first.lua",
        r#"
        local counter = require("shared.counter")
        local flag = require("shared/flag.lua")
        return { on_start = function() game.note("first " .. counter.bump() .. " " .. tostring(flag)) end }
        "#,
    );
    let second = script(
        &app,
        "scripts/second.lua",
        r#"
        local counter = require("shared/counter")
        return { on_start = function() game.note("second " .. counter.bump()) end }
        "#,
    );
    app.world_mut().spawn(Script::new(first));
    app.world_mut().spawn(Script::new(second));
    run(&mut app, 1);
    let mut seen = notes(&mut app);
    seen.sort();
    assert_eq!(seen, ["first 1 true", "second 2"]);
    assert!(errors(&app).is_empty(), "{:?}", errors(&app));
}

#[test]
fn changing_a_required_file_reloads_the_files_that_require_it() {
    let mut app = app();
    let _tuning = script(&app, "shared/tuning.lua", "return { speed = 1 }");
    let handle = script(
        &app,
        "scripts/mover.lua",
        r#"
        local tuning = require("shared.tuning")
        return {
          on_reload = function(self) game.note("reloaded") end,
          on_update = function(self) game.note("speed " .. tuning.speed) end,
        }
        "#,
    );
    app.world_mut().spawn(Script::new(handle));
    run(&mut app, 1);
    assert_eq!(notes(&mut app), ["speed 1"]);
    script(&app, "shared/tuning.lua", "return { speed = 2 }");
    run(&mut app, 1);
    assert_eq!(notes(&mut app), ["reloaded", "speed 2"]);
}

#[test]
fn bad_requires_are_errors_with_the_line() {
    let mut app = app();
    let _a = script(&app, "shared/a.lua", "return require('shared.b')");
    let _b = script(&app, "shared/b.lua", "return require('shared.a')");
    let circular = script(
        &app,
        "scripts/circular.lua",
        "local a = require('shared.a')\nreturn {}",
    );
    app.world_mut().spawn(Script::new(circular));
    run(&mut app, 1);
    let errors_now = errors(&app);
    assert!(
        errors_now
            .iter()
            .any(|e| e.contains("requires itself (directly or through other files)")),
        "{errors_now:?}"
    );
    // Inside a hook, a file that is not loaded yet cannot be waited for.
    let late = script(
        &app,
        "scripts/late.lua",
        "return { on_start = function() require('shared.never') end }",
    );
    app.world_mut().spawn(Script::new(late));
    run(&mut app, 1);
    let errors_now = errors(&app);
    assert!(
        errors_now
            .iter()
            .any(|e| e.starts_with("scripts/late.lua:1:")
                && (e.contains("is not loaded yet") || e.contains("cannot load"))),
        "{errors_now:?}"
    );
}
