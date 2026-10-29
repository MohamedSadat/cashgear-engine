//! Localization: [`Strings`] tables in RON, one per language, the current language in
//! [`Locale`], and [`Localized`] text that follows it.

use crate::Text;
use crate::font::Font;
use bevy_ecs::prelude::*;
use cg_assets::{AssetLoader, Assets, BoxError, BoxedFuture, Handle, LoadContext};
use cg_platform::storage::Storage;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// One language's strings, keyed by id, loaded from a `.strings.ron` file:
///
/// ```ron
/// (
///     language: "ar",
///     name: "العربية",
///     right_to_left: true,
///     font: Some("../fonts/arabic.font.ron"),
///     strings: {
///         "menu.play": "العب",
///         "hud.coins": "العملات: {count}",
///     },
/// )
/// ```
///
/// `name` is the language's own name, for a language menu. `font` (relative to the file)
/// replaces the font of every [`Localized`] text while the language is in use, and
/// `font_scale` scales their size, for scripts the game's own font lacks.
#[derive(Debug, Clone, PartialEq)]
pub struct Strings {
    /// The language code, such as `en` or `ar`.
    pub language: String,
    /// The language's name in itself.
    pub name: String,
    /// Whether the language is written right to left; layouts mirror.
    pub right_to_left: bool,
    /// The font for localized text, if the language needs its own.
    pub font: Option<Handle<Font>>,
    /// Scales the size of localized text.
    pub font_scale: f32,
    /// The strings, by id.
    pub strings: HashMap<String, String>,
}

impl cg_assets::Asset for Strings {}

/// The contents of a `.strings.ron` file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StringsFile {
    /// See [`Strings::language`].
    pub language: String,
    /// See [`Strings::name`].
    #[serde(default)]
    pub name: String,
    /// See [`Strings::right_to_left`].
    #[serde(default)]
    pub right_to_left: bool,
    /// The font's path, relative to the strings file.
    #[serde(default)]
    pub font: Option<String>,
    /// See [`Strings::font_scale`].
    #[serde(default = "one")]
    pub font_scale: f32,
    /// See [`Strings::strings`].
    #[serde(default)]
    pub strings: HashMap<String, String>,
}

fn one() -> f32 {
    1.0
}

/// Loads `.strings.ron` files as [`Strings`]. `UiPlugin` registers it.
#[derive(Debug, Default)]
pub struct StringsLoader;

impl AssetLoader for StringsLoader {
    type Asset = Strings;

    fn extensions(&self) -> &[&str] {
        &["strings.ron"]
    }

    fn load<'a>(&'a self, ctx: &'a mut LoadContext) -> BoxedFuture<'a, Result<Strings, BoxError>> {
        Box::pin(async move {
            let file: StringsFile = ron::de::from_bytes(ctx.bytes())?;
            let font = file.font.map(|path| {
                let path = ctx.resolve(&path);
                ctx.load::<Font>(path)
            });
            Ok(Strings {
                name: if file.name.is_empty() {
                    file.language.clone()
                } else {
                    file.name
                },
                language: file.language,
                right_to_left: file.right_to_left,
                font,
                font_scale: file.font_scale,
                strings: file.strings,
            })
        })
    }
}

/// Which languages the game has and where their strings are. Set it in
/// [`UiPlugin::locale`](crate::UiPlugin::locale).
#[derive(Debug, Clone, PartialEq)]
pub struct LocaleSettings {
    /// The language codes, in the order a language menu lists them.
    pub languages: Vec<String>,
    /// The path of each language's strings, with `{lang}` for its code.
    pub path: String,
    /// The language to start in, and to take strings from when the current language
    /// lacks them.
    pub default_language: String,
    /// The [`Storage`] key the chosen language is kept under between runs; `None` not to
    /// keep it.
    pub storage_key: Option<String>,
}

impl Default for LocaleSettings {
    fn default() -> Self {
        Self {
            languages: vec!["en".into()],
            path: "strings/{lang}.strings.ron".into(),
            default_language: "en".into(),
            storage_key: Some("language".into()),
        }
    }
}

