// SDL2 binding minimal demo: initialize the video subsystem, open a
// 640×480 window, sleep 2 seconds, then tear everything down.
//
// Run (JIT only — interpreter has no dlsym path):
//   ilang run --jit examples/sdl_window.il
//
// Requires SDL2 installed where dlopen("libSDL2") can find it.
// macOS (homebrew): /opt/homebrew/lib/libSDL2.dylib
// Linux: libSDL2-2.0.so.0 (apt: libsdl2-2.0-0)

@extern(C) {
    // Opaque handles — only `*SDL_Window` is reachable from ilang code.
    // Outside the block these flow as plain `i64` raw addresses.
    struct SDL_Window {}

    // Subset of SDL2's C API we need for the demo. The full ilang names
    // mirror the C symbols 1:1 (no `@symbol` overrides).
    @lib("SDL2") fn SDL_Init(flags: u32): i32
    @lib("SDL2") fn SDL_Quit()
    @lib("SDL2") fn SDL_Delay(ms: u32)
    @lib("SDL2") fn SDL_GetError(): *const char
    @lib("SDL2") fn SDL_CreateWindow(
        title: *const char,
        x: i32, y: i32, w: i32, h: i32,
        flags: u32
    ): *SDL_Window
    @lib("SDL2") fn SDL_DestroyWindow(window: *SDL_Window)

    // ─── ilang-friendly wrappers ─────────────────────────────────
    // These hide raw `*const char` / `*SDL_Window` from caller code;
    // SDL handles flow as `i64` (raw addresses) with `0` meaning NULL.

    // Returns 0 on success, < 0 on failure (matches SDL_Init's contract).
    // Pass SDL_INIT_VIDEO etc. in `flags`.
    fn init(flags: u32): i32 {
        SDL_Init(flags)
    }

    fn quit() {
        SDL_Quit()
    }

    fn delay(ms: u32) {
        SDL_Delay(ms)
    }

    // Latest SDL error message (or empty string when none).
    fn getError(): string {
        stringFromCstr(SDL_GetError())
    }

    // Returns the window pointer as i64. `0` indicates failure
    // (caller should consult `getError()`).
    fn createWindow(title: string, w: i32, h: i32): i64 {
        // SDL_WINDOWPOS_UNDEFINED = 0x1FFF0000 (centred-on-default-display).
        // Cast to i32 — SDL takes the position as a signed int. 0x1FFF0000
        // is a positive value so the bit pattern survives the cast intact.
        let undef = 0x1FFF0000 as i32
        let flags = 0 as u32           // no special window flags
        let win = SDL_CreateWindow(
            cstrFromString(title), undef, undef, w, h, flags
        )
        win as i64
    }

    fn destroyWindow(window: i64) {
        SDL_DestroyWindow(window as *SDL_Window)
    }
}

// SDL_INIT_VIDEO = 0x00000020. Other flags worth knowing:
//   SDL_INIT_AUDIO  = 0x00000010
//   SDL_INIT_TIMER  = 0x00000001
//   SDL_INIT_EVENTS = 0x00004000
const SDL_INIT_VIDEO: i64 = 0x20

// ─── main ────────────────────────────────────────────────────────

let rc = init(SDL_INIT_VIDEO as u32)
if rc < 0 as i32 {
    console.log("SDL_Init failed: " + getError())
} else {
    let win = createWindow("ilang × SDL2", 640 as i32, 480 as i32)
    if win == 0 {
        console.log("SDL_CreateWindow failed: " + getError())
    } else {
        console.log("window opened — closing in 2 seconds")
        delay(2000 as u32)
        destroyWindow(win)
    }
    quit()
}
