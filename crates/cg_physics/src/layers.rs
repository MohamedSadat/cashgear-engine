//! [`CollisionLayers`]: which colliders interact.

use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};

/// Which collision layers an entity's colliders are on, and which layers they interact
/// with. Up to 32 layers, one bit each.
///
/// Two colliders interact when each one's `filters` include a layer of the other:
/// `a.memberships & b.filters != 0 && b.memberships & a.filters != 0`. Only interacting
/// colliders block kinematic bodies, report overlaps and are found by queries filtered
/// by layer. The same rule covers tile colliders: put a `CollisionLayers` on the map
/// entity.
///
/// Without the component an entity is on [`DEFAULT`](Self::DEFAULT): layer 0, interacting
/// with everything.
///
/// ```
/// use cg_physics::CollisionLayers;
///
/// const WORLD: u32 = 1 << 0;
/// const PLAYER: u32 = 1 << 1;
/// const ENEMY: u32 = 1 << 2;
///
/// // Enemies stand on the world and touch the player, but walk through each other.
/// let enemy = CollisionLayers::new(ENEMY, WORLD | PLAYER);
/// let player = CollisionLayers::new(PLAYER, WORLD | ENEMY);
/// assert!(enemy.interacts(player));
/// assert!(!enemy.interacts(enemy));
/// ```
///
/// In prefabs: `"CollisionLayers": (memberships: 2, filters: 5)`.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub struct CollisionLayers {
    /// The layers this entity is on.
    pub memberships: u32,
    /// The layers it interacts with.
    pub filters: u32,
}

impl CollisionLayers {
    /// Every layer.
    pub const ALL_BITS: u32 = u32::MAX;
    /// Layer 0, interacting with every layer.
    pub const DEFAULT: Self = Self::new(1, Self::ALL_BITS);
    /// On no layer: interacts with nothing.
    pub const NONE: Self = Self::new(0, 0);

    /// On `memberships`, interacting with `filters`.
    pub const fn new(memberships: u32, filters: u32) -> Self {
        Self {
            memberships,
            filters,
        }
    }

    /// Whether colliders on `self` and `other` interact: each one's filters include a
    /// layer of the other.
    pub const fn interacts(self, other: Self) -> bool {
        self.memberships & other.filters != 0 && other.memberships & self.filters != 0
    }
}

impl Default for CollisionLayers {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interaction_needs_both_sides() {
        let a = CollisionLayers::new(0b01, 0b10);
        let b = CollisionLayers::new(0b10, 0b01);
        assert!(a.interacts(b) && b.interacts(a));
        // `c` wants to hit `a`, but `a` does not filter for `c`'s layer.
        let c = CollisionLayers::new(0b100, 0b01);
        assert!(!a.interacts(c) && !c.interacts(a));
        assert!(CollisionLayers::DEFAULT.interacts(CollisionLayers::DEFAULT));
        assert!(!CollisionLayers::NONE.interacts(CollisionLayers::DEFAULT));
    }
}
