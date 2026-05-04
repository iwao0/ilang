// SDL2 window: opaque handle + an ARC-managed `Window` class that
// wraps creation / destruction. Because the class lives inside the
// `@extern(C) {}` block, its method bodies can call the C extern
// fns and marshalling helpers (`cstrFromString`, `as *const char`,
// …) directly — same scope as the bindings themselves.

@extern(C) {
    // Opaque C type; only the `*SDL_Window` pointer escapes the
    // block (and even that ends up wrapped in `Window` below).
    struct SDL_Window {}

    @lib("SDL2") fn SDL_CreateWindow(
        title: *const char,
        x: i32, y: i32, w: i32, h: i32,
        flags: u32
    ): *SDL_Window
    @lib("SDL2") fn SDL_DestroyWindow(window: *SDL_Window)
    @lib("SDL2") fn SDL_SetWindowFullscreen(window: *SDL_Window, flags: u32): i32

    // ARC-managed wrapper. `init` opens the window and stashes the
    // raw pointer as i64 so users can interact with it as a regular
    // ilang value; `deinit` releases it when the last reference
    // goes away.
    class Window {
        handle: i64

        init(title: string, w: i32, h: i32) {
            // SDL_WINDOWPOS_UNDEFINED = 0x1FFF0000 (centred-on-default-display).
            let undef: i32 = 0x1FFF0000
            let flags: u32 = 0
            let win = SDL_CreateWindow(
                cstrFromString(title), undef, undef, w, h, flags
            )
            this.handle = win as i64
        }

        // Toggle "fullscreen-desktop" mode. SDL handles the actual
        // resolution; combine with `Renderer.setLogicalSize` to keep
        // the game's rendering coordinates fixed and let SDL
        // letterbox.
        setFullscreen(on: bool): i32 {
            let flag: u32 = if on { 0x1001 as u32 } else { 0 as u32 }
            SDL_SetWindowFullscreen(this.handle as *SDL_Window, flag)
        }

        deinit() {
            if this.handle != 0 {
                SDL_DestroyWindow(this.handle as *SDL_Window)
            }
        }
    }
}
