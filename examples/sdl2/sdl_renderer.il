// SDL2 2D renderer: opaque handle + an ARC-managed `Renderer`
// class. Drawing methods (clear / fillRect / present / setColor)
// hang directly off Renderer so callers don't need to thread a
// raw handle through every call.
//
// Filled rectangles act as the simplest "sprite" — no image
// loading involved.

@extern(C) {
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

    class Renderer {
        handle: i64

        // Takes the window's i64 handle. Decoupling Renderer from
        // sdl_window's `Window` class avoids a cross-module type
        // reference here.
        init(window_handle: i64) {
            // index = -1 picks the first available driver;
            // flags = 0 means default (driver-chosen accelerated).
            this.handle = SDL_CreateRenderer(
                window_handle as *void, -1, 0
            ) as i64
        }

        deinit() {
            if this.handle != 0 {
                SDL_DestroyRenderer(this.handle as *SDL_Renderer)
            }
        }

        setColor(r: u8, g: u8, b: u8, a: u8): i32 {
            SDL_SetRenderDrawColor(this.handle as *SDL_Renderer, r, g, b, a)
        }

        clear(): i32 {
            SDL_RenderClear(this.handle as *SDL_Renderer)
        }

        // Fill an axis-aligned rectangle. Coordinates are in window
        // pixels; `(x, y)` is the top-left corner.
        fillRect(x: i32, y: i32, w: i32, h: i32): i32 {
            let rect = new SDL_Rect()
            rect.x = x
            rect.y = y
            rect.w = w
            rect.h = h
            SDL_RenderFillRect(this.handle as *SDL_Renderer, rect)
        }

        // Swap the back-buffer to the screen (vsync if the driver does it).
        present() {
            SDL_RenderPresent(this.handle as *SDL_Renderer)
        }
    }
}
