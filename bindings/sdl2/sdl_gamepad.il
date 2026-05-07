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
    ///
    /// Edge detection (`isPressed` / `isReleased`) requires
    /// `pump()` to be called once per frame before the polling
    /// methods, so the previous-frame snapshot is up to date.
    class Gamepad {
        /// Pointer to the underlying `SDL_GameController`, cast to
        /// `i64`. `0` when no device is open.
        handle: i64
        /// Snapshot of every button's held / not-held state at the
        /// previous `pump()`. 15 slots — one per `Button` variant.
        prev: u8[]
        /// Snapshot at the most recent `pump()`. `isHeld` /
        /// `isPressed` / `isReleased` read this — never the live
        /// SDL state — so unrelated SDL calls between frames can't
        /// race with edge detection.
        curr: u8[]

        /// Open the first attached controller. Sets `handle` to `0`
        /// if none of the connected joysticks are recognised as game
        /// controllers (e.g. a steering wheel or a HID device with
        /// no SDL mapping).
        init() {
            this.handle = 0
            // SDL_CONTROLLER_BUTTON_MAX = 15 (matches the `Button`
            // enum, `a` … `dpadRight`).
            let p: u8[] = []
            let c: u8[] = []
            let i: i32 = 0
            while i < 15 {
                p.push(0)
                c.push(0)
                i += 1
            }
            this.prev = p
            this.curr = c
            let n = SDL_NumJoysticks()
            let j: i32 = 0
            while j < n {
                if SDL_IsGameController(j) != 0 {
                    let h = SDL_GameControllerOpen(j)
                    this.handle = h as i64
                    return
                }
                j += 1
            }
        }

        deinit() {
            if this.handle != 0 {
                SDL_GameControllerClose(this.handle as *SDL_GameController)
            }
        }

        /// Roll `curr → prev` and snapshot the latest button state
        /// into `curr`. Call once per frame after `sdl.pumpEvents()`
        /// (which keeps SDL's joystick state fresh); subsequent
        /// `isHeld` / `isPressed` / `isReleased` queries this
        /// snapshot, not SDL's live state.
        pump() {
            let i: i32 = 0
            while i < 15 {
                this.prev[i as i64] = this.curr[i as i64]
                i += 1
            }
            if this.handle == 0 {
                let z: i32 = 0
                while z < 15 {
                    this.curr[z as i64] = 0 as u8
                    z += 1
                }
                return
            }
            let j: i32 = 0
            while j < 15 {
                this.curr[j as i64] = SDL_GameControllerGetButton(
                    this.handle as *SDL_GameController, j
                )
                j += 1
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

        /// Returns:
        ///   true while the button was held at the most recent
        ///   `pump()`. `false` before the first `pump()` or when
        ///   `button` is out of range.
        isHeld(button: Button): bool {
            let b: i32 = button as i32
            if b < 0 || b >= 15 {
                return false
            }
            this.curr[b as i64] != 0 as u8
        }

        /// Returns:
        ///   true on the single frame the button transitions from
        ///   "up" to "down" — rising edge.
        isPressed(button: Button): bool {
            let b: i32 = button as i32
            if b < 0 || b >= 15 {
                return false
            }
            this.curr[b as i64] != 0 as u8 && this.prev[b as i64] == 0 as u8
        }

        /// Returns:
        ///   true on the single frame the button transitions from
        ///   "down" to "up" — falling edge.
        isReleased(button: Button): bool {
            let b: i32 = button as i32
            if b < 0 || b >= 15 {
                return false
            }
            this.prev[b as i64] != 0 as u8 && this.curr[b as i64] == 0 as u8
        }
    }
}
