// SDL2 2D renderer: opaque handle + an ARC-managed `Renderer`
// class. Drawing methods (clear / fillRect / present / setColor /
// drawTexture / drawTextureRot) hang directly off Renderer so
// callers don't need to thread a raw handle through every call.
//
// Sprite support: `Texture` wraps a streaming SDL_Texture you
// upload pixel bytes into via `update(rgba: u8[])`. Pixel format
// is RGBA8888 (1 byte each, in the order R, G, B, A on
// little-endian — SDL_PIXELFORMAT_ABGR8888). Renderer draws
// textures with optional rotation through `drawTextureRot`.

@extern(C) {
    struct SDL_Renderer {}

    // Opaque texture — created via SDL_CreateTexture, lives on
    // the GPU side, freed via SDL_DestroyTexture.
    struct SDL_Texture {}

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
    @lib("SDL2") fn SDL_SetRenderDrawBlendMode(
        renderer: *SDL_Renderer, blendMode: i32
    ): i32
    @lib("SDL2") fn SDL_RenderClear(renderer: *SDL_Renderer): i32
    @lib("SDL2") fn SDL_RenderFillRect(
        renderer: *SDL_Renderer, rect: *SDL_Rect
    ): i32
    @lib("SDL2") fn SDL_RenderPresent(renderer: *SDL_Renderer)

    @lib("SDL2") fn SDL_CreateTexture(
        renderer: *SDL_Renderer,
        format: u32, access: i32,
        w: i32, h: i32
    ): *SDL_Texture
    @lib("SDL2") fn SDL_DestroyTexture(tex: *SDL_Texture)
    @lib("SDL2") fn SDL_UpdateTexture(
        tex: *SDL_Texture, rect: *SDL_Rect,
        pixels: *const void, pitch: i32
    ): i32
    @lib("SDL2") fn SDL_SetTextureBlendMode(
        tex: *SDL_Texture, blendMode: i32
    ): i32
    @lib("SDL2") fn SDL_SetTextureColorMod(
        tex: *SDL_Texture, r: u8, g: u8, b: u8
    ): i32
    @lib("SDL2") fn SDL_SetTextureAlphaMod(
        tex: *SDL_Texture, alpha: u8
    ): i32
    @lib("SDL2") fn SDL_RenderCopy(
        renderer: *SDL_Renderer, tex: *SDL_Texture,
        srcrect: *SDL_Rect, dstrect: *SDL_Rect
    ): i32
    @lib("SDL2") fn SDL_RenderCopyEx(
        renderer: *SDL_Renderer, tex: *SDL_Texture,
        srcrect: *SDL_Rect, dstrect: *SDL_Rect,
        angle: f64, center: *void, flip: i32
    ): i32

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
            // Enable alpha blending so transparent texture pixels
            // composite over the background instead of replacing it.
            // SDL_BLENDMODE_BLEND = 1.
            if this.handle != 0 {
                SDL_SetRenderDrawBlendMode(this.handle as *SDL_Renderer, 1)
            }
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

        // Stretch the entire texture into the destination rect.
        drawTexture(tex_handle: i64, x: i32, y: i32, w: i32, h: i32): i32 {
            let dst = new SDL_Rect()
            dst.x = x
            dst.y = y
            dst.w = w
            dst.h = h
            SDL_RenderCopy(
                this.handle as *SDL_Renderer,
                tex_handle as *SDL_Texture,
                0 as *SDL_Rect, dst
            )
        }

        // Same as drawTexture but rotates around the dst rect's
        // centre by `angle` degrees (clockwise).
        drawTextureRot(
            tex_handle: i64, x: i32, y: i32, w: i32, h: i32, angle: f64
        ): i32 {
            let dst = new SDL_Rect()
            dst.x = x
            dst.y = y
            dst.w = w
            dst.h = h
            SDL_RenderCopyEx(
                this.handle as *SDL_Renderer,
                tex_handle as *SDL_Texture,
                0 as *SDL_Rect, dst,
                angle, 0 as *void, 0
            )
        }

        // Swap the back-buffer to the screen (vsync if the driver does it).
        present() {
            SDL_RenderPresent(this.handle as *SDL_Renderer)
        }
    }

    // ARC-managed sprite. Holds an SDL_Texture sized at construction
    // and lets you upload raw RGBA8 pixels (`r, g, b, a` byte order
    // on little-endian — SDL_PIXELFORMAT_ABGR8888 = 0x16762004).
    // Use Renderer.drawTexture / drawTextureRot to draw it.
    class Texture {
        handle: i64
        width: i32
        height: i32

        init(renderer: Renderer, w: i32, h: i32) {
            this.width = w
            this.height = h
            // SDL_PIXELFORMAT_ABGR8888 = 0x16762004 — bytes laid
            // out as R, G, B, A on little-endian, which is what
            // we'll feed in. SDL_TEXTUREACCESS_STREAMING = 1, lets
            // SDL_UpdateTexture push pixels efficiently.
            this.handle = SDL_CreateTexture(
                renderer.handle as *SDL_Renderer,
                0x16762004 as u32, 1,
                w, h
            ) as i64
            // Enable alpha so transparent texture pixels show
            // through. SDL_BLENDMODE_BLEND = 1.
            if this.handle != 0 {
                SDL_SetTextureBlendMode(this.handle as *SDL_Texture, 1)
            }
        }

        deinit() {
            if this.handle != 0 {
                SDL_DestroyTexture(this.handle as *SDL_Texture)
            }
        }

        // Upload pixel data. The buffer must be exactly
        // `width * height * 4` bytes (RGBA8 per pixel).
        update(rgba: u8[]): i32 {
            // pitch = bytes per row. For full-texture updates the
            // rect arg is NULL.
            let pitch = this.width * 4
            let p: *u8 = rgba
            SDL_UpdateTexture(
                this.handle as *SDL_Texture, 0 as *SDL_Rect,
                p as *const void, pitch
            )
        }

        // Optional per-draw tinting / fading.
        setColorMod(r: u8, g: u8, b: u8): i32 {
            SDL_SetTextureColorMod(this.handle as *SDL_Texture, r, g, b)
        }
        setAlphaMod(a: u8): i32 {
            SDL_SetTextureAlphaMod(this.handle as *SDL_Texture, a)
        }
    }
}
