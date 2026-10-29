//! Pointer input for the UI: hovering, pressing and clicking widgets with the mouse or a
//! finger, dragging sliders, and [`UiPointer`] for games that need to know whether the
//! pointer is over the UI.

use crate::focus::UiFocus;
use crate::layout::UiStack;
use crate::node::ComputedNode;
use crate::widgets::{
    BackgroundColor, Button, Clicked, Focusable, Interaction, Slider, SliderChanged, SliderThumb,
    UiImage,
};
use bevy_ecs::prelude::*;
use cg_core::glam::Vec2;
use cg_platform::Window;
use cg_platform::input::{PointerId, Pointers};
use std::collections::HashMap;

/// Where the pointers are relative to the UI.
///
/// Check `over_ui` before reacting to clicks in the game world, so a click on a button
/// does not also fire a shot.
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct UiPointer {
    pub(crate) over_ui: bool,
    pub(crate) position: Option<Vec2>,
    pub(crate) hovered: Option<Entity>,
    /// Widgets held down by a pointer this frame.
    pub(crate) pressed: Vec<Entity>,
    /// Set while something else (the debug overlay) is using the pointer.
    pub(crate) blocked: bool,
}

impl UiPointer {
    /// Whether any pointer is over a visible node that draws something or reacts to it.
    pub fn over_ui(&self) -> bool {
        self.over_ui
    }

    /// The primary pointer's position in UI pixels, if there is one.
    pub fn position(&self) -> Option<Vec2> {
        self.position
    }

    /// The interactive node under the mouse, if any.
    pub fn hovered(&self) -> Option<Entity> {
        self.hovered
    }
}

/// Which node each pointer pressed, while it stays down.
#[derive(Default)]
pub(crate) struct Captures(HashMap<PointerId, Entity>);

type Hit = (
    &'static ComputedNode,
    Has<Interaction>,
    Option<&'static Button>,
    Has<BackgroundColor>,
    Has<UiImage>,
);

/// Hit tests the pointers against the UI and updates [`Interaction`]s, sliders and focus.
/// Runs in `PreUpdate` after the actions.
#[allow(clippy::too_many_arguments)]
pub(crate) fn interact(
    pointers: Res<Pointers>,
    window: Res<Window>,
    stack: Res<UiStack>,
    hits: Query<Hit>,
    mut sliders: Query<(&ComputedNode, &mut Slider, Option<&Children>)>,
    thumbs: Query<&ComputedNode, With<SliderThumb>>,
    focusable: Query<(), With<Focusable>>,
    mut focus: ResMut<UiFocus>,
    mut ui_pointer: ResMut<UiPointer>,
    mut captures: Local<Captures>,
    mut clicks: MessageWriter<Clicked>,
    mut changes: MessageWriter<SliderChanged>,
) {
    let captures = &mut captures.0;
    let space = stack.space;
    ui_pointer.over_ui = false;
    ui_pointer.hovered = None;
    ui_pointer.position = pointers
        .primary()
        .map(|p| space.from_screen(p.position(), &window));
    if ui_pointer.blocked {
        captures.clear();
    }

    // The topmost node under a point: interactive (and enabled) ones for clicks, any that
    // draws for `over_ui`.
    let hit_test = |point: Vec2| {
        let mut solid = false;
        for &entity in stack.entities.iter().rev() {
            let Ok((computed, interactive, button, background, image)) = hits.get(entity) else {
                continue;
            };
            if !computed.rect.contains(point) {
                continue;
            }
            solid |= interactive || background || image;
            if interactive && !button.is_some_and(|b| b.disabled) {
                return (Some(entity), true);
            }
        }
        (None, solid)
    };

    let mut pressed_now = core::mem::take(&mut ui_pointer.pressed);
    pressed_now.clear();
    if !ui_pointer.blocked {
        for pointer in pointers.iter() {
            let point = space.from_screen(pointer.position(), &window);
            let (hit, solid) = hit_test(point);
            ui_pointer.over_ui |= solid;
            if pointer.id() == PointerId::Mouse && !pointer.pressed() {
                ui_pointer.hovered = hit;
                if let Some(hit) = hit
                    && pointer.delta() != Vec2::ZERO
                    && focusable.contains(hit)
                {
                    focus.set_from_pointer(hit);
                }
            }
            if pointer.just_pressed()
                && let Some(hit) = hit
            {
                captures.insert(pointer.id(), hit);
                if focusable.contains(hit) {
                    focus.set_from_pointer(hit);
                }
            }
            let Some(&captured) = captures.get(&pointer.id()) else {
                continue;
            };
            // Drag sliders while held.
            if let Ok((computed, mut slider, children)) = sliders.get_mut(captured) {
                let thumb = children
                    .into_iter()
                    .flat_map(|c| c.iter())
                    .find_map(|c| thumbs.get(c).ok())
                    .map_or(0.0, |t| t.rect.width());
                let rect = computed.content;
                let track = (rect.width() - thumb).max(1.0);
                let mut fraction = (point.x - rect.min.x - thumb * 0.5) / track;
                if computed.rtl {
                    fraction = 1.0 - fraction;
                }
                let before = slider.value;
                let mut changed = *slider;
                changed.set_fraction(fraction);
                if changed.value != before {
                    slider.value = changed.value;
                    changes.write(SliderChanged {
                        entity: captured,
                        value: changed.value,
                    });
                }
            }
            if pointer.pressed() && (hit == Some(captured) || sliders.contains(captured)) {
                pressed_now.push(captured);
            }
            if pointer.just_released() {
                captures.remove(&pointer.id());
                if hit == Some(captured) && hits.get(captured).is_ok_and(|h| h.2.is_some()) {
                    clicks.write(Clicked { entity: captured });
                }
            }
        }
    }
    // Pointers that are gone release what they held.
    captures.retain(|id, _| pointers.get(*id).is_some_and(|p| p.pressed()));
    ui_pointer.pressed = pressed_now;
}

/// Sets each widget's [`Interaction`] from the pointers and the focus. Runs after both.
pub(crate) fn update_interactions(
    ui_pointer: Res<UiPointer>,
    focus: Res<UiFocus>,
    mut interactions: Query<(Entity, &mut Interaction)>,
) {
    for (entity, mut interaction) in &mut interactions {
        let state = if ui_pointer.pressed.contains(&entity) {
            Interaction::Pressed
        } else if ui_pointer.hovered == Some(entity) || focus.focused == Some(entity) {
            Interaction::Hovered
        } else {
            Interaction::None
        };
        if *interaction != state {
            *interaction = state;
        }
    }
}
