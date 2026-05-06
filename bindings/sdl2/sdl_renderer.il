// SDL2 2D renderer (mirrors `SDL_render.h`): driver / renderer /
// texture / draw / geometry. The full C surface is exposed through
// `@extern(C)` declarations; the most-used calls are also wrapped
// as ARC-managed `Renderer` / `Texture` methods so callers don't
// have to thread raw handles around.
//
// Sprite support: `Texture` wraps a streaming SDL_Texture you
// upload pixel bytes into via `update(rgba: u8[])`. Pixel format
// is RGBA8888 (1 byte each, in the order R, G, B, A on
// little-endian — SDL_PIXELFORMAT_ABGR8888). Renderer draws
// textures with optional rotation through `drawTextureRot`.

// ─── enums (mirror the SDL_render.h typedefs) ─────────────────────

/// SDL_RendererFlags — passed to `SDL_CreateRenderer` / new Renderer.
@flags
enum RendererFlags: u32 {
    software      = 0x00000001
    accelerated   = 0x00000002
    presentVsync  = 0x00000004
    targetTexture = 0x00000008
}

/// SDL_ScaleMode — texture sampling filter.
enum ScaleMode: i32 {
    nearest = 0
    linear  = 1
    best    = 2
}

/// SDL_TextureAccess.
enum TextureAccess: i32 {
    static    = 0
    streaming = 1
    target    = 2
}

/// SDL_TextureModulate — multiplied against texture color/alpha on draw.
@flags
enum TextureModulate: i32 {
    none  = 0x00000000
    color = 0x00000001
    alpha = 0x00000002
}

/// SDL_RendererFlip — bit flags for `RenderCopyEx`.
@flags
enum RendererFlip: i32 {
    none       = 0x00000000
    horizontal = 0x00000001
    vertical   = 0x00000002
}

