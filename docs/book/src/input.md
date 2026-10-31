# Input

Games read **actions**, named by the game, rather than keys. An `ActionMap` binds each action to keys, mouse buttons, gamepad buttons and stick directions, and screen regions; on-screen touch controls press actions too. One map serves keyboard, gamepad and touch players alike.

```rust,ignore
use AxisDirection::{Negative, Positive};

app.insert_resource(
    ActionMap::new()
        .bind("left", KeyCode::ArrowLeft)
        .bind("left", KeyCode::KeyA)
        .bind("left", Binding::Axis(GamepadAxis::LeftStickX, Negative))
        .bind("jump", KeyCode::Space)
        .bind("jump", GamepadButton::South),
);

fn read(actions: Res<ActionState>) {
    if actions.just_pressed("jump") { /* ... */ }
    let run = actions.axis("left", "right"); // -1 to 1
}
```

`ActionState` has `pressed`, `just_pressed`, `just_released`, analogue `value`s and `axis` for a pair of opposite actions. Inside fixed steps, `just_pressed` means "since the previous fixed step", so a press is seen exactly once by gameplay whatever the frame rate; elsewhere it means "this frame".

## Pointers and touch

The mouse and every finger are `Pointer`s in the `Pointers` resource, with positions in logical pixels. `MainCamera::screen_to_world` turns them into world positions. The UI takes pointers first; `UiPointer::over_ui` tells a game whether a click landed on a button.

On-screen controls are UI nodes: a `TouchButton::new("jump")` presses its action while a finger is on it, and a `TouchDpad` presses up, down, left and right by the finger's direction. They show on touch screens (`Window::touch_screen`: phones, tablets, mobile browsers) after the first touch, and hide when a key or gamepad is used; `--touch` shows them anywhere for testing.

## Gamepads

Gamepads work on desktop and in browsers (the Gamepad API), through `gilrs`. Buttons use positions (`South`, `East`...), so `South` is A on an Xbox pad and cross on a PlayStation one. Android has no gamepad support yet.

## Rebinding

Bindings are plain data (`Binding` serializes with serde), so a settings screen can change and save them. `Binding::just_pressed` captures the next key or button for a "press a key" prompt, and `Display` names bindings for menus. The sample platformer's Controls screen does all of this.

## The menus' keys

The UI binds its own actions (`ui_up`, `ui_down`, `ui_left`, `ui_right`, `ui_accept`, `ui_back`) to arrows, WASD, Enter, Space, Escape and the gamepad, for actions the game's map lacks. Android's back button and gesture press `ui_back`.
