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

use sdl_core { Rect, Point, FPoint, Size, Color }
use sdl_surface { BlendMode }
use sdl_window { Window }

// ─── enums (mirror the SDL_render.h typedefs) ─────────────────────

/// `SDL_RendererFlags` — passed to `SDL_CreateRenderer` / new
/// `Renderer`.
@flags
pub enum RendererFlags: u32 {
    software      = 0x00000001
    accelerated   = 0x00000002
    presentVsync  = 0x00000004
    targetTexture = 0x00000008
}

/// `SDL_ScaleMode` — texture sampling filter.
pub enum ScaleMode: i32 {
    nearest = 0
    linear  = 1
    best    = 2
}

/// `SDL_TextureAccess`.
pub enum TextureAccess: i32 {
    static    = 0
    streaming = 1
    target    = 2
}

/// `SDL_TextureModulate` — multiplied against texture colour /
/// alpha on draw.
@flags
pub enum TextureModulate: i32 {
    none  = 0x00000000
    color = 0x00000001
    alpha = 0x00000002
}

/// `SDL_RendererFlip` — bit flags for `RenderCopyEx`.
/// `SDL_BlendOperation` — how source / destination factors get
/// combined in `composeCustomBlendMode`.
pub enum BlendOperation: i32 {
    add         = 0x1   // dst + src
    subtract    = 0x2   // dst - src
    revSubtract = 0x3   // src - dst
    minimum     = 0x4   // min(dst, src)
    maximum     = 0x5   // max(dst, src)
}

/// `SDL_BlendFactor` — the per-channel factor each side of the
/// blend equation gets multiplied by.
pub enum BlendFactor: i32 {
    zero             = 0x1
    one              = 0x2
    srcColor         = 0x3
    oneMinusSrcColor = 0x4
    srcAlpha         = 0x5
    oneMinusSrcAlpha = 0x6
    dstColor         = 0x7
    oneMinusDstColor = 0x8
    dstAlpha         = 0x9
    oneMinusDstAlpha = 0xA
}

@flags
pub enum RendererFlip: i32 {
    none       = 0x00000000
    horizontal = 0x00000001
    vertical   = 0x00000002
}

