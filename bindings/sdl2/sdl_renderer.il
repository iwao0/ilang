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

    // Bitmap text. Builds a small 5×7 pixel font on construction
    // (one atlas SDL_Texture sized 5*N × 7) and exposes a single
    // `drawText(renderer, text, x, y, scale, r, g, b, a)` method.
    // The atlas is white-on-transparent so `setColorMod` / alpha
    // tinting on draw lets each call pick its own colour.
    //
    // Supported characters: A–Z (uppercase only), 0–9, space, and
    // `.`, `:`, `,`, `-`, `!`, `?`. Lowercase / Unicode characters
    // are skipped silently.
    class Font {
        atlas: i64
        cell_w: i32
        cell_h: i32
        cell_count: i32

        init(renderer: Renderer) {
            let glyphs = font_glyphs()
            this.cell_w = 5
            this.cell_h = 7
            this.cell_count = glyphs.length as i32
            let atlas_w = this.cell_count * this.cell_w
            // Build the atlas pixel buffer: row-major RGBA8. Each
            // glyph occupies a 5×7 cell laid out left-to-right.
            let pixels: u8[] = []
            let y: i32 = 0
            while y < this.cell_h {
                let x: i32 = 0
                while x < atlas_w {
                    let glyph_idx = x / this.cell_w
                    let col = x - glyph_idx * this.cell_w
                    // Bit position inside the glyph: top-left first,
                    // 5 bits per row, 7 rows. MSB → bit (cell_w-1).
                    let bit_pos = (this.cell_h - 1 - y) * this.cell_w
                                + (this.cell_w - 1 - col)
                    let bit = (glyphs[glyph_idx] >> (bit_pos as i64)) & 1
                    if bit != 0 {
                        pixels.push(255)
                        pixels.push(255)
                        pixels.push(255)
                        pixels.push(255)
                    } else {
                        pixels.push(0)
                        pixels.push(0)
                        pixels.push(0)
                        pixels.push(0)
                    }
                    x += 1
                }
                y += 1
            }
            // SDL_PIXELFORMAT_ABGR8888 / STREAMING — same as Texture.
            this.atlas = SDL_CreateTexture(
                renderer.handle as *SDL_Renderer,
                0x16762004 as u32, 1,
                atlas_w, this.cell_h
            ) as i64
            if this.atlas != 0 {
                SDL_SetTextureBlendMode(this.atlas as *SDL_Texture, 1)
                let p: *u8 = pixels
                SDL_UpdateTexture(
                    this.atlas as *SDL_Texture, 0 as *SDL_Rect,
                    p as *const void, atlas_w * 4
                )
            }
        }

        deinit() {
            if this.atlas != 0 {
                SDL_DestroyTexture(this.atlas as *SDL_Texture)
            }
        }

        // Draw `text` at `(x, y)`, scaled by `scale` (1 = native
        // 5×7), tinted to RGBA `(r, g, b, a)`. Top-left of the first
        // glyph lands at `(x, y)`.
        drawText(
            ren: Renderer, text: string, x: i32, y: i32, scale: i32,
            r: u8, g: u8, b: u8, a: u8
        ) {
            SDL_SetTextureColorMod(this.atlas as *SDL_Texture, r, g, b)
            SDL_SetTextureAlphaMod(this.atlas as *SDL_Texture, a)
            let cursor: i32 = x
            let i: i64 = 0
            let n: i64 = text.length
            while i < n {
                let ch = text.charAt(i)
                let idx = font_char_index(ch)
                if idx >= 0 {
                    let src = new SDL_Rect()
                    src.x = idx * this.cell_w
                    src.y = 0
                    src.w = this.cell_w
                    src.h = this.cell_h
                    let dst = new SDL_Rect()
                    dst.x = cursor
                    dst.y = y
                    dst.w = this.cell_w * scale
                    dst.h = this.cell_h * scale
                    SDL_RenderCopy(
                        ren.handle as *SDL_Renderer,
                        this.atlas as *SDL_Texture,
                        src, dst
                    )
                }
                cursor += (this.cell_w + 1) * scale
                i += 1
            }
        }
    }
}