@extern(C) {
    // Opaque handles (forward-declared in SDL_render.h).
    struct SDL_Renderer {}
    struct SDL_Texture {}

    // SDL_render.h shared geometry types.
    struct SDL_Point {
        x: i32
        y: i32
    }
    struct SDL_FPoint {
        x: f32
        y: f32
    }
    struct SDL_FRect {
        x: f32
        y: f32
        w: f32
        h: f32
    }
    struct SDL_Color {
        r: u8
        g: u8
        b: u8
        a: u8
    }
    struct SDL_Vertex {
        position: SDL_FPoint
        color: SDL_Color
        tex_coord: SDL_FPoint
    }
    struct SDL_RendererInfo {
        name: *const char
        flags: u32
        num_texture_formats: u32
        texture_formats: u32[16]
        max_texture_width: i32
        max_texture_height: i32
    }

    // ─── driver / renderer lifecycle ──────────────────────────────
    @lib("SDL2") fn SDL_GetNumRenderDrivers(): i32
    @lib("SDL2") fn SDL_GetRenderDriverInfo(
        index: i32, info: *SDL_RendererInfo
    ): i32
    @lib("SDL2") fn SDL_CreateWindowAndRenderer(
        width: i32, height: i32, window_flags: u32,
        window: **void, renderer: **SDL_Renderer
    ): i32
    @lib("SDL2") fn SDL_CreateRenderer(
        window: *void, index: i32, flags: u32
    ): *SDL_Renderer
    @lib("SDL2") fn SDL_CreateSoftwareRenderer(surface: *void): *SDL_Renderer
    @lib("SDL2") fn SDL_GetRenderer(window: *void): *SDL_Renderer
    @lib("SDL2") fn SDL_RenderGetWindow(renderer: *SDL_Renderer): *void
    @lib("SDL2") fn SDL_GetRendererInfo(
        renderer: *SDL_Renderer, info: *SDL_RendererInfo
    ): i32
    @lib("SDL2") fn SDL_GetRendererOutputSize(
        renderer: *SDL_Renderer, w: *i32, h: *i32
    ): i32
    @lib("SDL2") fn SDL_DestroyRenderer(renderer: *SDL_Renderer)
    @lib("SDL2") fn SDL_RenderFlush(renderer: *SDL_Renderer): i32

    // ─── texture lifecycle / metadata ─────────────────────────────
    @lib("SDL2") fn SDL_CreateTexture(
        renderer: *SDL_Renderer,
        format: u32, access: i32,
        w: i32, h: i32
    ): *SDL_Texture
    @lib("SDL2") fn SDL_CreateTextureFromSurface(
        renderer: *SDL_Renderer, surface: *void
    ): *SDL_Texture
    @lib("SDL2") fn SDL_QueryTexture(
        tex: *SDL_Texture,
        format: *u32, access: *i32, w: *i32, h: *i32
    ): i32
    @lib("SDL2") fn SDL_DestroyTexture(tex: *SDL_Texture)

    @lib("SDL2") fn SDL_SetTextureColorMod(
        tex: *SDL_Texture, r: u8, g: u8, b: u8
    ): i32
    @lib("SDL2") fn SDL_GetTextureColorMod(
        tex: *SDL_Texture, r: *u8, g: *u8, b: *u8
    ): i32
    @lib("SDL2") fn SDL_SetTextureAlphaMod(
        tex: *SDL_Texture, alpha: u8
    ): i32
    @lib("SDL2") fn SDL_GetTextureAlphaMod(
        tex: *SDL_Texture, alpha: *u8
    ): i32
    @lib("SDL2") fn SDL_SetTextureBlendMode(
        tex: *SDL_Texture, blendMode: i32
    ): i32
    @lib("SDL2") fn SDL_GetTextureBlendMode(
        tex: *SDL_Texture, blendMode: *i32
    ): i32
    @lib("SDL2") fn SDL_SetTextureScaleMode(
        tex: *SDL_Texture, scaleMode: i32
    ): i32
    @lib("SDL2") fn SDL_GetTextureScaleMode(
        tex: *SDL_Texture, scaleMode: *i32
    ): i32
    @lib("SDL2") fn SDL_SetTextureUserData(
        tex: *SDL_Texture, userdata: *void
    ): i32
    @lib("SDL2") fn SDL_GetTextureUserData(tex: *SDL_Texture): *void

    @lib("SDL2") fn SDL_UpdateTexture(
        tex: *SDL_Texture, rect: *sdl.SDL_Rect,
        pixels: *const void, pitch: i32
    ): i32
    @lib("SDL2") fn SDL_UpdateYUVTexture(
        tex: *SDL_Texture, rect: *sdl.SDL_Rect,
        Yplane: *const u8, Ypitch: i32,
        Uplane: *const u8, Upitch: i32,
        Vplane: *const u8, Vpitch: i32
    ): i32
    @lib("SDL2") fn SDL_UpdateNVTexture(
        tex: *SDL_Texture, rect: *sdl.SDL_Rect,
        Yplane: *const u8, Ypitch: i32,
        UVplane: *const u8, UVpitch: i32
    ): i32
    @lib("SDL2") fn SDL_LockTexture(
        tex: *SDL_Texture, rect: *sdl.SDL_Rect,
        pixels: **void, pitch: *i32
    ): i32
    @lib("SDL2") fn SDL_LockTextureToSurface(
        tex: *SDL_Texture, rect: *sdl.SDL_Rect, surface: **void
    ): i32
    @lib("SDL2") fn SDL_UnlockTexture(tex: *SDL_Texture)

    // ─── render target / logical size / viewport / clip / scale ──
    @lib("SDL2") fn SDL_RenderTargetSupported(
        renderer: *SDL_Renderer
    ): i32
    @lib("SDL2") fn SDL_SetRenderTarget(
        renderer: *SDL_Renderer, tex: *SDL_Texture
    ): i32
    @lib("SDL2") fn SDL_GetRenderTarget(
        renderer: *SDL_Renderer
    ): *SDL_Texture

    @lib("SDL2") fn SDL_RenderSetLogicalSize(
        renderer: *SDL_Renderer, w: i32, h: i32
    ): i32
    @lib("SDL2") fn SDL_RenderGetLogicalSize(
        renderer: *SDL_Renderer, w: *i32, h: *i32
    )
    @lib("SDL2") fn SDL_RenderSetIntegerScale(
        renderer: *SDL_Renderer, enable: i32
    ): i32
    @lib("SDL2") fn SDL_RenderGetIntegerScale(
        renderer: *SDL_Renderer
    ): i32
    @lib("SDL2") fn SDL_RenderSetViewport(
        renderer: *SDL_Renderer, rect: *sdl.SDL_Rect
    ): i32
    @lib("SDL2") fn SDL_RenderGetViewport(
        renderer: *SDL_Renderer, rect: *sdl.SDL_Rect
    )
    @lib("SDL2") fn SDL_RenderSetClipRect(
        renderer: *SDL_Renderer, rect: *sdl.SDL_Rect
    ): i32
    @lib("SDL2") fn SDL_RenderGetClipRect(
        renderer: *SDL_Renderer, rect: *sdl.SDL_Rect
    )
    @lib("SDL2") fn SDL_RenderIsClipEnabled(
        renderer: *SDL_Renderer
    ): i32
    @lib("SDL2") fn SDL_RenderSetScale(
        renderer: *SDL_Renderer, scaleX: f32, scaleY: f32
    ): i32
    @lib("SDL2") fn SDL_RenderGetScale(
        renderer: *SDL_Renderer, scaleX: *f32, scaleY: *f32
    )
    @lib("SDL2") fn SDL_RenderWindowToLogical(
        renderer: *SDL_Renderer,
        windowX: i32, windowY: i32,
        logicalX: *f32, logicalY: *f32
    )
    @lib("SDL2") fn SDL_RenderLogicalToWindow(
        renderer: *SDL_Renderer,
        logicalX: f32, logicalY: f32,
        windowX: *i32, windowY: *i32
    )

    // ─── draw colour / blend mode ─────────────────────────────────
    @lib("SDL2") fn SDL_SetRenderDrawColor(
        renderer: *SDL_Renderer, r: u8, g: u8, b: u8, a: u8
    ): i32
    @lib("SDL2") fn SDL_GetRenderDrawColor(
        renderer: *SDL_Renderer, r: *u8, g: *u8, b: *u8, a: *u8
    ): i32
    @lib("SDL2") fn SDL_SetRenderDrawBlendMode(
        renderer: *SDL_Renderer, blendMode: i32
    ): i32
    @lib("SDL2") fn SDL_GetRenderDrawBlendMode(
        renderer: *SDL_Renderer, blendMode: *i32
    ): i32

    // ─── primitive draw / fill (integer coords) ───────────────────
    @lib("SDL2") fn SDL_RenderClear(renderer: *SDL_Renderer): i32
    @lib("SDL2") fn SDL_RenderDrawPoint(
        renderer: *SDL_Renderer, x: i32, y: i32
    ): i32
    @lib("SDL2") fn SDL_RenderDrawPoints(
        renderer: *SDL_Renderer, points: *const SDL_Point, count: i32
    ): i32
    @lib("SDL2") fn SDL_RenderDrawLine(
        renderer: *SDL_Renderer, x1: i32, y1: i32, x2: i32, y2: i32
    ): i32
    @lib("SDL2") fn SDL_RenderDrawLines(
        renderer: *SDL_Renderer, points: *const SDL_Point, count: i32
    ): i32
    @lib("SDL2") fn SDL_RenderDrawRect(
        renderer: *SDL_Renderer, rect: *sdl.SDL_Rect
    ): i32
    @lib("SDL2") fn SDL_RenderDrawRects(
        renderer: *SDL_Renderer, rects: *const sdl.SDL_Rect, count: i32
    ): i32
    @lib("SDL2") fn SDL_RenderFillRect(
        renderer: *SDL_Renderer, rect: *sdl.SDL_Rect
    ): i32
    @lib("SDL2") fn SDL_RenderFillRects(
        renderer: *SDL_Renderer, rects: *const sdl.SDL_Rect, count: i32
    ): i32
    @lib("SDL2") fn SDL_RenderCopy(
        renderer: *SDL_Renderer, tex: *SDL_Texture,
        srcrect: *sdl.SDL_Rect, dstrect: *sdl.SDL_Rect
    ): i32
    @lib("SDL2") fn SDL_RenderCopyEx(
        renderer: *SDL_Renderer, tex: *SDL_Texture,
        srcrect: *sdl.SDL_Rect, dstrect: *sdl.SDL_Rect,
        angle: f64, center: *const SDL_Point, flip: i32
    ): i32

    // ─── primitive draw / fill (float coords) ─────────────────────
    @lib("SDL2") fn SDL_RenderDrawPointF(
        renderer: *SDL_Renderer, x: f32, y: f32
    ): i32
    @lib("SDL2") fn SDL_RenderDrawPointsF(
        renderer: *SDL_Renderer, points: *const SDL_FPoint, count: i32
    ): i32
    @lib("SDL2") fn SDL_RenderDrawLineF(
        renderer: *SDL_Renderer, x1: f32, y1: f32, x2: f32, y2: f32
    ): i32
    @lib("SDL2") fn SDL_RenderDrawLinesF(
        renderer: *SDL_Renderer, points: *const SDL_FPoint, count: i32
    ): i32
    @lib("SDL2") fn SDL_RenderDrawRectF(
        renderer: *SDL_Renderer, rect: *const SDL_FRect
    ): i32
    @lib("SDL2") fn SDL_RenderDrawRectsF(
        renderer: *SDL_Renderer, rects: *const SDL_FRect, count: i32
    ): i32
    @lib("SDL2") fn SDL_RenderFillRectF(
        renderer: *SDL_Renderer, rect: *const SDL_FRect
    ): i32
    @lib("SDL2") fn SDL_RenderFillRectsF(
        renderer: *SDL_Renderer, rects: *const SDL_FRect, count: i32
    ): i32
    @lib("SDL2") fn SDL_RenderCopyF(
        renderer: *SDL_Renderer, tex: *SDL_Texture,
        srcrect: *sdl.SDL_Rect, dstrect: *const SDL_FRect
    ): i32
    @lib("SDL2") fn SDL_RenderCopyExF(
        renderer: *SDL_Renderer, tex: *SDL_Texture,
        srcrect: *sdl.SDL_Rect, dstrect: *const SDL_FRect,
        angle: f64, center: *const SDL_FPoint, flip: i32
    ): i32

    // ─── geometry / read-back / present ───────────────────────────
    @lib("SDL2") fn SDL_RenderGeometry(
        renderer: *SDL_Renderer, tex: *SDL_Texture,
        vertices: *const SDL_Vertex, num_vertices: i32,
        indices: *const i32, num_indices: i32
    ): i32
    @lib("SDL2") fn SDL_RenderGeometryRaw(
        renderer: *SDL_Renderer, tex: *SDL_Texture,
        xy: *const f32, xy_stride: i32,
        color: *const SDL_Color, color_stride: i32,
        uv: *const f32, uv_stride: i32,
        num_vertices: i32,
        indices: *const void, num_indices: i32, size_indices: i32
    ): i32
    @lib("SDL2") fn SDL_RenderReadPixels(
        renderer: *SDL_Renderer, rect: *sdl.SDL_Rect,
        format: u32, pixels: *void, pitch: i32
    ): i32
    @lib("SDL2") fn SDL_RenderPresent(renderer: *SDL_Renderer)

    // ─── platform / GL / Metal / VSync ───────────────────────────
    @lib("SDL2") fn SDL_GL_BindTexture(
        tex: *SDL_Texture, texw: *f32, texh: *f32
    ): i32
    @lib("SDL2") fn SDL_GL_UnbindTexture(tex: *SDL_Texture): i32
    @lib("SDL2") fn SDL_RenderGetMetalLayer(
        renderer: *SDL_Renderer
    ): *void
    @lib("SDL2") fn SDL_RenderGetMetalCommandEncoder(
        renderer: *SDL_Renderer
    ): *void
    @lib("SDL2") fn SDL_RenderSetVSync(
        renderer: *SDL_Renderer, vsync: i32
    ): i32

    // ─── friendlier ilang wrappers ───────────────────────────────

    /// Number of available 2D render drivers.
    fn numDrivers(): i32 {
        SDL_GetNumRenderDrivers()
    }

    /// Public ilang return type for `driverInfo` / `Renderer.info`.
    /// Mirrors `SDL_RendererInfo` but converts the raw `*const char`
    /// name into an owned `string` and the fixed-length texture
    /// format slot into a `u32[]` so callers never see a raw
    /// pointer.
    class RendererInfo {
        name: string
        flags: u32
        texture_formats: u32[]
        max_texture_width: i32
        max_texture_height: i32
    }

    /// Driver info at `index` (0..numDrivers).
    fn driverInfo(index: i32): RendererInfo {
        let raw = new SDL_RendererInfo()
        SDL_GetRenderDriverInfo(index, raw)
        let info = new RendererInfo()
        info.name = stringFromCstr(raw.name)
        info.flags = raw.flags
        info.max_texture_width = raw.max_texture_width
        info.max_texture_height = raw.max_texture_height
        let n: i64 = raw.num_texture_formats as i64
        let fmts: u32[] = []
        let i: i64 = 0
        while i < n {
            fmts.push(raw.texture_formats[i])
            i += 1
        }
        info.texture_formats = fmts
        info
    }

    // Bundle for `outputSize` / `getLogicalSize` / `logicalToWindow`.
    // Renamed from `Size` to avoid clashing with the umbrella's
    // `sdl.Size` (defined in sdl_window).
    class RenderSize {
        w: i32
        h: i32
    }
    // Bundle for `getScale`.
    class RenderScale {
        x: f32
        y: f32
    }
    // Bundle for `windowToLogical`.
    class FCoord {
        x: f32
        y: f32
    }
    // Bundle for `Texture.query` (mirrors SDL_QueryTexture's four
    // out parameters).
    class TextureInfo {
        format: u32
        access: i32
        w: i32
        h: i32
    }

    class Renderer {
        handle: i64

        /// Build with the SDL defaults: first available driver,
        /// accelerated path chosen by SDL, BLEND mode pre-enabled.
        init(window: sdl.Window) {
            this.handle = SDL_CreateRenderer(
                window.handle as *void, -1, 0
            ) as i64
            // SDL_BLENDMODE_BLEND = 1.
            if this.handle != 0 {
                SDL_SetRenderDrawBlendMode(this.handle as *SDL_Renderer, 1)
            }
        }

        /// As above but lets the caller pick a driver index and
        /// pass `RendererFlags`.
        init(window: sdl.Window, index: i32, flags: u32) {
            this.handle = SDL_CreateRenderer(
                window.handle as *void, index, flags
            ) as i64
            if this.handle != 0 {
                SDL_SetRenderDrawBlendMode(this.handle as *SDL_Renderer, 1)
            }
        }

        deinit() {
            if this.handle != 0 {
                SDL_DestroyRenderer(this.handle as *SDL_Renderer)
            }
        }

        // ─── info ─────────────────────────────────────────────────

        /// Driver / capability info for *this* renderer.
        info(): RendererInfo {
            let raw = new SDL_RendererInfo()
            SDL_GetRendererInfo(this.handle as *SDL_Renderer, raw)
            let info = new RendererInfo()
            info.name = stringFromCstr(raw.name)
            info.flags = raw.flags
            info.max_texture_width = raw.max_texture_width
            info.max_texture_height = raw.max_texture_height
            let n: i64 = raw.num_texture_formats as i64
            let fmts: u32[] = []
            let i: i64 = 0
            while i < n {
                fmts.push(raw.texture_formats[i])
                i += 1
            }
            info.texture_formats = fmts
            info
        }

        /// Drawable output size in pixels (may differ from window
        /// size on HiDPI / logical-size setups).
        outputSize(): RenderSize {
            let w: i32[] = [0]
            let h: i32[] = [0]
            SDL_GetRendererOutputSize(this.handle as *SDL_Renderer, w, h)
            let s = new RenderSize()
            s.w = w[0]
            s.h = h[0]
            s
        }

        // ─── colour / blend mode ──────────────────────────────────

        setColor(r: u8, g: u8, b: u8, a: u8): i32 {
            SDL_SetRenderDrawColor(this.handle as *SDL_Renderer, r, g, b, a)
        }

        getColor(): SDL_Color {
            let c = new SDL_Color()
            let r: u8[] = [0]
            let g: u8[] = [0]
            let b: u8[] = [0]
            let a: u8[] = [0]
            SDL_GetRenderDrawColor(this.handle as *SDL_Renderer, r, g, b, a)
            c.r = r[0]
            c.g = g[0]
            c.b = b[0]
            c.a = a[0]
            c
        }

        setBlendMode(mode: i32): i32 {
            SDL_SetRenderDrawBlendMode(this.handle as *SDL_Renderer, mode)
        }

        getBlendMode(): i32 {
            let m: i32[] = [0]
            SDL_GetRenderDrawBlendMode(this.handle as *SDL_Renderer, m)
            m[0]
        }

        // ─── logical / viewport / clip / scale ────────────────────

        // Pin the renderer's coordinate system to a virtual w×h grid.
        // SDL stretches the result to fit the actual window (with
        // letterboxing when the aspect differs).
        setLogicalSize(w: i32, h: i32): i32 {
            SDL_RenderSetLogicalSize(this.handle as *SDL_Renderer, w, h)
        }

        getLogicalSize(): RenderSize {
            let w: i32[] = [0]
            let h: i32[] = [0]
            SDL_RenderGetLogicalSize(this.handle as *SDL_Renderer, w, h)
            let s = new RenderSize()
            s.w = w[0]
            s.h = h[0]
            s
        }

        /// Snap stretched logical coords to whole pixels.
        setIntegerScale(enable: bool): i32 {
            let v: i32 = if enable { 1 } else { 0 }
            SDL_RenderSetIntegerScale(this.handle as *SDL_Renderer, v)
        }

        getIntegerScale(): bool {
            SDL_RenderGetIntegerScale(this.handle as *SDL_Renderer) != 0
        }

        /// Pass `0 as *sdl.SDL_Rect` (or `null`-equivalent) to clear.
        setViewport(rect: sdl.SDL_Rect): i32 {
            SDL_RenderSetViewport(this.handle as *SDL_Renderer, rect)
        }

        clearViewport(): i32 {
            SDL_RenderSetViewport(
                this.handle as *SDL_Renderer, 0 as *sdl.SDL_Rect
            )
        }

        getViewport(): sdl.SDL_Rect {
            let r = new sdl.SDL_Rect()
            SDL_RenderGetViewport(this.handle as *SDL_Renderer, r)
            r
        }

        setClipRect(rect: sdl.SDL_Rect): i32 {
            SDL_RenderSetClipRect(this.handle as *SDL_Renderer, rect)
        }

        clearClipRect(): i32 {
            SDL_RenderSetClipRect(
                this.handle as *SDL_Renderer, 0 as *sdl.SDL_Rect
            )
        }

        getClipRect(): sdl.SDL_Rect {
            let r = new sdl.SDL_Rect()
            SDL_RenderGetClipRect(this.handle as *SDL_Renderer, r)
            r
        }

        isClipEnabled(): bool {
            SDL_RenderIsClipEnabled(this.handle as *SDL_Renderer) != 0
        }

        setScale(x: f32, y: f32): i32 {
            SDL_RenderSetScale(this.handle as *SDL_Renderer, x, y)
        }

        getScale(): RenderScale {
            let x: f32[] = [0.0 as f32]
            let y: f32[] = [0.0 as f32]
            SDL_RenderGetScale(this.handle as *SDL_Renderer, x, y)
            let s = new RenderScale()
            s.x = x[0]
            s.y = y[0]
            s
        }

        windowToLogical(wx: i32, wy: i32): FCoord {
            let lx: f32[] = [0.0 as f32]
            let ly: f32[] = [0.0 as f32]
            SDL_RenderWindowToLogical(
                this.handle as *SDL_Renderer, wx, wy, lx, ly
            )
            let p = new FCoord()
            p.x = lx[0]
            p.y = ly[0]
            p
        }

        logicalToWindow(lx: f32, ly: f32): RenderSize {
            let wx: i32[] = [0]
            let wy: i32[] = [0]
            SDL_RenderLogicalToWindow(
                this.handle as *SDL_Renderer, lx, ly, wx, wy
            )
            let p = new RenderSize()
            p.w = wx[0]
            p.h = wy[0]
            p
        }

        // ─── primitive draw (integer) ─────────────────────────────

        clear(): i32 {
            SDL_RenderClear(this.handle as *SDL_Renderer)
        }

        drawPoint(x: i32, y: i32): i32 {
            SDL_RenderDrawPoint(this.handle as *SDL_Renderer, x, y)
        }

        drawPoints(points: SDL_Point[]): i32 {
            SDL_RenderDrawPoints(
                this.handle as *SDL_Renderer,
                points, points.length as i32
            )
        }

        drawLine(x1: i32, y1: i32, x2: i32, y2: i32): i32 {
            SDL_RenderDrawLine(this.handle as *SDL_Renderer, x1, y1, x2, y2)
        }

        drawLines(points: SDL_Point[]): i32 {
            SDL_RenderDrawLines(
                this.handle as *SDL_Renderer,
                points, points.length as i32
            )
        }

        drawRect(x: i32, y: i32, w: i32, h: i32): i32 {
            let rect = new sdl.SDL_Rect()
            rect.x = x
            rect.y = y
            rect.w = w
            rect.h = h
            SDL_RenderDrawRect(this.handle as *SDL_Renderer, rect)
        }

        drawRects(rects: sdl.SDL_Rect[]): i32 {
            SDL_RenderDrawRects(
                this.handle as *SDL_Renderer,
                rects, rects.length as i32
            )
        }

        // Fill an axis-aligned rectangle. Coordinates are in window
        // pixels; `(x, y)` is the top-left corner.
        fillRect(x: i32, y: i32, w: i32, h: i32): i32 {
            let rect = new sdl.SDL_Rect()
            rect.x = x
            rect.y = y
            rect.w = w
            rect.h = h
            SDL_RenderFillRect(this.handle as *SDL_Renderer, rect)
        }

        fillRects(rects: sdl.SDL_Rect[]): i32 {
            SDL_RenderFillRects(
                this.handle as *SDL_Renderer,
                rects, rects.length as i32
            )
        }

        // ─── primitive draw (float) ───────────────────────────────

        drawPointF(x: f32, y: f32): i32 {
            SDL_RenderDrawPointF(this.handle as *SDL_Renderer, x, y)
        }

        drawPointsF(points: SDL_FPoint[]): i32 {
            SDL_RenderDrawPointsF(
                this.handle as *SDL_Renderer,
                points, points.length as i32
            )
        }

        drawLineF(x1: f32, y1: f32, x2: f32, y2: f32): i32 {
            SDL_RenderDrawLineF(this.handle as *SDL_Renderer, x1, y1, x2, y2)
        }

        drawLinesF(points: SDL_FPoint[]): i32 {
            SDL_RenderDrawLinesF(
                this.handle as *SDL_Renderer,
                points, points.length as i32
            )
        }

        drawRectF(x: f32, y: f32, w: f32, h: f32): i32 {
            let rect = new SDL_FRect()
            rect.x = x
            rect.y = y
            rect.w = w
            rect.h = h
            SDL_RenderDrawRectF(this.handle as *SDL_Renderer, rect)
        }

        drawRectsF(rects: SDL_FRect[]): i32 {
            SDL_RenderDrawRectsF(
                this.handle as *SDL_Renderer,
                rects, rects.length as i32
            )
        }

        fillRectF(x: f32, y: f32, w: f32, h: f32): i32 {
            let rect = new SDL_FRect()
            rect.x = x
            rect.y = y
            rect.w = w
            rect.h = h
            SDL_RenderFillRectF(this.handle as *SDL_Renderer, rect)
        }

        fillRectsF(rects: SDL_FRect[]): i32 {
            SDL_RenderFillRectsF(
                this.handle as *SDL_Renderer,
                rects, rects.length as i32
            )
        }

        // ─── texture blit ────────────────────────────────────────

        // Stretch the entire texture into the destination rect.
        drawTexture(tex: Texture, x: i32, y: i32, w: i32, h: i32): i32 {
            let dst = new sdl.SDL_Rect()
            dst.x = x
            dst.y = y
            dst.w = w
            dst.h = h
            SDL_RenderCopy(
                this.handle as *SDL_Renderer,
                tex.handle as *SDL_Texture,
                0 as *sdl.SDL_Rect, dst
            )
        }

        // Same as drawTexture but rotates around the dst rect's
        // centre by `angle` degrees (clockwise).
        drawTextureRot(
            tex: Texture, x: i32, y: i32, w: i32, h: i32, angle: f64
        ): i32 {
            let dst = new sdl.SDL_Rect()
            dst.x = x
            dst.y = y
            dst.w = w
            dst.h = h
            SDL_RenderCopyEx(
                this.handle as *SDL_Renderer,
                tex.handle as *SDL_Texture,
                0 as *sdl.SDL_Rect, dst,
                angle, 0 as *const SDL_Point, 0
            )
        }

        // Verbose blit. `srcrect == 0` (`null`) draws the whole
        // source; same for `dstrect`.
        copy(
            tex: Texture,
            srcrect: sdl.SDL_Rect, dstrect: sdl.SDL_Rect
        ): i32 {
            SDL_RenderCopy(
                this.handle as *SDL_Renderer,
                tex.handle as *SDL_Texture,
                srcrect, dstrect
            )
        }

        copyEx(
            tex: Texture,
            srcrect: sdl.SDL_Rect, dstrect: sdl.SDL_Rect,
            angle: f64, center: SDL_Point, flip: i32
        ): i32 {
            SDL_RenderCopyEx(
                this.handle as *SDL_Renderer,
                tex.handle as *SDL_Texture,
                srcrect, dstrect, angle, center, flip
            )
        }

        copyF(
            tex: Texture,
            srcrect: sdl.SDL_Rect, dstrect: SDL_FRect
        ): i32 {
            SDL_RenderCopyF(
                this.handle as *SDL_Renderer,
                tex.handle as *SDL_Texture,
                srcrect, dstrect
            )
        }

        copyExF(
            tex: Texture,
            srcrect: sdl.SDL_Rect, dstrect: SDL_FRect,
            angle: f64, center: SDL_FPoint, flip: i32
        ): i32 {
            SDL_RenderCopyExF(
                this.handle as *SDL_Renderer,
                tex.handle as *SDL_Texture,
                srcrect, dstrect, angle, center, flip
            )
        }

        // ─── geometry ────────────────────────────────────────────

        /// Triangle list. `tex` may be a TARGET-less `Texture` to
        /// blend its pixels into the geometry, or pass a Texture
        /// whose handle is 0 for plain colour-only rendering.
        geometry(
            tex: Texture, vertices: SDL_Vertex[], indices: i32[]
        ): i32 {
            let tex_h = if tex.handle != 0 {
                tex.handle as *SDL_Texture
            } else {
                0 as *SDL_Texture
            }
            SDL_RenderGeometry(
                this.handle as *SDL_Renderer, tex_h,
                vertices, vertices.length as i32,
                indices, indices.length as i32
            )
        }

        /// Read back the framebuffer into an existing pixel buffer
        /// using the supplied SDL_PixelFormat. Buffer size must be
        /// at least `pitch * rect.h` bytes.
        readPixels(
            rect: sdl.SDL_Rect, format: u32, pixels: u8[], pitch: i32
        ): i32 {
            let p: *u8 = pixels
            SDL_RenderReadPixels(
                this.handle as *SDL_Renderer, rect, format,
                p as *void, pitch
            )
        }

        // Swap the back-buffer to the screen (vsync if the driver does it).
        present() {
            SDL_RenderPresent(this.handle as *SDL_Renderer)
        }

        flush(): i32 {
            SDL_RenderFlush(this.handle as *SDL_Renderer)
        }

        targetSupported(): bool {
            SDL_RenderTargetSupported(this.handle as *SDL_Renderer) != 0
        }

        // Redirect subsequent draw calls into a texture. Pass a
        // TARGET-access texture, then `resetTarget()` to swap back.
        setTarget(tex: Texture): i32 {
            SDL_SetRenderTarget(this.handle as *SDL_Renderer,
                                tex.handle as *SDL_Texture)
        }

        resetTarget(): i32 {
            SDL_SetRenderTarget(
                this.handle as *SDL_Renderer, 0 as *SDL_Texture
            )
        }

        setVSync(vsync: i32): i32 {
            SDL_RenderSetVSync(this.handle as *SDL_Renderer, vsync)
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

        // `access` selects the SDL_TextureAccess:
        //   1 = STREAMING — CPU-uploaded pixels via `update`
        //   2 = TARGET    — renderer draws into it (`Renderer.setTarget`)
        // SDL_PIXELFORMAT_ABGR8888 = 0x16762004 (R, G, B, A on
        // little-endian).
        init(renderer: Renderer, w: i32, h: i32, access: i32) {
            this.width = w
            this.height = h
            this.handle = SDL_CreateTexture(
                renderer.handle as *SDL_Renderer,
                0x16762004 as u32, access,
                w, h
            ) as i64
            if this.handle != 0 {
                // SDL_BLENDMODE_BLEND = 1.
                SDL_SetTextureBlendMode(this.handle as *SDL_Texture, 1)
            }
        }

        // Like the basic init but lets the caller pick the
        // SDL_PixelFormatEnum.
        init(
            renderer: Renderer, format: u32, access: i32, w: i32, h: i32
        ) {
            this.width = w
            this.height = h
            this.handle = SDL_CreateTexture(
                renderer.handle as *SDL_Renderer, format, access, w, h
            ) as i64
            if this.handle != 0 {
                SDL_SetTextureBlendMode(this.handle as *SDL_Texture, 1)
            }
        }

        deinit() {
            if this.handle != 0 {
                SDL_DestroyTexture(this.handle as *SDL_Texture)
            }
        }

        /// SDL_QueryTexture in one shot — format / access / w / h.
        query(): TextureInfo {
            let fmt: u32[] = [0 as u32]
            let access: i32[] = [0]
            let w: i32[] = [0]
            let h: i32[] = [0]
            SDL_QueryTexture(
                this.handle as *SDL_Texture, fmt, access, w, h
            )
            let i = new TextureInfo()
            i.format = fmt[0]
            i.access = access[0]
            i.w = w[0]
            i.h = h[0]
            i
        }

        // Upload pixel data. The buffer must be exactly
        // `width * height * 4` bytes (RGBA8 per pixel).
        update(rgba: u8[]): i32 {
            // pitch = bytes per row. For full-texture updates the
            // rect arg is NULL.
            let pitch = this.width * 4
            let p: *u8 = rgba
            SDL_UpdateTexture(
                this.handle as *SDL_Texture, 0 as *sdl.SDL_Rect,
                p as *const void, pitch
            )
        }

        /// Update a sub-rect of the texture.
        updateRect(rect: sdl.SDL_Rect, rgba: u8[], pitch: i32): i32 {
            let p: *u8 = rgba
            SDL_UpdateTexture(
                this.handle as *SDL_Texture, rect, p as *const void, pitch
            )
        }

        // Optional per-draw tinting / fading.
        setColorMod(r: u8, g: u8, b: u8): i32 {
            SDL_SetTextureColorMod(this.handle as *SDL_Texture, r, g, b)
        }

        getColorMod(): SDL_Color {
            let c = new SDL_Color()
            let r: u8[] = [0]
            let g: u8[] = [0]
            let b: u8[] = [0]
            SDL_GetTextureColorMod(this.handle as *SDL_Texture, r, g, b)
            c.r = r[0]
            c.g = g[0]
            c.b = b[0]
            c.a = 255 as u8
            c
        }

        setAlphaMod(a: u8): i32 {
            SDL_SetTextureAlphaMod(this.handle as *SDL_Texture, a)
        }

        getAlphaMod(): u8 {
            let a: u8[] = [0]
            SDL_GetTextureAlphaMod(this.handle as *SDL_Texture, a)
            a[0]
        }

        // SDL_BlendMode: 0=NONE, 1=BLEND, 2=ADD, 4=MOD, 8=MUL.
        setBlendMode(mode: i32): i32 {
            SDL_SetTextureBlendMode(this.handle as *SDL_Texture, mode)
        }

        getBlendMode(): i32 {
            let m: i32[] = [0]
            SDL_GetTextureBlendMode(this.handle as *SDL_Texture, m)
            m[0]
        }

        // SDL_ScaleMode: 0=Nearest, 1=Linear, 2=Best.
        setScaleMode(mode: i32): i32 {
            SDL_SetTextureScaleMode(this.handle as *SDL_Texture, mode)
        }

        getScaleMode(): i32 {
            let m: i32[] = [0]
            SDL_GetTextureScaleMode(this.handle as *SDL_Texture, m)
            m[0]
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
                    this.atlas as *SDL_Texture, 0 as *sdl.SDL_Rect,
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
                    let src = new sdl.SDL_Rect()
                    src.x = idx * this.cell_w
                    src.y = 0
                    src.w = this.cell_w
                    src.h = this.cell_h
                    let dst = new sdl.SDL_Rect()
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