@extern(C, "SDL2") {
    /// Opaque renderer handle.
    pub struct SDL_Renderer {}
    /// Opaque texture handle.
    pub struct SDL_Texture {}

    /// Float-coordinate rectangle. Mirrors C `SDL_FRect`.
    pub struct FRect {
        x: f32
        y: f32
        w: f32
        h: f32
    }
    // (`Color` lives in `sdl_core.il` — imported above so it
    // resolves where `Vertex` / `setRenderDrawColor` etc. need it.)
    /// Position + colour + UV, for `RenderGeometry`. Mirrors C
    /// `SDL_Vertex`.
    pub struct Vertex {
        position: FPoint
        color: Color
        texCoord: FPoint
    }
    /// `SDL_RendererInfo` — driver / capability data.
    pub struct SDL_RendererInfo {
        name: *const char
        flags: u32
        numTextureFormats: u32
        textureFormats: u32[16]
        maxTextureWidth: i32
        maxTextureHeight: i32
    }

    // ─── driver / renderer lifecycle ──────────────────────────────
    @lib pub fn SDL_GetNumRenderDrivers(): i32
    @lib pub fn SDL_GetRenderDriverInfo(
        index: i32, info: *SDL_RendererInfo
    ): i32
    @lib pub fn SDL_CreateWindowAndRenderer(
        width: i32, height: i32, window_flags: u32,
        window: **void, renderer: **SDL_Renderer
    ): i32
    @lib pub fn SDL_CreateRenderer(
        window: *void, index: i32, flags: u32
    ): *SDL_Renderer
    @lib pub fn SDL_CreateSoftwareRenderer(surface: *void): *SDL_Renderer
    @lib pub fn SDL_GetRenderer(window: *void): *SDL_Renderer
    @lib pub fn SDL_RenderGetWindow(renderer: *SDL_Renderer): *void
    @lib pub fn SDL_GetRendererInfo(
        renderer: *SDL_Renderer, info: *SDL_RendererInfo
    ): i32
    @lib pub fn SDL_GetRendererOutputSize(
        renderer: *SDL_Renderer, w: *i32, h: *i32
    ): i32
    @lib pub fn SDL_DestroyRenderer(renderer: *SDL_Renderer)
    @lib pub fn SDL_RenderFlush(renderer: *SDL_Renderer): i32

    // ─── texture lifecycle / metadata ─────────────────────────────
    @lib pub fn SDL_CreateTexture(
        renderer: *SDL_Renderer,
        format: u32, access: i32,
        w: i32, h: i32
    ): *SDL_Texture
    @lib pub fn SDL_CreateTextureFromSurface(
        renderer: *SDL_Renderer, surface: *void
    ): *SDL_Texture
    @lib pub fn SDL_QueryTexture(
        tex: *SDL_Texture,
        format: *u32, access: *i32, w: *i32, h: *i32
    ): i32
    @lib pub fn SDL_DestroyTexture(tex: *SDL_Texture)

    @lib pub fn SDL_SetTextureColorMod(
        tex: *SDL_Texture, r: u8, g: u8, b: u8
    ): i32
    @lib pub fn SDL_GetTextureColorMod(
        tex: *SDL_Texture, r: *u8, g: *u8, b: *u8
    ): i32
    @lib pub fn SDL_SetTextureAlphaMod(
        tex: *SDL_Texture, alpha: u8
    ): i32
    @lib pub fn SDL_GetTextureAlphaMod(
        tex: *SDL_Texture, alpha: *u8
    ): i32
    @lib pub fn SDL_SetTextureBlendMode(
        tex: *SDL_Texture, blendMode: i32
    ): i32
    @lib pub fn SDL_GetTextureBlendMode(
        tex: *SDL_Texture, blendMode: *i32
    ): i32
    @lib pub fn SDL_SetTextureScaleMode(
        tex: *SDL_Texture, scaleMode: i32
    ): i32
    @lib pub fn SDL_GetTextureScaleMode(
        tex: *SDL_Texture, scaleMode: *i32
    ): i32
    @lib pub fn SDL_SetTextureUserData(
        tex: *SDL_Texture, userdata: *void
    ): i32
    @lib pub fn SDL_GetTextureUserData(tex: *SDL_Texture): *void

    @lib pub fn SDL_UpdateTexture(
        tex: *SDL_Texture, rect: *Rect,
        pixels: *const void, pitch: i32
    ): i32
    @lib pub fn SDL_UpdateYUVTexture(
        tex: *SDL_Texture, rect: *Rect,
        Yplane: *const u8, Ypitch: i32,
        Uplane: *const u8, Upitch: i32,
        Vplane: *const u8, Vpitch: i32
    ): i32
    @lib pub fn SDL_UpdateNVTexture(
        tex: *SDL_Texture, rect: *Rect,
        Yplane: *const u8, Ypitch: i32,
        UVplane: *const u8, UVpitch: i32
    ): i32
    @lib pub fn SDL_LockTexture(
        tex: *SDL_Texture, rect: *Rect,
        pixels: **void, pitch: *i32
    ): i32
    @lib pub fn SDL_LockTextureToSurface(
        tex: *SDL_Texture, rect: *Rect, surface: **void
    ): i32
    @lib pub fn SDL_UnlockTexture(tex: *SDL_Texture)

    // ─── render target / logical size / viewport / clip / scale ──
    @lib pub fn SDL_RenderTargetSupported(
        renderer: *SDL_Renderer
    ): i32
    @lib pub fn SDL_SetRenderTarget(
        renderer: *SDL_Renderer, tex: *SDL_Texture
    ): i32
    @lib pub fn SDL_GetRenderTarget(
        renderer: *SDL_Renderer
    ): *SDL_Texture

    @lib pub fn SDL_RenderSetLogicalSize(
        renderer: *SDL_Renderer, w: i32, h: i32
    ): i32
    @lib pub fn SDL_RenderGetLogicalSize(
        renderer: *SDL_Renderer, w: *i32, h: *i32
    )
    @lib pub fn SDL_RenderSetIntegerScale(
        renderer: *SDL_Renderer, enable: i32
    ): i32
    @lib pub fn SDL_RenderGetIntegerScale(
        renderer: *SDL_Renderer
    ): i32
    @lib pub fn SDL_RenderSetViewport(
        renderer: *SDL_Renderer, rect: *Rect
    ): i32
    @lib pub fn SDL_RenderGetViewport(
        renderer: *SDL_Renderer, rect: *Rect
    )
    @lib pub fn SDL_RenderSetClipRect(
        renderer: *SDL_Renderer, rect: *Rect
    ): i32
    @lib pub fn SDL_RenderGetClipRect(
        renderer: *SDL_Renderer, rect: *Rect
    )
    @lib pub fn SDL_RenderIsClipEnabled(
        renderer: *SDL_Renderer
    ): i32
    @lib pub fn SDL_RenderSetScale(
        renderer: *SDL_Renderer, scaleX: f32, scaleY: f32
    ): i32
    @lib pub fn SDL_RenderGetScale(
        renderer: *SDL_Renderer, scaleX: *f32, scaleY: *f32
    )
    @lib pub fn SDL_RenderWindowToLogical(
        renderer: *SDL_Renderer,
        windowX: i32, windowY: i32,
        logicalX: *f32, logicalY: *f32
    )
    @lib pub fn SDL_RenderLogicalToWindow(
        renderer: *SDL_Renderer,
        logicalX: f32, logicalY: f32,
        windowX: *i32, windowY: *i32
    )

    // ─── draw colour / blend mode ─────────────────────────────────
    @lib pub fn SDL_SetRenderDrawColor(
        renderer: *SDL_Renderer, r: u8, g: u8, b: u8, a: u8
    ): i32
    @lib pub fn SDL_GetRenderDrawColor(
        renderer: *SDL_Renderer, r: *u8, g: *u8, b: *u8, a: *u8
    ): i32
    @lib pub fn SDL_SetRenderDrawBlendMode(
        renderer: *SDL_Renderer, blendMode: i32
    ): i32
    /// `SDL_ComposeCustomBlendMode` — assemble a custom blend mode
    /// out of factor / operation knobs. The returned i32 fits in
    /// the same `SDL_BlendMode` slot used by `setBlendMode` /
    /// `setSurfaceBlendMode`. Use this when the four canned modes
    /// (none / blend / add / mod / mul) don't match what you want.
    @lib pub fn SDL_ComposeCustomBlendMode(
        srcColorFactor: i32, dstColorFactor: i32, colorOperation: i32,
        srcAlphaFactor: i32, dstAlphaFactor: i32, alphaOperation: i32
    ): i32

    @lib pub fn SDL_GetRenderDrawBlendMode(
        renderer: *SDL_Renderer, blendMode: *i32
    ): i32

    // ─── primitive draw / fill (integer coords) ───────────────────
    @lib pub fn SDL_RenderClear(renderer: *SDL_Renderer): i32
    @lib pub fn SDL_RenderDrawPoint(
        renderer: *SDL_Renderer, x: i32, y: i32
    ): i32
    @lib pub fn SDL_RenderDrawPoints(
        renderer: *SDL_Renderer, points: *const Point, count: i32
    ): i32
    @lib pub fn SDL_RenderDrawLine(
        renderer: *SDL_Renderer, x1: i32, y1: i32, x2: i32, y2: i32
    ): i32
    @lib pub fn SDL_RenderDrawLines(
        renderer: *SDL_Renderer, points: *const Point, count: i32
    ): i32
    @lib pub fn SDL_RenderDrawRect(
        renderer: *SDL_Renderer, rect: *Rect
    ): i32
    @lib pub fn SDL_RenderDrawRects(
        renderer: *SDL_Renderer, rects: *const Rect, count: i32
    ): i32
    @lib pub fn SDL_RenderFillRect(
        renderer: *SDL_Renderer, rect: *Rect
    ): i32
    @lib pub fn SDL_RenderFillRects(
        renderer: *SDL_Renderer, rects: *const Rect, count: i32
    ): i32
    @lib pub fn SDL_RenderCopy(
        renderer: *SDL_Renderer, tex: *SDL_Texture,
        srcrect: *Rect, dstrect: *Rect
    ): i32
    @lib pub fn SDL_RenderCopyEx(
        renderer: *SDL_Renderer, tex: *SDL_Texture,
        srcrect: *Rect, dstrect: *Rect,
        angle: f64, center: *const Point, flip: i32
    ): i32

    // ─── primitive draw / fill (float coords) ─────────────────────
    @lib pub fn SDL_RenderDrawPointF(
        renderer: *SDL_Renderer, x: f32, y: f32
    ): i32
    @lib pub fn SDL_RenderDrawPointsF(
        renderer: *SDL_Renderer, points: *const FPoint, count: i32
    ): i32
    @lib pub fn SDL_RenderDrawLineF(
        renderer: *SDL_Renderer, x1: f32, y1: f32, x2: f32, y2: f32
    ): i32
    @lib pub fn SDL_RenderDrawLinesF(
        renderer: *SDL_Renderer, points: *const FPoint, count: i32
    ): i32
    @lib pub fn SDL_RenderDrawRectF(
        renderer: *SDL_Renderer, rect: *const FRect
    ): i32
    @lib pub fn SDL_RenderDrawRectsF(
        renderer: *SDL_Renderer, rects: *const FRect, count: i32
    ): i32
    @lib pub fn SDL_RenderFillRectF(
        renderer: *SDL_Renderer, rect: *const FRect
    ): i32
    @lib pub fn SDL_RenderFillRectsF(
        renderer: *SDL_Renderer, rects: *const FRect, count: i32
    ): i32
    @lib pub fn SDL_RenderCopyF(
        renderer: *SDL_Renderer, tex: *SDL_Texture,
        srcrect: *Rect, dstrect: *const FRect
    ): i32
    @lib pub fn SDL_RenderCopyExF(
        renderer: *SDL_Renderer, tex: *SDL_Texture,
        srcrect: *Rect, dstrect: *const FRect,
        angle: f64, center: *const FPoint, flip: i32
    ): i32

    // ─── geometry / read-back / present ───────────────────────────
    @lib pub fn SDL_RenderGeometry(
        renderer: *SDL_Renderer, tex: *SDL_Texture,
        vertices: *const Vertex, num_vertices: i32,
        indices: *const i32, num_indices: i32
    ): i32
    @lib pub fn SDL_RenderGeometryRaw(
        renderer: *SDL_Renderer, tex: *SDL_Texture,
        xy: *const f32, xy_stride: i32,
        color: *const Color, color_stride: i32,
        uv: *const f32, uv_stride: i32,
        num_vertices: i32,
        indices: *const void, num_indices: i32, size_indices: i32
    ): i32
    @lib pub fn SDL_RenderReadPixels(
        renderer: *SDL_Renderer, rect: *Rect,
        format: u32, pixels: *void, pitch: i32
    ): i32
    @lib pub fn SDL_RenderPresent(renderer: *SDL_Renderer)

    // ─── platform / GL / Metal / VSync ───────────────────────────
    @lib pub fn SDL_GL_BindTexture(
        tex: *SDL_Texture, texw: *f32, texh: *f32
    ): i32
    @lib pub fn SDL_GL_UnbindTexture(tex: *SDL_Texture): i32
    @lib pub fn SDL_RenderGetMetalLayer(
        renderer: *SDL_Renderer
    ): *void
    @lib pub fn SDL_RenderGetMetalCommandEncoder(
        renderer: *SDL_Renderer
    ): *void
    @lib pub fn SDL_RenderSetVSync(
        renderer: *SDL_Renderer, vsync: i32
    ): i32

    // ─── public ilang return types ────────────────────────────────

    /// Public ilang return type for `Renderer.driverInfo` /
    /// `Renderer.info`. Mirrors `SDL_RendererInfo` but converts
    /// the raw `*const char` name into an owned `string` and the
    /// fixed-length texture format slot into a `u32[]` so callers
    /// never see a raw pointer.
    pub class RendererInfo {
        pub name: string
        pub flags: u32
        pub textureFormats: u32[]
        pub maxTextureWidth: i32
        pub maxTextureHeight: i32
    }

    /// Bundle for `Texture.query` (mirrors `SDL_QueryTexture`'s
    /// four out parameters).
    pub class TextureInfo {
        pub format: u32
        pub access: i32
        pub w: i32
        pub h: i32
    }

    /// ARC-managed wrapper around `SDL_Renderer`. `deinit` calls
    /// `SDL_DestroyRenderer` automatically.
    pub class Renderer {
        /// Pointer to the underlying `SDL_Renderer` cast to `i64`.
        /// `0` when creation failed.
        pub handle: i64

        /// Build with the SDL defaults: first available driver,
        /// accelerated path chosen by SDL, BLEND mode pre-enabled.
        pub init(window: Window) {
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
        pub init(window: Window, index: i32, flags: u32) {
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
        pub get info(): RendererInfo {
            let raw = new SDL_RendererInfo()
            SDL_GetRendererInfo(this.handle as *SDL_Renderer, &raw)
            let info = new RendererInfo()
            info.name = stringFromCstr(raw.name)
            info.flags = raw.flags
            info.maxTextureWidth = raw.maxTextureWidth
            info.maxTextureHeight = raw.maxTextureHeight
            let n: i64 = raw.numTextureFormats as i64
            let fmts: u32[] = []
            let i: i64 = 0
            while i < n {
                fmts.push(raw.textureFormats[i])
                i += 1
            }
            info.textureFormats = fmts
            info
        }

        /// Drawable output size in pixels (may differ from window
        /// size on HiDPI / logical-size setups).
        pub get outputSize(): Size {
            let s = new Size()
            SDL_GetRendererOutputSize(this.handle as *SDL_Renderer, &s.w, &s.h)
            s
        }

        // ─── colour / blend mode ──────────────────────────────────

        /// Set the draw colour used by `clear` / line / rect calls.
        ///
        /// **Args:**
        /// - `r`, `g`, `b`, `a` — 0..255 channels (a = 255 is opaque).
        ///
        /// **Returns:**
        ///   `0` on success, `< 0` on failure.
        pub setColor(r: u8, g: u8, b: u8, a: u8): i32 {
            SDL_SetRenderDrawColor(this.handle as *SDL_Renderer, r, g, b, a)
        }

        /// The current draw colour as a `Color`. Read-only — use
        /// `setColor(r, g, b, a)` (which keeps the multi-arg
        /// signature for ergonomic literal calls) to write.
        pub get color(): Color {
            let c = new Color()
            SDL_GetRenderDrawColor(
                this.handle as *SDL_Renderer, &c.r, &c.g, &c.b, &c.a
            )
            c
        }

        /// Active blend mode for draw calls. The setter discards
        /// the error code SDL would return; call
        /// `SDL_SetRenderDrawBlendMode` directly if you need to
        /// react to a failed mode change.
        pub get blendMode(): BlendMode {
            let m: i32 = 0
            SDL_GetRenderDrawBlendMode(this.handle as *SDL_Renderer, &m)
            m as BlendMode
        }
        pub set blendMode(mode: BlendMode) {
            SDL_SetRenderDrawBlendMode(this.handle as *SDL_Renderer, mode as i32)
            return
        }

        // ─── logical / viewport / clip / scale ────────────────────

        /// Pin the renderer's coordinate system to a virtual `w × h`
        /// grid. SDL stretches the result to fit the actual window
        /// (with letterboxing when the aspect differs).
        ///
        /// **Args:**
        /// - `w`, `h` — virtual width / height.
        ///
        /// **Returns:**
        ///   `0` on success, `< 0` on failure.
        pub setLogicalSize(w: i32, h: i32): i32 {
            SDL_RenderSetLogicalSize(this.handle as *SDL_Renderer, w, h)
        }

        /// The current logical size, or `(0, 0)` if not set.
        /// Read-only — `setLogicalSize(w, h)` keeps the multi-arg
        /// signature for the writer.
        pub get logicalSize(): Size {
            let s = new Size()
            SDL_RenderGetLogicalSize(this.handle as *SDL_Renderer, &s.w, &s.h)
            s
        }

        /// Snap stretched logical coords to whole pixels.
        pub get isIntegerScale(): bool {
            SDL_RenderGetIntegerScale(this.handle as *SDL_Renderer) != 0
        }
        pub set isIntegerScale(enable: bool) {
            let v: i32 = if enable { 1 } else { 0 }
            SDL_RenderSetIntegerScale(this.handle as *SDL_Renderer, v)
            return
        }

        /// Active drawing viewport (window-relative pixels). Set
        /// to a fresh `Rect` to confine drawing; call
        /// `clearViewport()` to remove the constraint.
        pub get viewport(): Rect {
            let r = new Rect()
            SDL_RenderGetViewport(this.handle as *SDL_Renderer, &r)
            r
        }
        pub set viewport(rect: Rect) {
            SDL_RenderSetViewport(this.handle as *SDL_Renderer, &rect)
            return
        }

        /// Drop any viewport constraint set via `viewport = ...`.
        pub clearViewport(): i32 {
            SDL_RenderSetViewport(
                this.handle as *SDL_Renderer, 0 as *Rect
            )
        }

        /// Active clip rectangle (drawing outside is suppressed).
        pub get clipRect(): Rect {
            let r = new Rect()
            SDL_RenderGetClipRect(this.handle as *SDL_Renderer, &r)
            r
        }
        pub set clipRect(rect: Rect) {
            SDL_RenderSetClipRect(this.handle as *SDL_Renderer, &rect)
            return
        }

        /// Drop any clip rectangle set via `clipRect = ...`.
        pub clearClipRect(): i32 {
            SDL_RenderSetClipRect(
                this.handle as *SDL_Renderer, 0 as *Rect
            )
        }

        pub get isClipEnabled(): bool {
            SDL_RenderIsClipEnabled(this.handle as *SDL_Renderer) != 0
        }

        /// Render scale factor as an `FPoint(x, y)`. The setter
        /// keeps the explicit two-arg form (`setScale(x, y)`)
        /// since callers usually pass scalar literals.
        pub setScale(x: f32, y: f32): i32 {
            SDL_RenderSetScale(this.handle as *SDL_Renderer, x, y)
        }

        pub get scale(): FPoint {
            let s = new FPoint()
            SDL_RenderGetScale(this.handle as *SDL_Renderer, &s.x, &s.y)
            s
        }

        pub windowToLogical(wx: i32, wy: i32): FPoint {
            let p = new FPoint()
            SDL_RenderWindowToLogical(
                this.handle as *SDL_Renderer, wx, wy, &p.x, &p.y
            )
            p
        }

        pub logicalToWindow(lx: f32, ly: f32): Point {
            let p = new Point()
            SDL_RenderLogicalToWindow(
                this.handle as *SDL_Renderer, lx, ly, &p.x, &p.y
            )
            p
        }

        // ─── primitive draw (integer) ─────────────────────────────

        /// Fill the entire render target with the current draw
        /// colour. Typically called once per frame before drawing.
        ///
        /// **Returns:**
        ///   `0` on success, `< 0` on failure.
        pub clear(): i32 {
            SDL_RenderClear(this.handle as *SDL_Renderer)
        }

        /// Draw a single pixel at `(x, y)`.
        pub drawPoint(x: i32, y: i32): i32 {
            SDL_RenderDrawPoint(this.handle as *SDL_Renderer, x, y)
        }

        pub drawPoints(points: Point[]): i32 {
            SDL_RenderDrawPoints(
                this.handle as *SDL_Renderer,
                points, points.length as i32
            )
        }

        pub drawLine(x1: i32, y1: i32, x2: i32, y2: i32): i32 {
            SDL_RenderDrawLine(this.handle as *SDL_Renderer, x1, y1, x2, y2)
        }

        pub drawLines(points: Point[]): i32 {
            SDL_RenderDrawLines(
                this.handle as *SDL_Renderer,
                points, points.length as i32
            )
        }

        pub drawRect(x: i32, y: i32, w: i32, h: i32): i32 {
            let rect = new Rect()
            rect.x = x
            rect.y = y
            rect.w = w
            rect.h = h
            SDL_RenderDrawRect(this.handle as *SDL_Renderer, &rect)
        }

        pub drawRects(rects: Rect[]): i32 {
            SDL_RenderDrawRects(
                this.handle as *SDL_Renderer,
                rects, rects.length as i32
            )
        }

        /// Fill an axis-aligned rectangle. Coordinates are in
        /// window pixels (or logical pixels when `setLogicalSize`
        /// is in effect); `(x, y)` is the top-left corner.
        ///
        /// **Args:**
        /// - `x`, `y` — top-left corner.
        /// - `w`, `h` — rectangle size.
        ///
        /// **Returns:**
        ///   `0` on success, `< 0` on failure.
        pub fillRect(x: i32, y: i32, w: i32, h: i32): i32 {
            let rect = new Rect()
            rect.x = x
            rect.y = y
            rect.w = w
            rect.h = h
            SDL_RenderFillRect(this.handle as *SDL_Renderer, &rect)
        }

        pub fillRects(rects: Rect[]): i32 {
            SDL_RenderFillRects(
                this.handle as *SDL_Renderer,
                rects, rects.length as i32
            )
        }

        // ─── primitive draw (float) ───────────────────────────────

        pub drawPointF(x: f32, y: f32): i32 {
            SDL_RenderDrawPointF(this.handle as *SDL_Renderer, x, y)
        }

        pub drawPointsF(points: FPoint[]): i32 {
            SDL_RenderDrawPointsF(
                this.handle as *SDL_Renderer,
                points, points.length as i32
            )
        }

        pub drawLineF(x1: f32, y1: f32, x2: f32, y2: f32): i32 {
            SDL_RenderDrawLineF(this.handle as *SDL_Renderer, x1, y1, x2, y2)
        }

        pub drawLinesF(points: FPoint[]): i32 {
            SDL_RenderDrawLinesF(
                this.handle as *SDL_Renderer,
                points, points.length as i32
            )
        }

        pub drawRectF(x: f32, y: f32, w: f32, h: f32): i32 {
            let rect = new FRect()
            rect.x = x
            rect.y = y
            rect.w = w
            rect.h = h
            SDL_RenderDrawRectF(this.handle as *SDL_Renderer, &rect)
        }

        pub drawRectsF(rects: FRect[]): i32 {
            SDL_RenderDrawRectsF(
                this.handle as *SDL_Renderer,
                rects, rects.length as i32
            )
        }

        pub fillRectF(x: f32, y: f32, w: f32, h: f32): i32 {
            let rect = new FRect()
            rect.x = x
            rect.y = y
            rect.w = w
            rect.h = h
            SDL_RenderFillRectF(this.handle as *SDL_Renderer, &rect)
        }

        pub fillRectsF(rects: FRect[]): i32 {
            SDL_RenderFillRectsF(
                this.handle as *SDL_Renderer,
                rects, rects.length as i32
            )
        }

        // ─── texture blit ────────────────────────────────────────

        /// Stretch the entire texture into the destination rect.
        ///
        /// **Args:**
        /// - `tex` — source texture.
        /// - `x`, `y` — destination top-left.
        /// - `w`, `h` — destination size (texture is scaled to fit).
        ///
        /// **Returns:**
        ///   `0` on success, `< 0` on failure.
        pub drawTexture(tex: Texture, x: i32, y: i32, w: i32, h: i32): i32 {
            let dst = new Rect()
            dst.x = x
            dst.y = y
            dst.w = w
            dst.h = h
            SDL_RenderCopy(
                this.handle as *SDL_Renderer,
                tex.handle as *SDL_Texture,
                0 as *Rect, &dst
            )
        }

        /// Same as `drawTexture` but rotates around the dst rect's
        /// centre.
        ///
        /// **Args:**
        /// - `tex` — source texture.
        /// - `x`, `y` — destination top-left.
        /// - `w`, `h` — destination size.
        /// - `angle` — rotation in degrees, clockwise.
        ///
        /// **Returns:**
        ///   `0` on success, `< 0` on failure.
        pub drawTextureRot(
            tex: Texture, x: i32, y: i32, w: i32, h: i32, angle: f64
        ): i32 {
            let dst = new Rect()
            dst.x = x
            dst.y = y
            dst.w = w
            dst.h = h
            SDL_RenderCopyEx(
                this.handle as *SDL_Renderer,
                tex.handle as *SDL_Texture,
                0 as *Rect, &dst,
                angle, 0 as *const Point, 0
            )
        }

        /// Verbose blit. `srcrect == 0` (`null`) draws the whole
        /// source; same for `dstrect`.
        ///
        /// **Args:**
        /// - `tex` — source texture.
        /// - `srcrect` — sub-rect within `tex` (or `0` for whole).
        /// - `dstrect` — destination rect on the render target (or
        ///   `0` to fill the target).
        ///
        /// **Returns:**
        ///   `0` on success, `< 0` on failure.
        pub copy(
            tex: Texture,
            srcrect: Rect, dstrect: Rect
        ): i32 {
            SDL_RenderCopy(
                this.handle as *SDL_Renderer,
                tex.handle as *SDL_Texture,
                &srcrect, &dstrect
            )
        }

        pub copyEx(
            tex: Texture,
            srcrect: Rect, dstrect: Rect,
            angle: f64, center: Point, flip: i32
        ): i32 {
            SDL_RenderCopyEx(
                this.handle as *SDL_Renderer,
                tex.handle as *SDL_Texture,
                &srcrect, &dstrect, angle, &center, flip
            )
        }

        pub copyF(
            tex: Texture,
            srcrect: Rect, dstrect: FRect
        ): i32 {
            SDL_RenderCopyF(
                this.handle as *SDL_Renderer,
                tex.handle as *SDL_Texture,
                &srcrect, &dstrect
            )
        }

        pub copyExF(
            tex: Texture,
            srcrect: Rect, dstrect: FRect,
            angle: f64, center: FPoint, flip: i32
        ): i32 {
            SDL_RenderCopyExF(
                this.handle as *SDL_Renderer,
                tex.handle as *SDL_Texture,
                &srcrect, &dstrect, angle, &center, flip
            )
        }

        // ─── geometry ────────────────────────────────────────────

        /// Triangle list. `tex` may be a TARGET-less `Texture` to
        /// blend its pixels into the geometry, or pass a Texture
        /// whose handle is 0 for plain colour-only rendering.
        pub geometry(
            tex: Texture, vertices: Vertex[], indices: i32[]
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
        pub readPixels(
            rect: Rect, format: u32, pixels: u8[], pitch: i32
        ): i32 {
            let p: *u8 = pixels
            SDL_RenderReadPixels(
                this.handle as *SDL_Renderer, &rect, format,
                p as *void, pitch
            )
        }

        /// Swap the back-buffer to the screen (vsync if the driver
        /// has it enabled).
        pub present() {
            SDL_RenderPresent(this.handle as *SDL_Renderer)
        }

        /// Force any pending draw calls to be sent to the GPU /
        /// platform layer immediately.
        pub flush(): i32 {
            SDL_RenderFlush(this.handle as *SDL_Renderer)
        }

        /// **Returns:**
        ///   true when the driver supports rendering into TARGET
        ///   textures.
        pub targetSupported(): bool {
            SDL_RenderTargetSupported(this.handle as *SDL_Renderer) != 0
        }

        /// Redirect subsequent draw calls into a texture. Pass a
        /// TARGET-access `Texture`, then `resetTarget()` to swap
        /// back to the window.
        ///
        /// **Returns:**
        ///   `0` on success, `< 0` on failure.
        pub setTarget(tex: Texture): i32 {
            SDL_SetRenderTarget(this.handle as *SDL_Renderer,
                                tex.handle as *SDL_Texture)
        }

        /// Restore drawing to the window (after `setTarget`).
        pub resetTarget(): i32 {
            SDL_SetRenderTarget(
                this.handle as *SDL_Renderer, 0 as *SDL_Texture
            )
        }

        /// **Args:**
        /// - `vsync` — `1` to enable vsync, `0` to disable.
        pub setVSync(vsync: i32): i32 {
            SDL_RenderSetVSync(this.handle as *SDL_Renderer, vsync)
        }

        // ─── static helpers ──────────────────────────────────────

        /// Number of available 2D render drivers.
        pub static numDrivers(): i32 {
            SDL_GetNumRenderDrivers()
        }

        /// Driver info at `index` (`0..numDrivers()`).
        pub static driverInfo(index: i32): RendererInfo {
            let raw = new SDL_RendererInfo()
            SDL_GetRenderDriverInfo(index, &raw)
            let info = new RendererInfo()
            info.name = stringFromCstr(raw.name)
            info.flags = raw.flags
            info.maxTextureWidth = raw.maxTextureWidth
            info.maxTextureHeight = raw.maxTextureHeight
            let n: i64 = raw.numTextureFormats as i64
            let fmts: u32[] = []
            let i: i64 = 0
            while i < n {
                fmts.push(raw.textureFormats[i])
                i += 1
            }
            info.textureFormats = fmts
            info
        }

        /// Build a custom `SDL_BlendMode` from explicit blend
        /// factors and operations. The returned value plugs into
        /// `Renderer.setBlendMode` / `Texture.setBlendMode` /
        /// `Surface.setBlendMode` exactly the same way as the
        /// canned modes (`0` = none, `1` = blend, `2` = add,
        /// `4` = mod, `8` = mul).
        ///
        /// **Args:**
        /// - `srcColor` — factor for the incoming RGB.
        /// - `dstColor` — factor for the existing-in-target RGB.
        /// - `colorOp` — combine operation for RGB.
        /// - `srcAlpha` — factor for the incoming alpha.
        /// - `dstAlpha` — factor for the existing-in-target alpha.
        /// - `alphaOp` — combine operation for alpha.
        ///
        /// **Returns:**
        ///   the resulting `BlendMode` (a 32-bit opaque token —
        ///   feed it straight into `Renderer.blendMode = ...` /
        ///   `Texture.blendMode = ...`), or `BlendMode.invalid`
        ///   if the platform can't synthesise the requested
        ///   combination.
        pub static composeCustomBlendMode(
            srcColor: BlendFactor, dstColor: BlendFactor,
            colorOp: BlendOperation,
            srcAlpha: BlendFactor, dstAlpha: BlendFactor,
            alphaOp: BlendOperation
        ): BlendMode {
            SDL_ComposeCustomBlendMode(
                srcColor as i32, dstColor as i32, colorOp as i32,
                srcAlpha as i32, dstAlpha as i32, alphaOp as i32
            ) as BlendMode
        }
    }

    /// ARC-managed sprite. Holds an `SDL_Texture` sized at
    /// construction and lets you upload raw RGBA8 pixels
    /// (`r, g, b, a` byte order on little-endian —
    /// `SDL_PIXELFORMAT_ABGR8888 = 0x16762004`). Use
    /// `Renderer.drawTexture` / `drawTextureRot` to draw it.
    pub class Texture {
        /// Pointer to `SDL_Texture` cast to `i64`. `0` when
        /// creation failed.
        pub handle: i64
        pub width: i32
        pub height: i32

        /// RGBA8888 texture with the requested access mode.
        ///
        /// **Args:**
        /// - `renderer` — the parent `Renderer`.
        /// - `w`, `h` — texture dimensions in pixels.
        /// - `access` — `TextureAccess.streaming` for CPU-uploaded
        ///   pixels via `update`; `TextureAccess.target`
        ///   for a draw target (`Renderer.setTarget`);
        ///   `TextureAccess.static` for a one-shot
        ///   upload that won't change after creation.
        ///
        /// Pixel format is `SDL_PIXELFORMAT_ABGR8888` (= `0x16762004`,
        /// `R, G, B, A` on little-endian).
        pub init(renderer: Renderer, w: i32, h: i32, access: TextureAccess) {
            this.width = w
            this.height = h
            this.handle = SDL_CreateTexture(
                renderer.handle as *SDL_Renderer,
                0x16762004 as u32, access as i32,
                w, h
            ) as i64
            if this.handle != 0 {
                // SDL_BLENDMODE_BLEND = 1.
                SDL_SetTextureBlendMode(this.handle as *SDL_Texture, 1)
            }
        }

        /// Like the basic `init` but lets the caller pick the
        /// `SDL_PixelFormatEnum`.
        ///
        /// **Args:**
        /// - `renderer` — the parent `Renderer`.
        /// - `format` — `SDL_PIXELFORMAT_*` value.
        /// - `access` — `TextureAccess.streaming` / `target` /
        ///   `static`.
        /// - `w`, `h` — texture dimensions.
        pub init(
            renderer: Renderer, format: u32, access: TextureAccess,
            w: i32, h: i32
        ) {
            this.width = w
            this.height = h
            this.handle = SDL_CreateTexture(
                renderer.handle as *SDL_Renderer, format, access as i32, w, h
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

        /// Internal "wrap an existing handle" init. Distinguished
        /// from the public ones by its (i64, i32, i32) signature
        /// — none of the public inits have a non-Object first
        /// argument. Used only by `Texture.fromHandle(...)`.
        pub init(handle: i64, w: i32, h: i32) {
            this.handle = handle
            this.width = w
            this.height = h
        }

        /// Wrap a raw `*SDL_Texture` (cast to i64) returned by
        /// another module (e.g. `sdl_image.imgLoadTexture`,
        /// `sdl_ttf` rendering). Takes ownership: `deinit` will
        /// call `SDL_DestroyTexture`. Width / height are queried
        /// from SDL since the caller doesn't always know them.
        /// Pass `0` to construct a "dead" Texture (every method
        /// short-circuits on `handle == 0`).
        pub static fromHandle(handle: i64): Texture {
            if handle == 0 {
                return new Texture(0 as i64, 0 as i32, 0 as i32)
            }
            let fmt: u32 = 0 as u32
            let access: i32 = 0
            let w: i32 = 0
            let h: i32 = 0
            SDL_QueryTexture(handle as *SDL_Texture, &fmt, &access, &w, &h)
            new Texture(handle, w, h)
        }

        /// SDL_QueryTexture in one shot — format / access / w / h.
        pub get info(): TextureInfo {
            let i = new TextureInfo()
            SDL_QueryTexture(
                this.handle as *SDL_Texture,
                &i.format, &i.access, &i.w, &i.h
            )
            i
        }

        /// Upload pixel data into a STREAMING texture.
        ///
        /// **Args:**
        /// - `rgba` — RGBA8 buffer, exactly `width * height * 4`
        ///   bytes (4 bytes per pixel, row-major).
        ///
        /// **Returns:**
        ///   `0` on success, `< 0` on failure.
        pub update(rgba: u8[]): i32 {
            // pitch = bytes per row. For full-texture updates the
            // rect arg is NULL.
            let pitch = this.width * 4
            let p: *u8 = rgba
            SDL_UpdateTexture(
                this.handle as *SDL_Texture, 0 as *Rect,
                p as *const void, pitch
            )
        }

        /// Update a sub-rect of the texture.
        pub updateRect(rect: Rect, rgba: u8[], pitch: i32): i32 {
            let p: *u8 = rgba
            SDL_UpdateTexture(
                this.handle as *SDL_Texture, &rect, p as *const void, pitch
            )
        }

        /// Per-draw RGB tint multiplied against the texture pixels.
        ///
        /// **Args:**
        /// - `r`, `g`, `b` — `255` is "no tint" on each channel.
        pub setColorMod(r: u8, g: u8, b: u8): i32 {
            SDL_SetTextureColorMod(this.handle as *SDL_Texture, r, g, b)
        }

        /// The current colour-mod (alpha reads as `255`). The
        /// setter keeps the three-arg method form so call sites
        /// can pass scalar literals without constructing a
        /// `Color` first.
        pub get colorMod(): Color {
            let c = new Color()
            c.a = 255 as u8
            SDL_GetTextureColorMod(this.handle as *SDL_Texture, &c.r, &c.g, &c.b)
            c
        }

        /// Per-draw alpha multiplier (fade-out). `255` = opaque.
        /// The setter discards the SDL error code.
        pub get alphaMod(): u8 {
            let a: u8 = 0
            SDL_GetTextureAlphaMod(this.handle as *SDL_Texture, &a)
            a
        }
        pub set alphaMod(a: u8) {
            SDL_SetTextureAlphaMod(this.handle as *SDL_Texture, a)
            return
        }

        /// Per-texture blend mode applied when the renderer
        /// composites this texture. Setter discards the error
        /// code.
        pub get blendMode(): BlendMode {
            let m: i32 = 0
            SDL_GetTextureBlendMode(this.handle as *SDL_Texture, &m)
            m as BlendMode
        }
        pub set blendMode(mode: BlendMode) {
            SDL_SetTextureBlendMode(this.handle as *SDL_Texture, mode as i32)
            return
        }

        /// Sampling filter SDL applies when scaling this texture
        /// at draw time. Setter discards the error code.
        pub get scaleMode(): ScaleMode {
            let m: i32 = 0
            SDL_GetTextureScaleMode(this.handle as *SDL_Texture, &m)
            m as ScaleMode
        }
        pub set scaleMode(mode: ScaleMode) {
            SDL_SetTextureScaleMode(this.handle as *SDL_Texture, mode as i32)
            return
        }
    }

}

