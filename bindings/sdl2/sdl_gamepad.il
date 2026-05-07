// SDL2 game-controller (gamepad) bindings: opens the first attached
// controller, exposes axis + button polling. Initialise SDL with
// `InitFlag.gameController` (and ideally `InitFlag.joystick`) before
// instantiating `Gamepad`.

/// `SDL_GameControllerButton` — face / shoulder / d-pad / stick-press
/// buttons that follow the standard layout.
enum Button: i32 {
    a              = 0
    b              = 1
    x              = 2
    y              = 3
    back           = 4
    guide          = 5
    start          = 6
    leftStick      = 7
    rightStick     = 8
    leftShoulder   = 9
    rightShoulder  = 10
    dpadUp         = 11
    dpadDown       = 12
    dpadLeft       = 13
    dpadRight      = 14
}

/// `SDL_GameControllerAxis`. Stick axes range `[-32768, 32767]`;
/// trigger axes range `[0, 32767]`.
enum Axis: i32 {
    leftX        = 0
    leftY        = 1
    rightX       = 2
    rightY       = 3
    triggerLeft  = 4
    triggerRight = 5
}

@extern(C) {
    /// Opaque SDL handle.
    struct SDL_GameController {}

    @lib("SDL2") fn SDL_NumJoysticks(): i32
    @lib("SDL2") fn SDL_IsGameController(joystick_index: i32): i32
    @lib("SDL2") fn SDL_GameControllerOpen(joystick_index: i32): *SDL_GameController
    @lib("SDL2") fn SDL_GameControllerClose(controller: *SDL_GameController)
    @lib("SDL2") fn SDL_GameControllerGetAxis(
        controller: *SDL_GameController, axis: i32
    ): i16
    @lib("SDL2") fn SDL_GameControllerGetButton(
        controller: *SDL_GameController, button: i32
    ): u8

    /// ARC-managed wrapper around the first available controller.
    /// `handle == 0` means no controller was attached at the time of
    /// construction; in that case every poll degrades to returning
    /// `0` / `false`.
    class Gamepad {
        /// Pointer to the underlying `SDL_GameController`, cast to
        /// `i64`. `0` when no device is open.
        handle: i64

        /// Open the first attached controller. Sets `handle` to `0`
        /// if none of the connected joysticks are recognised as game
        /// controllers (e.g. a steering wheel or a HID device with
        /// no SDL mapping).
        init() {
            this.handle = 0
            let n = SDL_NumJoysticks()
            let i: i32 = 0
            while i < n {
                if SDL_IsGameController(i) != 0 {
                    let p = SDL_GameControllerOpen(i)
                    this.handle = p as i64
                    return
                }
                i += 1
            }
        }

        deinit() {
            if this.handle != 0 {
                SDL_GameControllerClose(this.handle as *SDL_GameController)
            }
        }

        /// Read the current value of a stick / trigger axis.
        ///
        /// Args:
        ///   axis — the axis to read.
        ///
        /// Returns:
        ///   stick axes in `[-32768, 32767]`; trigger axes in
        ///   `[0, 32767]`. `0` when no controller is open.
        getAxis(axis: Axis): i16 {
            if this.handle == 0 { return 0 as i16 }
            SDL_GameControllerGetAxis(
                this.handle as *SDL_GameController, axis as i32
            )
        }

        /// Read the held / not-held state of a button.
        ///
        /// Args:
        ///   button — the button to query.
        ///
        /// Returns:
        ///   true when the button is currently held. `false` when
        ///   no controller is open.
        isPressed(button: Button): bool {
            if this.handle == 0 { return false }
            SDL_GameControllerGetButton(
                this.handle as *SDL_GameController, button as i32
            ) != 0 as u8
        }
    }
}
