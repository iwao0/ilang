// SDL2 keyboard polling. `Keyboard.isPressed(scancode)` reads SDL's
// internal keyboard state — call `sdl.pumpEvents()` once per frame
// before querying so the state reflects the latest events.

@extern(C) {
    @lib("SDL2") fn SDL_GetKeyboardState(numkeys: *i32): *const u8

    // No instance state — `Keyboard` exists just to namespace the
    // polling helper. Use the static method directly:
    //     if Keyboard.isPressed(scancode) { ... }
    class Keyboard {
        static isPressed(scancode: i32): bool {
            // numkeys = NULL: we already know the array is at least
            // SDL_NUM_SCANCODES = 512 entries, no need for the count.
            let state = SDL_GetKeyboardState(0 as *i32)
            // Snapshot 512 bytes (covers every defined SDL_Scancode).
            let snapshot = bytesFromBuffer(state as *const void, 512 as size_t)
            if scancode < 0 {
                false
            } else {
                snapshot[scancode] != 0
            }
        }
    }
}

// SDL_Scancode constants. Names match the C SDL_SCANCODE_* values.
const SCANCODE_A: i32 = 4
const SCANCODE_D: i32 = 7
const SCANCODE_S: i32 = 22
const SCANCODE_W: i32 = 26
const SCANCODE_RETURN: i32 = 40
const SCANCODE_ESCAPE: i32 = 41
const SCANCODE_SPACE: i32 = 44
const SCANCODE_RIGHT: i32 = 79
const SCANCODE_LEFT: i32 = 80
const SCANCODE_DOWN: i32 = 81
const SCANCODE_UP: i32 = 82
