// SDL2 2D renderer: create a hardware-accelerated drawing context
// from an existing window, then issue colour / clear / fillRect /
// present commands. Filled rectangles act as the simplest
// "sprite" — no image loading involved.
//
// Window handles flow in as `i64` (raw addresses produced by
// `sdl_window.create`); the renderer hands out its own `i64`
// handle the same way.

@extern(C) {
    // Opaque renderer handle. The `void *` window incoming from
    // the caller is what SDL needs — we don't reference
    // sdl_window.SDL_Window across module boundaries.
    struct SDL_Renderer {}

    // C `SDL_Rect { int x, y, w, h }` — 16 B, passed by pointer.
    struct SDL_Rect {
        x: i32
        y: i32
        w: i32
        h: i32
    }

    @lib("SDL2") fn SDL_CreateRenderer(
        window: *void, index: i32, flags: u32
    ): *SDL_Renderer
    @lib("SDL2") fn SDL_DestroyRenderer(renderer: *SDL_Renderer)
    @lib("SDL2") fn SDL_SetRenderDrawColor(
        renderer: *SDL_Renderer, r: u8, g: u8, b: u8, a: u8
    ): i32
    @lib("SDL2") fn SDL_RenderClear(renderer: *SDL_Renderer): i32
    @lib("SDL2") fn SDL_RenderFillRect(
        renderer: *SDL_Renderer, rect: *SDL_Rect
    ): i32
    @lib("SDL2") fn SDL_RenderPresent(renderer: *SDL_Renderer)

    // Returns the renderer handle as i64. `0` indicates failure.
    // `index = -1` picks the first available driver; `flags = 0`
    // means default (driver-chosen accelerated when available).
    fn create(window: i64): i64 {
        SDL_CreateRenderer(window as *void, -1 as i32, 0 as u32) as i64
    }

    fn destroy(renderer: i64) {
        SDL_DestroyRenderer(renderer as *SDL_Renderer)
    }

    // Colour components are 0..=255 each (a = alpha).
    fn setColor(renderer: i64, r: u8, g: u8, b: u8, a: u8): i32 {
        SDL_SetRenderDrawColor(renderer as *SDL_Renderer, r, g, b, a)
    }

    // Wipe the back-buffer with the current draw colour.
    fn clear(renderer: i64): i32 {
        SDL_RenderClear(renderer as *SDL_Renderer)
    }

    // Fill an axis-aligned rectangle. Coordinates are in window
    // pixels; `(x, y)` is the top-left corner.
    fn fillRect(renderer: i64, x: i32, y: i32, w: i32, h: i32): i32 {
        let rect = new SDL_Rect()
        rect.x = x
        rect.y = y
        rect.w = w
        rect.h = h
        SDL_RenderFillRect(renderer as *SDL_Renderer, rect)
    }

    // Swap the back-buffer to the screen (vsync if the driver does it).
    fn present(renderer: i64) {
        SDL_RenderPresent(renderer as *SDL_Renderer)
    }
}
