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

    // ARC-managed wrapper. `init` opens the window and stashes the
    // raw pointer as i64 so users can interact with it as a regular
    // ilang value; `deinit` releases it when the last reference
    // goes away.
    class Window {
        handle: i64

        init(title: string, w: i32, h: i32) {
            // SDL_WINDOWPOS_UNDEFINED = 0x1FFF0000 (centred-on-default-display).
            let undef = 0x1FFF0000 as i32
            let flags = 0 as u32
            let win = SDL_CreateWindow(
                cstrFromString(title), undef, undef, w, h, flags
            )
            this.handle = win as i64
        }

        deinit() {
            if this.handle != 0 {
                SDL_DestroyWindow(this.handle as *SDL_Window)
            }
        }
    }
}
