// SDL2 core: subsystem init / quit and the latest-error helper.
// Used by every other SDL2 module so it makes sense to import this
// first from the main program.

@extern(C) {
    @lib("SDL2") fn SDL_Init(flags: u32): i32
    @lib("SDL2") fn SDL_Quit()
    @lib("SDL2") fn SDL_GetError(): *const char

    // Returns 0 on success, < 0 on failure (matches SDL_Init).
    fn init(flags: u32): i32 {
        SDL_Init(flags)
    }

    fn quit() {
        SDL_Quit()
    }

    // Latest SDL error message (or empty string when none).
    fn getError(): string {
        stringFromCstr(SDL_GetError())
    }
}

// Init flag constants. Pass these (OR'd together) to `init`.
const INIT_TIMER: i64 = 0x00000001
const INIT_AUDIO: i64 = 0x00000010
const INIT_VIDEO: i64 = 0x00000020
const INIT_EVENTS: i64 = 0x00004000
