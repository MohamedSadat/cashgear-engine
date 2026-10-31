//! {{title}} on desktop and the web; the game itself is the `{{crate_name}}` library.

// No console window behind the game in Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> cashgear_engine::core::AppExit {
    {{crate_name}}::run()
}
