// Tests write real files into a temporary asset folder.
#![allow(clippy::disallowed_methods)]

use super::*;
use cg_core::prelude::*;
use cg_world::prelude::*;
use core::time::Duration;
use std::path::PathBuf;

/// A text file as an asset.
#[derive(Debug, PartialEq)]
struct Text(String);
impl Asset for Text {}

struct TextLoader;

impl AssetLoader for TextLoader {
    type Asset = Text;

    fn extensions(&self) -> &[&str] {
        &["txt"]
    }

    fn load<'a>(&'a self, ctx: &'a mut LoadContext) -> BoxedFuture<'a, Result<Text, BoxError>> {
        Box::pin(async move {
            let text = String::from_utf8(ctx.bytes().to_vec())?;
            if text.starts_with("bad") {
                return Err("the file says it is bad".into());
            }
            Ok(Text(text))
        })
    }
}

/// A list of other text files (one path per line, relative to the list), loaded as
/// dependencies, plus the text of `header.txt` next to it, read as part of the list.
struct List {
    header: String,
    items: Vec<Handle<Text>>,
}
impl Asset for List {}

struct ListLoader;

impl AssetLoader for ListLoader {
    type Asset = List;

    fn extensions(&self) -> &[&str] {
        &["list"]
    }

    fn load<'a>(&'a self, ctx: &'a mut LoadContext) -> BoxedFuture<'a, Result<List, BoxError>> {
        Box::pin(async move {
            let text = String::from_utf8(ctx.bytes().to_vec())?;
            let header_path = ctx.resolve("header.txt");
            let header = String::from_utf8(ctx.read(&header_path).await?)?;
            let items = text
                .lines()
                .map(|line| {
                    let path = ctx.resolve(line);
                    ctx.load(path)
                })
                .collect();
            Ok(List { header, items })
        })
    }
}

/// A folder of files under the system's temporary folder, removed when dropped.
struct Folder(PathBuf);