impl LocaleSettings {
    /// Settings for these languages, the first being the default, with strings at
    /// `strings/{lang}.strings.ron`.
    pub fn new(languages: &[&str]) -> Self {
        Self {
            languages: languages.iter().map(|l| (*l).to_owned()).collect(),
            default_language: languages.first().copied().unwrap_or("en").to_owned(),
            ..Self::default()
        }
    }
}

/// The current language and its strings.
///
/// ```
/// use cg_core::prelude::*;
/// use cg_ui::Locale;
///
/// fn greet(locale: Res<Locale>) {
///     info!("{}", locale.format("hud.coins", &[("count", "3")]));
/// }
///
/// fn switch(mut locale: ResMut<Locale>) {
///     locale.set_language("ar");
/// }
/// ```
///
/// A string the current language lacks comes from the default language; one neither has
/// shows as its id.
#[derive(Resource, Debug, Clone, Default)]
pub struct Locale {
    files: Vec<(String, Handle<Strings>)>,
    current: String,
    fallback: String,
    table: HashMap<String, String>,
    names: HashMap<String, String>,
    rtl: bool,
    font: Option<Handle<Font>>,
    font_scale: f32,
    revision: u64,
    built: Option<(String, u64)>,
}

impl Locale {
    /// A locale over strings files loaded elsewhere, starting in `language`.
    pub fn new(
        files: impl IntoIterator<Item = (String, Handle<Strings>)>,
        language: &str,
        fallback: &str,
    ) -> Self {
        Self {
            files: files.into_iter().collect(),
            current: language.to_owned(),
            fallback: fallback.to_owned(),
            font_scale: 1.0,
            ..Self::default()
        }
    }

    /// The current language's code.
    pub fn language(&self) -> &str {
        &self.current
    }

    /// Switches language. Returns `false` (and changes nothing) for a language the game
    /// does not have.
    pub fn set_language(&mut self, language: &str) -> bool {
        if !self.files.iter().any(|(code, _)| code == language) {
            return false;
        }
        if self.current != language {
            self.current = language.to_owned();
        }
        true
    }

    /// The language codes, in the order of the settings.
    pub fn languages(&self) -> impl Iterator<Item = &str> {
        self.files.iter().map(|(code, _)| code.as_str())
    }

    /// A language's own name (its code until its strings load).
    pub fn language_name<'a>(&'a self, language: &'a str) -> &'a str {
        self.names.get(language).map_or(language, String::as_str)
    }

    /// A string, if the current or default language has it.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.table.get(key).map(String::as_str)
    }

    /// A string, or its id when no language has it.
    pub fn text<'a>(&'a self, key: &'a str) -> &'a str {
        self.get(key).unwrap_or(key)
    }

    /// A string with `{name}` placeholders filled from `args`. `{{` and `}}` are literal
    /// braces.
    pub fn format(&self, key: &str, args: &[(&str, &str)]) -> String {
        fill(self.text(key), args)
    }

    /// Whether the current language is written right to left.
    pub fn is_rtl(&self) -> bool {
        self.rtl
    }

    /// The font the current language asks for, if any.
    pub fn font(&self) -> Option<&Handle<Font>> {
        self.font.as_ref()
    }

    /// The current language's scale for text size.
    pub fn font_scale(&self) -> f32 {
        self.font_scale
    }

    /// Changes whenever the language or its strings change.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Rebuilds the merged table when the language or a strings file changed.
    pub(crate) fn rebuild(&mut self, strings: &Assets<Strings>) {
        let key = (self.current.clone(), strings.revision());
        if self.built.as_ref() == Some(&key) {
            return;
        }
        self.built = Some(key);
        let find = |code: &str| {
            self.files
                .iter()
                .find(|(c, _)| c == code)
                .and_then(|(_, handle)| strings.get(handle.id()))
        };
        let mut table = HashMap::new();
        if let Some(fallback) = find(&self.fallback) {
            table.extend(fallback.strings.clone());
        }
        let current = find(&self.current);
        if let Some(current) = current {
            table.extend(current.strings.clone());
        }
        self.rtl = current.is_some_and(|s| s.right_to_left);
        self.font = current.and_then(|s| s.font.clone());
        self.font_scale = current.map_or(1.0, |s| s.font_scale);
        self.names = self
            .files
            .iter()
            .filter_map(|(code, handle)| {
                strings
                    .get(handle.id())
                    .map(|s| (code.clone(), s.name.clone()))
            })
            .collect();
        self.table = table;
        self.revision += 1;
    }
}

