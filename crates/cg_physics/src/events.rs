//! Overlap reporting: the [`CollisionStarted`] and [`CollisionEnded`] messages, and the
//! [`Overlaps`] resource that lists what overlaps now.

use crate::world::Pair;
use bevy_ecs::prelude::*;
use std::collections::BTreeSet;

/// Two colliders started overlapping in the latest physics step. The lower entity comes
/// first.
///
/// Overlaps are reported for every pair of interacting colliders (see
/// [`CollisionLayers`](crate::CollisionLayers)) that share area, except pairs of two
/// static colliders: a body touching a coin sensor, two bodies running into each other,
/// a sensor area reaching a crate. A body resting against a wall or floor does not
/// overlap it; that contact is in its [`KinematicContacts`](crate::KinematicContacts).
///
/// Overlaps are checked once per physics step, after the bodies move. A body that moved
/// is tested against sensors and other bodies by the box covering its whole move, so a
/// fast body still touches the coins it passes (that overlap then ends in the next step).
///
/// ```
/// use cg_core::prelude::*;
/// use cg_physics::CollisionStarted;
///
/// #[derive(Component)]
/// struct Player;
/// #[derive(Component)]
/// struct Coin;
///
/// fn collect(
///     mut commands: Commands,
///     mut started: MessageReader<CollisionStarted>,
///     player: Single<Entity, With<Player>>,
///     coins: Query<(), With<Coin>>,
/// ) {
///     for collision in started.read() {
///         if let Some(other) = collision.other(*player)
///             && coins.contains(other)
///         {
///             commands.entity(other).despawn();
///         }
///     }
/// }
/// ```
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CollisionStarted(pub Entity, pub Entity);

/// Two colliders stopped overlapping in the latest physics step, because they moved
/// apart, one lost its collider or was despawned. The lower entity comes first.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CollisionEnded(pub Entity, pub Entity);

macro_rules! pair_methods {
    ($type:ty) => {
        impl $type {
            /// The other entity of the pair, if `entity` is one of the two.
            pub fn other(&self, entity: Entity) -> Option<Entity> {
                if self.0 == entity {
                    Some(self.1)
                } else if self.1 == entity {
                    Some(self.0)
                } else {
                    None
                }
            }

            /// Whether `entity` is one of the two.
            pub fn involves(&self, entity: Entity) -> bool {
                self.0 == entity || self.1 == entity
            }
        }
    };
}

pair_methods!(CollisionStarted);
pair_methods!(CollisionEnded);

/// The pairs of colliders that overlap as of the latest physics step: the same pairs
/// [`CollisionStarted`] and [`CollisionEnded`] report the changes of.
///
/// ```
/// use cg_core::prelude::*;
/// use cg_physics::Overlaps;
///
/// #[derive(Component)]
/// struct Ladder;
///
/// fn on_ladder(player: Entity, overlaps: &Overlaps, ladders: &Query<(), With<Ladder>>) -> bool {
///     overlaps.with(player).any(|other| ladders.contains(other))
/// }
/// ```
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct Overlaps {
    pairs: BTreeSet<Pair>,
}

impl Overlaps {
    /// Whether `a` and `b` overlap.
    pub fn contains(&self, a: Entity, b: Entity) -> bool {
        self.pairs.contains(&Pair::new(a, b))
    }

    /// Every entity overlapping `entity`.
    pub fn with(&self, entity: Entity) -> impl Iterator<Item = Entity> + '_ {
        self.pairs.iter().filter_map(move |&Pair(a, b)| {
            if a == entity {
                Some(b)
            } else if b == entity {
                Some(a)
            } else {
                None
            }
        })
    }

    /// Every overlapping pair, the lower entity first, ordered by entity index.
    pub fn iter(&self) -> impl Iterator<Item = (Entity, Entity)> + '_ {
        self.pairs.iter().map(|pair| (pair.0, pair.1))
    }

    /// The number of overlapping pairs.
    pub fn len(&self) -> usize {
        self.pairs.len()
    }

    /// Whether nothing overlaps.
    pub fn is_empty(&self) -> bool {
        self.pairs.is_empty()
    }

    /// Replaces the pairs, returning the ones that started and the ones that ended.
    pub(crate) fn replace(&mut self, pairs: BTreeSet<Pair>) -> (Vec<Pair>, Vec<Pair>) {
        let started = pairs.difference(&self.pairs).copied().collect();
        let ended = self.pairs.difference(&pairs).copied().collect();
        self.pairs = pairs;
        (started, ended)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replacing_reports_the_difference() {
        let e = |i| Entity::from_raw_u32(i).unwrap();
        let mut overlaps = Overlaps::default();
        let pair = |a, b| Pair::new(e(a), e(b));
        let (started, ended) = overlaps.replace([pair(2, 1), pair(1, 3)].into());
        assert_eq!((started.len(), ended.len()), (2, 0));
        let (started, ended) = overlaps.replace([pair(1, 3), pair(3, 2)].into());
        assert_eq!(started, [Pair(e(2), e(3))]);
        assert_eq!(ended, [Pair(e(1), e(2))]);
        assert!(overlaps.contains(e(3), e(1)));
        assert_eq!(overlaps.with(e(3)).collect::<Vec<_>>(), [e(1), e(2)]);
        let message = CollisionStarted(e(1), e(3));
        assert_eq!(message.other(e(3)), Some(e(1)));
        assert_eq!(message.other(e(2)), None);
        assert!(message.involves(e(1)) && !message.involves(e(2)));
    }
}
