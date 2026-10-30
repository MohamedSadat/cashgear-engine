// The capture test writes files on desktop only.
#![allow(clippy::disallowed_methods)]

use super::ASSETS;
use cashgear_engine::audio::AudioOutput;
use cashgear_engine::platform::input::InputPlugin;
use cashgear_engine::prelude::*;
use cashgear_engine::render::{
    CameraView, FrameView, OffscreenFrame, OffscreenRenderer, QueuedSprite, RenderOverlays,
    RenderSystems,
};
use cg_editor::{Editor, EditorPlugin, PlayMode};
use platformer::{Game, Options};

/// The platformer with the editor built in, without a window: a mock audio device, saves in
/// memory, a 1600×900 window.
fn app(editor: EditorPlugin) -> App {
    let mut app = App::new();
    app.add_plugins(AssetPlugin {
        root: Some(ASSETS.into()),
        hot_reload: Some(false),
        ..default()
    })
    .add_plugins(InputPlugin { gamepads: false })
    .add_plugins(StoragePlugin {
        memory: true,
        ..default()
    })
    .add_plugins(TiledPlugin)
    .add_plugins(PhysicsPlugin::default())
    .add_plugins(AnimPlugin::default())
    .add_plugins(AudioPlugin {
        output: AudioOutput::Mock,
        settings_key: None,
        ..default()
    })
    .add_plugins(UiPlugin {
        locale: Some(LocaleSettings::new(&["en", "ar"])),
        ..default()
    })
    .add_plugins(ScriptPlugin {
        show_errors: false,
        ..default()
    })
    .add_plugins(editor)
    .add_plugins(Game {
        options: Options::default(),
    })
    .add_message::<Lifecycle>();
    app.insert_resource(Window::with_size(UVec2::new(1600, 900), 1.0));
    app
}

fn frame(app: &mut App) {
    app.update(core::time::Duration::from_millis(16));
}

#[test]
fn the_editor_demo_edits_saves_plays_and_stops() {
    let save = std::env::temp_dir().join(format!("platformer_editor_test_{}", std::process::id()));
    let mut editor = EditorPlugin::new(PlayMode::Game).open("maps/level1.tmx");
    editor.demo = true;
    editor.save_dir = Some(save.clone());
    let mut app = app(editor);
    let mut exit = None;
    for _ in 0..4000 {
        frame(&mut app);
        if let Some(code) = app.should_exit() {
            exit = Some(code);
            break;
        }
    }
    let _ = std::fs::remove_dir_all(save);
    assert_eq!(
        exit,
        Some(AppExit::Success),
        "the demo failed; its log says why"
    );
    // The game's own files are untouched.
    let original = std::fs::read_to_string(format!("{ASSETS}/maps/level1.tmx")).unwrap();
    assert!(original.contains(r#"<property name="speed" type="int" value="24"/>"#));
}

#[derive(Resource, Default)]
struct Snapshot {
    world: Vec<QueuedSprite>,
    screen: Vec<QueuedSprite>,
    view: CameraView,
}

fn snapshot(sprites: Res<Sprites>, view: Res<FrameView>, mut snapshot: ResMut<Snapshot>) {
    snapshot.world = sprites.queued().to_vec();
    snapshot.screen = sprites.screen_queued().to_vec();
    snapshot.view = view.0.clone();
}

/// Renders the editor offscreen, egui panels included, and saves PNGs for people to look
/// at: `cargo test -p platformer_editor -- --ignored capture_editor` writes them to
/// `CG_CAPTURE_DIR` (default `target/editor-captures`).
#[test]
#[ignore = "needs a GPU; writes PNGs for people to look at"]
fn capture_editor() {
    let dir = std::env::var("CG_CAPTURE_DIR").unwrap_or_else(|_| {
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../target/editor-captures"
        )
        .into()
    });
    std::fs::create_dir_all(&dir).unwrap();
    let mut renderer = pollster::block_on(OffscreenRenderer::new()).expect("a GPU");
    let mut app = app(EditorPlugin::new(PlayMode::Game).open("maps/level1.tmx"));
    app.init_resource::<Snapshot>().add_systems(
        Render,
        snapshot
            .after(RenderSystems::Queue)
            .before(RenderSystems::Draw),
    );
    let mut shoot = |app: &mut App, name: &str| {
        let world = app.world_mut();
        let window = world.resource::<Window>().clone();
        let mut overlays = world.remove_resource::<RenderOverlays>().unwrap();
        let snapshot = world.resource::<Snapshot>();
        let mut sprites = Sprites::default();
        sprites.extend(snapshot.world.iter().copied());
        for sprite in &snapshot.screen {
            sprites.push_screen(*sprite);
        }
        let image = renderer
            .render_frame(
                OffscreenFrame {
                    size: window.physical_size(),
                    scale_factor: window.scale_factor() as f32,
                    area: Some(window.safe_area()),
                },
                &snapshot.view,
                world.resource::<Assets<Image>>(),
                &sprites,
                &DebugShapes::default(),
                Some(&mut overlays),
            )
            .unwrap();
        world.insert_resource(overlays);
        std::fs::write(format!("{dir}/{name}.png"), image.to_png().unwrap()).unwrap();
    };
    for _ in 0..30 {
        frame(&mut app);
    }
    shoot(&mut app, "map");
    // Select a walker, with the move gizmo and its properties in the inspector.
    {
        let world = app.world_mut();
        let id = 6;
        world.resource_mut::<Editor>().select_object(id);
    }
    for _ in 0..3 {
        frame(&mut app);
    }
    shoot(&mut app, "selected");
    app.world_mut().resource_mut::<Editor>().show_palette();
    for _ in 0..3 {
        frame(&mut app);
    }
    shoot(&mut app, "palette");
    app.world_mut().resource_mut::<Editor>().request_play();
    for _ in 0..90 {
        frame(&mut app);
    }
    shoot(&mut app, "playing");
    app.world_mut().resource_mut::<Editor>().request_stop();
    for _ in 0..10 {
        frame(&mut app);
    }
    // A prefab: the checkpoint flag's sensor and script.
    let mut editor = app.world_mut().resource_mut::<Editor>();
    editor.request_open("prefabs/checkpoint_sparks.prefab.ron");
    for _ in 0..20 {
        frame(&mut app);
    }
    app.world_mut().resource_mut::<Editor>().select_node(&[]);
    for _ in 0..3 {
        frame(&mut app);
    }
    shoot(&mut app, "prefab");
}
