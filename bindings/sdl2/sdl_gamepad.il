// SDL2 game-controller (gamepad) bindings: opens the first attached
// controller, exposes axis + button polling. Initialise SDL with
// `INIT_GAMECONTROLLER` (and ideally `INIT_JOYSTICK`) before
// instantiating `Gamepad`.

@extern(C) {
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

    // ARC-managed wrapper around the first available controller.
    // `handle` is 0 if nothing was attached at construction time.
    class Gamepad {
        handle: i64

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

        // Axis value in [-32768, 32767]. 0 when no controller open.
        getAxis(axis: i32): i16 {
            if this.handle == 0 { return 0 as i16 }
            SDL_GameControllerGetAxis(this.handle as *SDL_GameController, axis)
        }

        // True when the button is currently held. False when no
        // controller is open.
        isPressed(button: i32): bool {
            if this.handle == 0 { return false }
            SDL_GameControllerGetButton(this.handle as *SDL_GameController, button) != 0 as u8
        }
    }
}

// SDL_GameControllerButton enum values.
const BUTTON_A: i32 = 0
const BUTTON_B: i32 = 1
const BUTTON_X: i32 = 2
const BUTTON_Y: i32 = 3
const BUTTON_BACK: i32 = 4
const BUTTON_GUIDE: i32 = 5
const BUTTON_START: i32 = 6
const BUTTON_LEFTSTICK: i32 = 7
const BUTTON_RIGHTSTICK: i32 = 8
const BUTTON_LEFTSHOULDER: i32 = 9
const BUTTON_RIGHTSHOULDER: i32 = 10
const BUTTON_DPAD_UP: i32 = 11
const BUTTON_DPAD_DOWN: i32 = 12
const BUTTON_DPAD_LEFT: i32 = 13
const BUTTON_DPAD_RIGHT: i32 = 14

// SDL_GameControllerAxis enum values. Stick axes range
// [-32768, 32767]; trigger axes range [0, 32767].
const AXIS_LEFTX: i32 = 0
const AXIS_LEFTY: i32 = 1
const AXIS_RIGHTX: i32 = 2
const AXIS_RIGHTY: i32 = 3
const AXIS_TRIGGERLEFT: i32 = 4
const AXIS_TRIGGERRIGHT: i32 = 5