impl Folder {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("cg_assets_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn write(&self, path: &str, text: &str) {
        let full = self.0.join(path);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(full, text).unwrap();
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn app(folder: &Folder) -> App {
    let mut app = App::new();
    app.add_plugins(AssetPlugin {
        root: Some(folder.0.to_string_lossy().into_owned()),
        hot_reload: Some(false),
        ..default()
    })
    .init_asset::<Text>()
    .init_asset::<List>()
    .register_loader(TextLoader)
    .register_loader(ListLoader);
    app
}

fn server(app: &App) -> AssetServer {
    app.world().resource::<AssetServer>().clone()
}

fn text(app: &App, handle: &Handle<Text>) -> Option<String> {
    let texts = app.world().resource::<Assets<Text>>();
    texts.get(handle).map(|text| text.0.clone())
}

#[test]
fn loads_arrive_in_the_next_update_and_share_handles() {
    let folder = Folder::new("basic");
    folder.write("greeting.txt", "hello");
    let mut app = app(&folder);
    let server = server(&app);
    let handle: Handle<Text> = server.load("./greeting.txt");
    assert!(handle.is_strong());
    assert_eq!(server.load_state(&handle), LoadState::Loading);
    assert_eq!(server.load::<Text>("greeting.txt"), handle);
    app.update(Duration::ZERO);
    assert_eq!(server.load_state(&handle), LoadState::Loaded);
    assert!(server.is_loaded(&handle));
    assert_eq!(text(&app, &handle).as_deref(), Some("hello"));
    assert_eq!(server.path(&handle).as_deref(), Some("greeting.txt"));
    assert_eq!(server.get_handle::<Text>("greeting.txt"), Some(handle));
}

#[test]
fn failures_are_kept_with_their_reason() {
    let folder = Folder::new("failures");
    folder.write("bad.txt", "bad news");
    folder.write("data.bin", "?");
    let mut app = app(&folder);
    let server = server(&app);
    let missing: Handle<Text> = server.load("missing.txt");
    let bad: Handle<Text> = server.load("bad.txt");
    let unknown: Handle<Text> = server.load("data.bin");
    app.update(Duration::ZERO);
    for handle in [&missing, &bad, &unknown] {
        assert_eq!(server.load_state(handle), LoadState::Failed);
        assert_eq!(text(&app, handle), None);
    }
    assert_eq!(
        server.load_error(&missing),
        Some(AssetError::NotFound {
            path: "missing.txt".into()
        })
    );
    assert_eq!(
        server.load_error(&bad).unwrap().to_string(),
        "could not load `bad.txt`: the file says it is bad"
    );
    assert!(matches!(
        server.load_error(&unknown),
        Some(AssetError::NoLoader { .. })
    ));
}

#[test]
fn dependencies_count_towards_loaded() {
    let folder = Folder::new("dependencies");
    folder.write("lists/header.txt", "Shopping");
    folder.write("lists/food.list", "../items/apple.txt\n../items/pear.txt");
    folder.write("items/apple.txt", "apple");
    folder.write("items/pear.txt", "pear");
    folder.write("lists/broken.list", "../items/apple.txt\n../items/plum.txt");
    let mut app = app(&folder);
    let server = server(&app);
    let food: Handle<List> = server.load("lists/food.list");
    let broken: Handle<List> = server.load("lists/broken.list");
    app.update(Duration::ZERO);

    assert!(server.is_loaded(&food));
    let lists = app.world().resource::<Assets<List>>();
    let list = lists.get(&food).unwrap();
    assert_eq!(list.header, "Shopping");
    let names: Vec<_> = list.items.iter().map(|item| text(&app, item)).collect();
    assert_eq!(names, [Some("apple".into()), Some("pear".into())]);

    // The broken list itself loaded, but one of its items did not.
    assert_eq!(server.load_state(&broken), LoadState::Loaded);
    assert_eq!(server.recursive_load_state(&broken), LoadState::Failed);
}

#[test]
fn dropping_the_last_handle_unloads_the_asset() {
    let folder = Folder::new("unload");
    folder.write("a.txt", "first");
    let mut app = app(&folder);
    let server = server(&app);
    let handle: Handle<Text> = server.load("a.txt");
    app.update(Duration::ZERO);
    let id = handle.id();
    drop(handle);
    app.update(Duration::ZERO);
    assert!(!app.world().resource::<Assets<Text>>().contains(id));
    assert_eq!(server.load_state(id), LoadState::NotLoaded);

    folder.write("a.txt", "second");
    let again: Handle<Text> = server.load("a.txt");
    app.update(Duration::ZERO);
    assert_eq!(text(&app, &again).as_deref(), Some("second"));
}

#[test]
fn reloads_replace_assets_and_failed_reloads_keep_them() {
    let folder = Folder::new("reload");
    folder.write("lists/header.txt", "Before");
    folder.write("lists/x.list", "a.txt");
    folder.write("lists/a.txt", "one");
    let mut app = app(&folder);
    let server = server(&app);
    let list: Handle<List> = server.load("lists/x.list");
    let a: Handle<Text> = server.load("lists/a.txt");
    app.update(Duration::ZERO);
    let version = app.world().resource::<Assets<Text>>().version(&a);

    folder.write("lists/a.txt", "two");
    assert_eq!(server.reload("lists/a.txt"), 1);
    app.update(Duration::ZERO);
    assert_eq!(text(&app, &a).as_deref(), Some("two"));
    assert_ne!(app.world().resource::<Assets<Text>>().version(&a), version);

    folder.write("lists/a.txt", "bad edit");
    server.reload("lists/a.txt");
    app.update(Duration::ZERO);
    assert_eq!(text(&app, &a).as_deref(), Some("two"));
    assert_eq!(server.load_state(&a), LoadState::Loaded);

    // The header is read by the list loader, so changing it reloads the list.
    folder.write("lists/header.txt", "After");
    assert_eq!(server.reload("lists/header.txt"), 1);
    app.update(Duration::ZERO);
    let lists = app.world().resource::<Assets<List>>();
    assert_eq!(lists.get(&list).unwrap().header, "After");
}

#[test]
fn events_report_additions_changes_and_removals() {
    let folder = Folder::new("events");
    folder.write("a.txt", "one");
    let mut app = app(&folder);
    let server = server(&app);
    let handle: Handle<Text> = server.load("a.txt");
    let id = handle.id();
    app.update(Duration::ZERO);
    server.reload("a.txt");
    app.update(Duration::ZERO);
    drop(handle);
    app.update(Duration::ZERO);
    let messages = app.world().resource::<Messages<AssetEvent<Text>>>();
    let mut cursor = messages.get_cursor();
    let seen: Vec<_> = cursor.read(messages).copied().collect();
    assert!(seen.ends_with(&[AssetEvent::Modified { id }, AssetEvent::Removed { id }]));
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
enum Phase {
    Loading,
    Playing,
}

#[test]
fn loading_states_wait_for_their_group() {
    let folder = Folder::new("loading_state");
    folder.write("a.txt", "a");
    let mut app = app(&folder);
    app.world()
        .resource::<AssetServer>()
        .set_simulated_delay(0.5);
    app.init_state(Phase::Loading)
        .add_loading_state(Phase::Loading, Phase::Playing);
    let handle: Handle<Text> = server(&app).load("a.txt");
    app.world_mut().resource_mut::<LoadingAssets>().add(&handle);

    let current = |app: &App| app.world().resource::<State<Phase>>().current().clone();
    let progress = |app: &App| {
        app.world()
            .resource::<LoadingAssets>()
            .progress(app.world().resource::<AssetServer>())
    };
    app.update(Duration::from_millis(100));
    assert_eq!(progress(&app).fraction(), 0.0);
    app.update(Duration::from_millis(300));
    assert_eq!(current(&app), Phase::Loading);
    // The delay has passed: the asset arrives, and the state moves on a frame later.
    app.update(Duration::from_millis(300));
    assert!(progress(&app).done());
    app.update(Duration::from_millis(16));
    assert_eq!(current(&app), Phase::Playing);
}

#[test]
fn prefab_files_are_spawnable_and_follow_changes() {
    let folder = Folder::new("prefabs");
    folder.write(
        "prefabs/dot.prefab.ron",
        r#"(components: { "Transform2D": (translation: (1, 2)) })"#,
    );
    let mut app = app(&folder);
    app.load_prefab("dot", "prefabs/dot.prefab.ron");
    app.update(Duration::ZERO);
    let spawn = |app: &mut App| {
        let world = app.world_mut();
        let entity = world.spawn_empty().id();
        cg_world::insert_prefab(world, entity, "dot").unwrap();
        world.get::<Transform2D>(entity).unwrap().translation
    };
    assert_eq!(spawn(&mut app), vec2(1.0, 2.0));

    folder.write(
        "prefabs/dot.prefab.ron",
        r#"(components: { "Transform2D": (translation: (3, 4)) })"#,
    );
    server(&app).reload("prefabs/dot.prefab.ron");
    app.update(Duration::ZERO);
    assert_eq!(spawn(&mut app), vec2(3.0, 4.0));
}

#[test]
fn assets_added_under_a_path_are_found_by_loads() {
    let folder = Folder::new("added");
    let mut app = app(&folder);
    let server = server(&app);
    let made = server.add("generated/note.txt", Text("made in code".into()));
    assert_eq!(server.load::<Text>("generated/note.txt"), made);
    app.update(Duration::ZERO);
    assert!(server.is_loaded(&made));
    assert_eq!(text(&app, &made).as_deref(), Some("made in code"));
}

#[test]
#[should_panic(expected = "is not registered")]
fn loading_an_unregistered_type_panics() {
    struct Unknown;
    impl Asset for Unknown {}
    let folder = Folder::new("unregistered");
    let app = app(&folder);
    let _: Handle<Unknown> = server(&app).load("x.unknown");
}
