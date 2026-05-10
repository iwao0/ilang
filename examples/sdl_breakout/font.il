// Tiny bitmap text renderer used by the breakout HUD. Sits on top
// of the public `sdl.Texture` / `sdl.Renderer` API — no raw FFI —
// so the SDL2 bindings stay focused on wrapping the C surface.
//
// Builds a 5×7 pixel font atlas on construction (one streaming
// `sdl.Texture` sized `5*N × 7`) and exposes a single
// `drawText(renderer, text, x, y, scale, r, g, b, a)` method. The
// atlas is white-on-transparent so colour-mod + alpha-mod tinting
// at draw time picks the per-call colour.
//
// Supported characters: A–Z (uppercase only), 0–9, space, and
// `.`, `:`, `,`, `-`, `!`, `?`. Lowercase / Unicode characters
// are skipped silently.

use sdl

pub class Font {
    /// Atlas texture. Owned by this Font — its `deinit` frees the
    /// underlying SDL_Texture when the Font drops.
    pub atlas: sdl.Texture
    /// Glyph cell width in source pixels (`5`).
    pub cellW: i32
    /// Glyph cell height in source pixels (`7`).
    pub cellH: i32
    /// Number of glyphs packed into the atlas.
    pub cellCount: i32

    /// Build the atlas texture against `renderer`.
    pub init(renderer: sdl.Renderer) {
        let glyphs = font_glyphs()
        this.cellW = 5
        this.cellH = 7
        this.cellCount = glyphs.length as i32
        let atlas_w = this.cellCount * this.cellW
        // Build the atlas pixel buffer: row-major RGBA8. Each
        // glyph occupies a 5×7 cell laid out left-to-right.
        let pixels: u8[] = []
        let y: i32 = 0
        while y < this.cellH {
            let x: i32 = 0
            while x < atlas_w {
                let glyph_idx = x / this.cellW
                let col = x - glyph_idx * this.cellW
                // Bit position inside the glyph: top-left first,
                // 5 bits per row, 7 rows. MSB → bit (cellW-1).
                let bit_pos = (this.cellH - 1 - y) * this.cellW
                            + (this.cellW - 1 - col)
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
        // STREAMING access — uploads pixel data via update().
        this.atlas = new sdl.Texture(renderer, atlas_w, this.cellH, 1)
        let _ = this.atlas.update(pixels)
    }

    /// Draw a string at `(x, y)`, top-left aligned.
    ///
    /// Args:
    ///   ren        — renderer to draw into.
    ///   text       — string to render.
    ///   x, y       — top-left of the first glyph.
    ///   scale      — integer scale factor (`1` = native 5×7).
    ///   r, g, b, a — RGBA tint applied to the white atlas.
    pub drawText(
        ren: sdl.Renderer, text: string, x: i32, y: i32, scale: i32,
        r: u8, g: u8, b: u8, a: u8
    ) {
        this.atlas.setColorMod(r, g, b)
        this.atlas.setAlphaMod(a)
        let cursor: i32 = x
        let i: i64 = 0
        let n: i64 = text.length
        while i < n {
            let ch = text.charAt(i)
            let idx = font_char_index(ch)
            if idx >= 0 {
                let src = new sdl.Rect()
                src.x = idx * this.cellW
                src.y = 0
                src.w = this.cellW
                src.h = this.cellH
                let dst = new sdl.Rect()
                dst.x = cursor
                dst.y = y
                dst.w = this.cellW * scale
                dst.h = this.cellH * scale
                ren.copy(this.atlas, src, dst)
            }
            cursor += (this.cellW + 1) * scale
            i += 1
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
