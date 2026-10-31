//! Showing and hiding entities: [`Visibility`] and the computed [`InheritedVisibility`].

use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};

/// Whether an entity (and, through inheritance, its children) is drawn.
///
/// Hiding a parent hides its whole subtree unless a descendant says
/// [`Visible`](Self::Visible). Adding a [`Sprite`](crate::Sprite) adds a `Visibility`.
///
/// In prefabs: `"Visibility": Hidden`.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[require(InheritedVisibility)]
pub enum Visibility {
    /// Visible if the parent is, or if there is no parent. The default.
    #[default]
    Inherited,
    /// Visible whatever the parent says.
    Visible,
    /// Not drawn, and children that inherit are not drawn either.
    Hidden,
}

impl Visibility {
    /// Flips between [`Hidden`](Self::Hidden) and [`Inherited`](Self::Inherited).
    pub fn toggle(&mut self) {
        *self = match self {
            Self::Hidden => Self::Inherited,
            Self::Inherited | Self::Visible => Self::Hidden,
        };
    }
}

/// Whether an entity is drawn, after inheritance: computed from [`Visibility`] down the
/// hierarchy in [`PostUpdate`](cg_core::PostUpdate). Game code reads it; the renderer
/// skips entities where it is `false`.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct InheritedVisibility(bool);

impl Default for InheritedVisibility {
    fn default() -> Self {
        Self::VISIBLE
    }
}

impl InheritedVisibility {
    /// Drawn.
    pub const VISIBLE: Self = Self(true);
    /// Not drawn.
    pub const HIDDEN: Self = Self(false);

    /// Whether the entity is drawn.
    pub fn get(self) -> bool {
        self.0
    }
}

/// Hierarchy roots that visibility starts from.
/// The system set in [`PostUpdate`](cg_core::PostUpdate) that computes
/// [`InheritedVisibility`]. Systems that read it there go after it.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VisibilitySystems {
    /// Propagates visibility down the hierarchy.
    Propagate,
}

type RootFilter = (Without<ChildOf>, Or<(With<Visibility>, With<Children>)>);

/// An entity's visibility inputs and output, and its children.
type VisibilityNode = (
    Option<&'static Visibility>,
    Option<&'static mut InheritedVisibility>,
    Option<&'static Children>,
);

/// Computes [`InheritedVisibility`] from [`Visibility`] down the hierarchy. Entities with
/// no `Visibility` pass their parent's on unchanged. Roots are visited in storage order;
/// only children are looked up one by one.
#[profiling::function]
pub(crate) fn propagate_visibility(
    mut roots: Query<VisibilityNode, RootFilter>,
    mut nodes: Query<VisibilityNode, With<ChildOf>>,
    mut stack: Local<Vec<(Entity, bool)>>,
) {
    for (visibility, inherited, children) in &mut roots {
        let visible = resolve(visibility, true, inherited);
        if let Some(children) = children {
            stack.extend(children.iter().map(|child| (child, visible)));
        }
    }
    while let Some((entity, parent_visible)) = stack.pop() {
        let Ok((visibility, inherited, children)) = nodes.get_mut(entity) else {
            continue;
        };
        let visible = resolve(visibility, parent_visible, inherited);
        if let Some(children) = children {
            stack.extend(children.iter().map(|child| (child, visible)));
        }
    }
}

/// Whether an entity is visible given its parent, written to its [`InheritedVisibility`]
/// only when that changes.
fn resolve(
    visibility: Option<&Visibility>,
    parent_visible: bool,
    inherited: Option<Mut<InheritedVisibility>>,
) -> bool {
    let visible = match visibility {
        Some(Visibility::Hidden) => false,
        Some(Visibility::Visible) => true,
        Some(Visibility::Inherited) | None => parent_visible,
    };
    if let Some(mut inherited) = inherited
        && inherited.0 != visible
    {
        inherited.0 = visible;
    }
    visible
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_ecs::schedule::Schedule;

    #[test]
    fn hidden_parents_hide_inheriting_children() {
        let mut world = World::new();
        let mut schedule = Schedule::default();
        schedule.add_systems(propagate_visibility);

        let root = world.spawn(Visibility::Hidden).id();
        let plain = world.spawn(ChildOf(root)).id(); // no Visibility of its own
        let inherits = world.spawn((Visibility::Inherited, ChildOf(plain))).id();
        let forced = world.spawn((Visibility::Visible, ChildOf(root))).id();
        let other = world.spawn(Visibility::default()).id();
        schedule.run(&mut world);

        let shown = |world: &World, e| world.get::<InheritedVisibility>(e).unwrap().get();
        assert!(!shown(&world, root));
        assert!(!shown(&world, inherits));
        assert!(shown(&world, forced));
        assert!(shown(&world, other));

        *world.get_mut::<Visibility>(root).unwrap() = Visibility::Inherited;
        schedule.run(&mut world);
        assert!(shown(&world, inherits));
    }

    #[test]
    fn toggle_hides_and_shows() {
        let mut visibility = Visibility::Inherited;
        visibility.toggle();
        assert_eq!(visibility, Visibility::Hidden);
        visibility.toggle();
        assert_eq!(visibility, Visibility::Inherited);
    }
}