/// Fills `{name}` placeholders.
fn fill(text: &str, args: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find(['{', '}']) {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        if tail.starts_with("{{") || tail.starts_with("}}") {
            out.push_str(&tail[..1]);
            rest = &tail[2..];
        } else if let Some(stripped) = tail.strip_prefix('{')
            && let Some(end) = stripped.find('}')
        {
            let name = &stripped[..end];
            match args.iter().find(|(n, _)| *n == name) {
                Some((_, value)) => out.push_str(value),
                None => out.push_str(&tail[..end + 2]),
            }
            rest = &stripped[end + 1..];
        } else {
            out.push_str(&tail[..1]);
            rest = &tail[1..];
        }
    }
    out.push_str(rest);
    out
}

/// Text that shows a string of the current language: the [`Text`]'s value is set from
/// `key` (with `args` filled in) whenever the language or its strings change, and its font
/// and size follow the language's.
///
/// ```
/// use cg_core::prelude::*;
/// use cg_ui::{Localized, Text};
///
/// fn spawn(mut commands: Commands) {
///     commands.spawn((Text::default(), Localized::new("menu.play")));
///     commands.spawn((Text::default(), Localized::new("hud.coins").with_arg("count", "0")));
/// }
/// ```
///
/// In prefabs: `"Localized": (key: "menu.play")`.
#[derive(Component, Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[require(Text)]
#[serde(default)]
pub struct Localized {
    /// The string's id.
    pub key: String,
    /// Values for the string's `{name}` placeholders.
    pub args: Vec<(String, String)>,
    #[serde(skip)]
    base: Option<(Handle<Font>, f32)>,
    #[serde(skip)]
    applied: Option<u64>,
}

impl Localized {
    /// The string with this id.
    pub fn new(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            ..Self::default()
        }
    }

    /// The same, with a value for a placeholder.
    pub fn with_arg(mut self, name: impl Into<String>, value: impl ToString) -> Self {
        self.set_arg(name, value);
        self
    }

    /// Sets a placeholder's value.
    pub fn set_arg(&mut self, name: impl Into<String>, value: impl ToString) {
        let name = name.into();
        let value = value.to_string();
        match self.args.iter_mut().find(|(n, _)| *n == name) {
            Some((_, v)) => {
                if *v != value {
                    *v = value;
                }
            }
            None => self.args.push((name, value)),
        }
    }
}

/// Loads the configured languages and picks the starting one.
pub(crate) fn setup_locale(world: &mut World, settings: &LocaleSettings) -> Locale {
    let Some(server) = world.get_resource::<cg_assets::AssetServer>().cloned() else {
        return Locale::default();
    };
    let files: Vec<_> = settings
        .languages
        .iter()
        .map(|code| {
            let handle = server.load::<Strings>(settings.path.replace("{lang}", code));
            if let Some(mut loading) = world.get_resource_mut::<cg_assets::LoadingAssets>() {
                loading.add(&handle);
            }
            (code.clone(), handle)
        })
        .collect();
    let saved = match (&settings.storage_key, world.get_resource::<Storage>()) {
        (Some(key), Some(storage)) => storage.read(key).ok().flatten(),
        _ => None,
    };
    let start = saved
        .map(|s| s.trim().to_owned())
        .filter(|code| settings.languages.contains(code))
        .unwrap_or_else(|| settings.default_language.clone());
    Locale::new(files, &start, &settings.default_language)
}

