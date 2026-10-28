//! Mouse and touch as one "pointer" input.

use cg_core::ecs::resource::Resource;
use cg_core::glam::Vec2;
use serde::{Deserialize, Serialize};

/// Identifies a pointer: the mouse, or one finger on a touch screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PointerId {
    /// The mouse. It is pressed while the left button is held.
    Mouse,
    /// A finger, by the platform's touch id. It exists from touch start to one frame after
    /// touch end.
    Touch(u64),
}

/// One pointer's state this frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pointer {
    id: PointerId,
    position: Vec2,
    normalized: Vec2,
    delta: Vec2,
    pressed: bool,
    just_pressed: bool,
    just_released: bool,
    placed: bool,
}

impl Pointer {
    fn new(id: PointerId) -> Self {
        Self {
            id,
            position: Vec2::ZERO,
            normalized: Vec2::ZERO,
            delta: Vec2::ZERO,
            pressed: false,
            just_pressed: false,
            just_released: false,
            placed: false,
        }
    }

    /// Which pointer this is.
    pub fn id(&self) -> PointerId {
        self.id
    }

    /// Position in logical pixels from the window's top-left corner.
    pub fn position(&self) -> Vec2 {
        self.position
    }

    /// Position as a fraction of the window size: `(0, 0)` is top-left, `(1, 1)` bottom-right.
    pub fn normalized(&self) -> Vec2 {
        self.normalized
    }

    /// Movement this frame, in logical pixels.
    pub fn delta(&self) -> Vec2 {
        self.delta
    }

    /// Whether the pointer is down: the left mouse button is held or the finger touches.
    pub fn pressed(&self) -> bool {
        self.pressed
    }

    /// Whether the pointer went down this frame.
    pub fn just_pressed(&self) -> bool {
        self.just_pressed
    }

    /// Whether the pointer went up this frame.
    pub fn just_released(&self) -> bool {
        self.just_released
    }

    /// Whether this is a touch pointer.
    pub fn is_touch(&self) -> bool {
        matches!(self.id, PointerId::Touch(_))
    }
}

/// Logical pixels counted as one line of scrolling. Browsers report about 100 pixels per
/// mouse-wheel notch, so a notch reads as about one line on every platform.
const PIXELS_PER_SCROLL_LINE: f32 = 100.0;

/// Every mouse and touch pointer, updated by the platform runner.
///
/// Game code that only needs "where did the player click or tap" reads
/// [`primary`](Self::primary); multi-touch code iterates [`iter`](Self::iter).
#[derive(Resource, Debug, Clone, Default)]
pub struct Pointers {
    pointers: Vec<Pointer>,
    scroll: Vec2,
}

