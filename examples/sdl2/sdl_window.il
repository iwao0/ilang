// SDL2 window create / destroy. Window handles flow as plain `i64`
// raw addresses outside this block; callers treat `0` as failure
// (consult `sdl_core.getError()` for the reason).

@extern(C) {
    // Opaque handle — only `*SDL_Window` is reachable from ilang code.
    struct SDL_Window {}

    @lib("SDL2") fn SDL_CreateWindow(
        title: *const char,
        x: i32, y: i32, w: i32, h: i32,
        flags: u32
    ): *SDL_Window
    @lib("SDL2") fn SDL_DestroyWindow(window: *SDL_Window)

    // Returns the window pointer as i64. `0` indicates failure.
    fn create(title: string, w: i32, h: i32): i64 {
        // SDL_WINDOWPOS_UNDEFINED = 0x1FFF0000 (centred-on-default-display).
        let undef = 0x1FFF0000 as i32
        let flags = 0 as u32
        let win = SDL_CreateWindow(
            cstrFromString(title), undef, undef, w, h, flags
        )
        win as i64
    }

    fn destroy(window: i64) {
        SDL_DestroyWindow(window as *SDL_Window)
    }
}