/// Rebuilds the locale's table. Runs in `PostUpdate` before localized text is applied.
pub(crate) fn update_locale(mut locale: ResMut<Locale>, strings: Res<Assets<Strings>>) {
    let before = locale.revision;
    locale.bypass_change_detection().rebuild(&strings);
    if locale.revision != before {
        locale.set_changed();
    }
}

/// Sets localized texts' strings, fonts and sizes from the locale.
pub(crate) fn apply_localized(
    locale: Res<Locale>,
    mut texts: Query<(&mut Localized, &mut Text)>,
    mut missing: Local<HashSet<String>>,
) {
    for (mut localized, mut text) in &mut texts {
        let stale = localized.applied != Some(locale.revision) || localized.is_changed();
        if !stale {
            continue;
        }
        let localized = localized.bypass_change_detection();
        localized.applied = Some(locale.revision);
        let base = localized
            .base
            .get_or_insert_with(|| (text.style.font.clone(), text.style.size))
            .clone();
        if locale.get(&localized.key).is_none()
            && !locale.files.is_empty()
            && locale.built.is_some()
            && missing.insert(localized.key.clone())
        {
            log::warn!(
                "no string `{}` in language `{}`",
                localized.key,
                locale.language()
            );
        }
        let args: Vec<(&str, &str)> = localized
            .args
            .iter()
            .map(|(n, v)| (n.as_str(), v.as_str()))
            .collect();
        let value = locale.format(&localized.key, &args);
        let font = locale.font().cloned().unwrap_or(base.0);
        let size = base.1 * locale.font_scale();
        if text.value != value {
            text.value = value;
        }
        if text.style.font != font {
            text.style.font = font;
        }
        if text.style.size != size {
            text.style.size = size;
        }
    }
}

/// The language last saved, so it is written only when it changes.
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub(crate) struct SavedLanguage {
    pub(crate) key: String,
    pub(crate) language: String,
}

/// Saves the chosen language when it changes.
pub(crate) fn save_language(
    locale: Res<Locale>,
    storage: Option<Res<Storage>>,
    mut saved: ResMut<SavedLanguage>,
) {
    if saved.language == locale.language() {
        return;
    }
    saved.language = locale.language().to_owned();
    if let Some(storage) = storage
        && let Err(err) = storage.write(&saved.key, locale.language())
    {
        log::warn!("cannot save the language: {err}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholders_are_filled_and_braces_escaped() {
        assert_eq!(fill("Coins: {count}", &[("count", "12")]), "Coins: 12");
        assert_eq!(fill("{a}{b}!", &[("a", "x"), ("b", "y")]), "xy!");
        assert_eq!(fill("{{literal}} {missing}", &[]), "{literal} {missing}");
        assert_eq!(fill("unclosed {brace", &[]), "unclosed {brace");
        assert_eq!(fill("العملات: {count}", &[("count", "٣")]), "العملات: ٣");
    }

    #[test]
    fn the_table_falls_back_to_the_default_language() {
        let mut assets = Assets::<Strings>::default();
        let strings = |language: &str, rtl, pairs: &[(&str, &str)]| Strings {
            language: language.into(),
            name: language.to_uppercase(),
            right_to_left: rtl,
            font: None,
            font_scale: 1.0,
            strings: pairs
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect(),
        };
        let en = assets.add(strings("en", false, &[("play", "Play"), ("quit", "Quit")]));
        let ar = assets.add(strings("ar", true, &[("play", "العب")]));
        let mut locale = Locale::new([("en".into(), en), ("ar".into(), ar)], "en", "en");
        locale.rebuild(&assets);
        assert_eq!((locale.text("play"), locale.is_rtl()), ("Play", false));
        assert!(locale.set_language("ar"));
        assert!(!locale.set_language("fr"));
        let revision = locale.revision();
        locale.rebuild(&assets);
        assert!(locale.revision() > revision);
        assert_eq!(locale.text("play"), "العب");
        assert_eq!(locale.text("quit"), "Quit");
        assert_eq!(locale.text("nope"), "nope");
        assert!(locale.is_rtl());
        assert_eq!(locale.language_name("ar"), "AR");
    }
}