impl Pointers {
    /// All pointers, the mouse first if it is over the window, then touches in start order.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &Pointer> {
        self.pointers.iter()
    }

    /// The pointer with this id, if it is active.
    pub fn get(&self, id: PointerId) -> Option<&Pointer> {
        self.pointers.iter().find(|p| p.id == id)
    }

    /// The pointer most games care about: the oldest touch if any, otherwise the mouse.
    pub fn primary(&self) -> Option<&Pointer> {
        self.pointers
            .iter()
            .find(|p| p.is_touch())
            .or_else(|| self.get(PointerId::Mouse))
    }

    /// Whether any pointer went down this frame.
    pub fn any_just_pressed(&self) -> bool {
        self.pointers.iter().any(|p| p.just_pressed)
    }

    /// Mouse-wheel scroll this frame, in lines. Positive `y` scrolls up.
    pub fn scroll(&self) -> Vec2 {
        self.scroll
    }

    fn entry(&mut self, id: PointerId) -> &mut Pointer {
        let index = match self.pointers.iter().position(|p| p.id == id) {
            Some(index) => index,
            None => {
                let pointer = Pointer::new(id);
                if id == PointerId::Mouse {
                    self.pointers.insert(0, pointer);
                    0
                } else {
                    self.pointers.push(pointer);
                    self.pointers.len() - 1
                }
            }
        };
        &mut self.pointers[index]
    }

    /// Moves a pointer, adding it if it is new. `position` is in logical pixels and
    /// `window_size` is the window's logical size.
    pub fn move_to(&mut self, id: PointerId, position: Vec2, window_size: Vec2) {
        let pointer = self.entry(id);
        if pointer.placed {
            pointer.delta += position - pointer.position;
        }
        pointer.placed = true;
        pointer.position = position;
        pointer.normalized = if window_size.x > 0.0 && window_size.y > 0.0 {
            position / window_size
        } else {
            Vec2::ZERO
        };
    }

    /// Puts a pointer down, adding it if it is new.
    pub fn press(&mut self, id: PointerId) {
        let pointer = self.entry(id);
        if !pointer.pressed {
            pointer.pressed = true;
            pointer.just_pressed = true;
        }
    }

    /// Lifts a pointer. Touch pointers disappear at the end of the frame.
    pub fn release(&mut self, id: PointerId) {
        if let Some(pointer) = self.pointers.iter_mut().find(|p| p.id == id)
            && pointer.pressed
        {
            pointer.pressed = false;
            pointer.just_released = true;
        }
    }

    /// The mouse left the window. The mouse pointer goes away unless a button is held.
    pub fn mouse_left(&mut self) {
        self.pointers
            .retain(|p| p.id != PointerId::Mouse || p.pressed || p.just_released);
    }

    /// Lifts every pointer, for example when the window loses focus.
    pub fn release_all(&mut self) {
        for pointer in self.pointers.iter_mut().filter(|p| p.pressed) {
            pointer.pressed = false;
            pointer.just_released = true;
        }
    }

    /// Adds mouse-wheel scroll in lines.
    pub fn scroll_lines(&mut self, lines: Vec2) {
        self.scroll += lines;
    }

    /// Adds scroll in logical pixels (touchpads, browsers).
    pub fn scroll_pixels(&mut self, pixels: Vec2) {
        self.scroll += pixels / PIXELS_PER_SCROLL_LINE;
    }

    /// Ends the frame: clears per-frame state and removes lifted touch pointers. The input
    /// plugin calls this at the end of every frame.
    pub fn clear(&mut self) {
        self.pointers.retain(|p| !p.is_touch() || p.pressed);
        for pointer in &mut self.pointers {
            pointer.delta = Vec2::ZERO;
            pointer.just_pressed = false;
            pointer.just_released = false;
        }
        self.scroll = Vec2::ZERO;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: Vec2 = Vec2::new(200.0, 100.0);

    #[test]
    fn touch_lives_until_the_frame_after_release() {
        let mut pointers = Pointers::default();
        let finger = PointerId::Touch(7);
        pointers.move_to(finger, Vec2::new(50.0, 50.0), WINDOW);
        pointers.press(finger);
        let p = *pointers.get(finger).unwrap();
        assert!(p.pressed() && p.just_pressed());
        assert_eq!(p.normalized(), Vec2::new(0.25, 0.5));

        pointers.clear();
        pointers.move_to(finger, Vec2::new(60.0, 50.0), WINDOW);
        assert_eq!(pointers.get(finger).unwrap().delta(), Vec2::new(10.0, 0.0));
        pointers.release(finger);
        assert!(pointers.get(finger).unwrap().just_released());

        pointers.clear();
        assert!(pointers.get(finger).is_none());
    }

    #[test]
    fn primary_prefers_touch_over_mouse() {
        let mut pointers = Pointers::default();
        pointers.move_to(PointerId::Mouse, Vec2::new(1.0, 1.0), WINDOW);
        assert_eq!(pointers.primary().unwrap().id(), PointerId::Mouse);
        pointers.move_to(PointerId::Touch(1), Vec2::new(2.0, 2.0), WINDOW);
        pointers.move_to(PointerId::Touch(2), Vec2::new(3.0, 3.0), WINDOW);
        assert_eq!(pointers.primary().unwrap().id(), PointerId::Touch(1));
        assert_eq!(pointers.iter().next().unwrap().id(), PointerId::Mouse);
    }

    #[test]
    fn mouse_stays_while_dragging_outside() {
        let mut pointers = Pointers::default();
        pointers.move_to(PointerId::Mouse, Vec2::new(1.0, 1.0), WINDOW);
        pointers.press(PointerId::Mouse);
        pointers.mouse_left();
        assert!(pointers.get(PointerId::Mouse).is_some());
        pointers.release(PointerId::Mouse);
        pointers.clear();
        pointers.mouse_left();
        assert!(pointers.get(PointerId::Mouse).is_none());
    }

    #[test]
    fn scroll_resets_each_frame() {
        let mut pointers = Pointers::default();
        pointers.scroll_lines(Vec2::new(0.0, 1.0));
        pointers.scroll_pixels(Vec2::new(0.0, 200.0));
        assert_eq!(pointers.scroll(), Vec2::new(0.0, 3.0));
        pointers.clear();
        assert_eq!(pointers.scroll(), Vec2::ZERO);
    }
}
