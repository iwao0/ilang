// Procedurally-generated GPU resources used by the world renderer:
//   • starTex   — neon star (sprite for particle bursts)
//   • glowTex   — radial gradient (brick highlight bloom)
//   • crtTex    — full-window scanline + vignette overlay
//   • sceneTex  — TARGET-access scratch for the world
//   • bloomTex  — TARGET-access tiny buffer for the blur pass
//
// Each is built once at construction. `init` returns early on the
// first failed `SDL_CreateTexture`; the caller can detect that by
// checking `<this>.starTex.handle == 0`.

use sdl
use sprites
use consts

pub class Assets {
    pub starTex: sdl.Texture
    pub glowTex: sdl.Texture
    pub crtTex: sdl.Texture
    pub sceneTex: sdl.Texture
    pub bloomTex: sdl.Texture

    pub init(ren: sdl.Renderer) {
        // White star sprite for particle bursts. Tinted at draw
        // time via `setColorMod`, rotated via `drawTextureRot`.
        this.starTex = new sdl.Texture(ren, consts.starTexSize, consts.starTexSize, 1)
        if this.starTex.handle == 0 {
            console.log("SDL_CreateTexture failed: " + sdl.getError())
            return
        }
        this.starTex.update(sprites.makeStarSprite())

        // Soft radial-gradient sprite, tinted per-brick at draw
        // time so the wave highlight blooms as a blurry circle
        // rather than a rectangle.
        this.glowTex = new sdl.Texture(ren, consts.glowTexSize, consts.glowTexSize, 1)
        if this.glowTex.handle == 0 {
            console.log("SDL_CreateTexture failed: " + sdl.getError())
            return
        }
        this.glowTex.update(sprites.makeGlowSprite())

        // Full-window CRT-effect overlay — built once. The alpha
        // channel encodes scanlines + vignette, the colour is
        // black, so blending it on top darkens each pixel by the
        // right amount.
        this.crtTex = new sdl.Texture(ren, consts.winW, consts.winH, 1)
        if this.crtTex.handle == 0 {
            console.log("SDL_CreateTexture failed: " + sdl.getError())
            return
        }
        this.crtTex.update(sprites.makeCrtOverlay(consts.winW, consts.winH))

        // Off-screen accumulator for the world. Drawn into each
        // frame (with a small per-frame fade applied first) and
        // then composited back to the window with chromatic-
        // aberration channel offsets.
        // SDL_TEXTUREACCESS_TARGET = 2.
        this.sceneTex = new sdl.Texture(ren, consts.winW, consts.winH, 2)
        if this.sceneTex.handle == 0 {
            console.log("SDL_CreateTexture(TARGET) failed: " + sdl.getError())
            return
        }
        // Linear filtering on the scene texture so that the bloom
        // pass's downscale + upscale produces a smooth blur
        // instead of a chunky pixel grid.
        this.sceneTex.setScaleMode(1)

        // Tiny target used to "blur" the scene. We draw `sceneTex`
        // shrunk into `bloomTex` (each downsample averages a 4×4
        // block thanks to linear filtering), and later draw
        // `bloomTex` back out at full window size additively —
        // the upscale spreads each averaged pixel across many
        // output pixels, giving a soft glow around the bright
        // stuff in the scene.
        this.bloomTex = new sdl.Texture(ren, consts.bloomW, consts.bloomH, 2)
        if this.bloomTex.handle == 0 {
            console.log("SDL_CreateTexture(bloom) failed: " + sdl.getError())
            return
        }
        this.bloomTex.setScaleMode(1)
        return
    }
}