// 5×7 bitmap font, packed as a 35-bit value per glyph (top-left
// pixel = MSB at bit 34, top row first, left-to-right within each
// row). Order matches `FONT_ALPHABET` in `font_char_index`.
fn font_glyphs(): i64[] {
    let g: i64[] = [
        // A–E
        rows_to_glyph(0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001),
        rows_to_glyph(0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110),
        rows_to_glyph(0b01111, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b01111),
        rows_to_glyph(0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110),
        rows_to_glyph(0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111),
        // F–J
        rows_to_glyph(0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000),
        rows_to_glyph(0b01111, 0b10000, 0b10000, 0b10111, 0b10001, 0b10001, 0b01110),
        rows_to_glyph(0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001),
        rows_to_glyph(0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b11111),
        rows_to_glyph(0b00111, 0b00010, 0b00010, 0b00010, 0b00010, 0b10010, 0b01100),
        // K–O
        rows_to_glyph(0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001),
        rows_to_glyph(0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111),
        rows_to_glyph(0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001),
        rows_to_glyph(0b10001, 0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001),
        rows_to_glyph(0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110),
        // P–T
        rows_to_glyph(0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000),
        rows_to_glyph(0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101),
        rows_to_glyph(0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001),
        rows_to_glyph(0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110),
        rows_to_glyph(0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100),
        // U–Z
        rows_to_glyph(0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110),
        rows_to_glyph(0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100),
        rows_to_glyph(0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b10101, 0b01010),
        rows_to_glyph(0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001),
        rows_to_glyph(0b10001, 0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100),
        rows_to_glyph(0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111),
        // 0–4
        rows_to_glyph(0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110),
        rows_to_glyph(0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110),
        rows_to_glyph(0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111),
        rows_to_glyph(0b11111, 0b00010, 0b00100, 0b00010, 0b00001, 0b10001, 0b01110),
        rows_to_glyph(0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010),
        // 5–9
        rows_to_glyph(0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110),
        rows_to_glyph(0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110),
        rows_to_glyph(0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b10000),
        rows_to_glyph(0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110),
        rows_to_glyph(0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100),
        // " "  "."  ":"  ","  "-"  "!"  "?"
        rows_to_glyph(0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000),
        rows_to_glyph(0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00100, 0b00100),
        rows_to_glyph(0b00000, 0b00100, 0b00100, 0b00000, 0b00100, 0b00100, 0b00000),
        rows_to_glyph(0b00000, 0b00000, 0b00000, 0b00000, 0b00100, 0b00100, 0b01000),
        rows_to_glyph(0b00000, 0b00000, 0b00000, 0b11111, 0b00000, 0b00000, 0b00000),
        rows_to_glyph(0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00000, 0b00100),
        rows_to_glyph(0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b00000, 0b00100),
    ]
    g
}

fn rows_to_glyph(
    r0: i32, r1: i32, r2: i32, r3: i32, r4: i32, r5: i32, r6: i32
): i64 {
    ((r0 as i64) << 30)
        | ((r1 as i64) << 25)
        | ((r2 as i64) << 20)
        | ((r3 as i64) << 15)
        | ((r4 as i64) << 10)
        | ((r5 as i64) << 5)
        | (r6 as i64)
}

// Position of `c` inside the font's alphabet, or `-1` if missing.
// Linear scan — alphabet is short and called per glyph per frame.
fn font_char_index(c: string): i32 {
    let alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789 .:,-!?"
    let n: i64 = alphabet.length
    let i: i64 = 0
    let result: i32 = -1
    while i < n {
        if alphabet.charAt(i) == c {
            result = i as i32
            i = n
        } else {
            i = i + 1
        }
    }
    result
}
