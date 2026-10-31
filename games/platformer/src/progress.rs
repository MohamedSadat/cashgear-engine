//! The run through a level ([`Session`]: hearts, coins, time, the checkpoint) and what is
//! saved between runs ([`Progress`]: unlocked levels and the best result in each).

use cashgear_engine::prelude::*;
use serde::{Deserialize, Serialize};

/// Hearts at the start of a level. A hit costs one; a fall costs one and goes back to the
/// checkpoint; with none left the level is over.
pub(crate) const MAX_HEARTS: u32 = 3;
/// The save file's key in `Storage`.
const SAVE_KEY: &str = "progress.ron";

pub(crate) fn plugin(app: &mut App) {
    app.init_resource::<Session>()
        .add_systems(Startup, load_progress)
        .add_systems(FixedUpdate, tick_clock.in_set(crate::GameSystems::Interact));
}

/// The best result in one level.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub(crate) struct Best {
    /// The most coins taken in one run.
    pub coins: u32,
    /// The coins in the level, when that run was made.
    pub total: u32,
    /// The fastest finish, in seconds.
    pub time: f32,
}

/// What is saved: how far the player got and their best results.
#[derive(Resource, Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Progress {
    /// Levels that can be played: the first `unlocked + 1`.
    pub unlocked: usize,
    /// The best result per level, by index.
    pub best: Vec<Option<Best>>,
}

impl Progress {
    /// Whether level `index` (0-based) can be played.
    pub fn is_unlocked(&self, index: usize) -> bool {
        index <= self.unlocked
    }

    /// The best result in level `index`, if it was ever finished.
    pub fn best(&self, index: usize) -> Option<Best> {
        self.best.get(index).copied().flatten()
    }

    /// Records a finished level: unlocks the next and keeps the best coins and time
    /// (separately). Returns whether either was beaten.
    pub fn finish(&mut self, index: usize, coins: u32, total: u32, time: f32) -> bool {
        self.unlocked = self.unlocked.max(index + 1);
        if self.best.len() <= index {
            self.best.resize(index + 1, None);
        }
        let slot = &mut self.best[index];
        match slot {
            None => {
                *slot = Some(Best { coins, total, time });
                true
            }
            Some(best) => {
                let better = coins > best.coins || time < best.time;
                best.coins = best.coins.max(coins);
                best.total = total;
                best.time = best.time.min(time);
                better
            }
        }
    }

    /// Writes the progress to storage.
    pub fn save(&self, storage: &Storage) {
        let text = ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::new().new_line("\n"));
        match text.map(|text| storage.write(SAVE_KEY, &text)) {
            Ok(Ok(())) => {}
            Ok(Err(err)) => warn!("could not save the progress: {err}"),
            Err(err) => warn!("could not write the progress: {err}"),
        }
    }
}

fn load_progress(mut commands: Commands, storage: Res<Storage>) {
    let progress = match storage.read(SAVE_KEY) {
        Ok(Some(text)) => ron::from_str::<Progress>(&text).unwrap_or_else(|err| {
            warn!("ignoring the saved progress: {err}");
            Progress::default()
        }),
        Ok(None) => Progress::default(),
        Err(err) => {
            warn!("cannot read the saved progress: {err}");
            Progress::default()
        }
    };
    commands.insert_resource(progress);
}

/// The level being played and how it is going.
#[derive(Resource, Debug, Clone, PartialEq)]
pub(crate) struct Session {
    /// The level, 0-based.
    pub level: usize,
    pub hearts: u32,
    pub coins: u32,
    /// The coins in the level.
    pub coins_total: u32,
    /// Seconds of play in this level (paused time does not count).
    pub time: f32,
    /// Fixed steps since the level started; moving platforms follow it.
    pub steps: u64,
    /// Where the player starts, and where the latest checkpoint is.
    pub start: Vec2,
    pub checkpoint: Option<Vec2>,
    /// Hearts lost (hits and falls) in this level.
    pub hits: u32,
    pub falls: u32,
    pub stomps: u32,
    /// Set when the exit is reached, with the time.
    pub finished: Option<f32>,
    /// Whether this finish beat the saved best.
    pub new_best: bool,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            level: 0,
            hearts: MAX_HEARTS,
            coins: 0,
            coins_total: 0,
            time: 0.0,
            steps: 0,
            start: Vec2::ZERO,
            checkpoint: None,
            hits: 0,
            falls: 0,
            stomps: 0,
            finished: None,
            new_best: false,
        }
    }
}

impl Session {
    /// A fresh run of `level`.
    pub fn new(level: usize) -> Self {
        Self { level, ..default() }
    }

    /// Where the player comes back after a fall.
    pub fn respawn_point(&self) -> Vec2 {
        self.checkpoint.unwrap_or(self.start)
    }
}

/// Counts play time and steps until the exit is reached.
fn tick_clock(time: Res<Time>, mut session: ResMut<Session>) {
    session.steps += 1;
    if session.finished.is_none() {
        session.time += time.fixed_delta_secs();
    }
}

/// `mm:ss.t` for the HUD and the menus.
pub(crate) fn format_time(seconds: f32) -> String {
    let mut text = String::new();
    write_time(&mut text, seconds);
    text
}

/// [`format_time`] into a string that keeps its memory (the HUD's, every frame).
pub(crate) fn write_time(out: &mut String, seconds: f32) {
    use core::fmt::Write;
    let tenths = (seconds.max(0.0) * 10.0) as u32;
    out.clear();
    let _ = write!(
        out,
        "{:02}:{:02}.{}",
        tenths / 600,
        tenths / 10 % 60,
        tenths % 10
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finishing_unlocks_and_keeps_the_best() {
        let mut progress = Progress::default();
        assert!(progress.is_unlocked(0) && !progress.is_unlocked(1));
        assert!(progress.finish(0, 10, 40, 90.0));
        assert!(progress.is_unlocked(1) && !progress.is_unlocked(2));
        // Fewer coins but faster: the time improves, the coins stay.
        assert!(progress.finish(0, 8, 40, 80.0));
        assert_eq!(
            progress.best(0),
            Some(Best {
                coins: 10,
                total: 40,
                time: 80.0
            })
        );
        assert!(!progress.finish(0, 5, 40, 85.0));
        // Replaying an earlier level does not lock later ones.
        progress.finish(2, 1, 50, 120.0);
        progress.finish(0, 1, 40, 99.0);
        assert_eq!(progress.unlocked, 3);
        assert_eq!(progress.best(1), None);
    }

    #[test]
    fn progress_round_trips_through_storage() {
        let storage = Storage::memory();
        let mut progress = Progress::default();
        progress.finish(1, 7, 30, 61.5);
        progress.save(&storage);
        let text = storage.read(SAVE_KEY).unwrap().unwrap();
        assert_eq!(ron::from_str::<Progress>(&text).unwrap(), progress);
        assert_eq!(format_time(61.55), "01:01.5");
    }
}
