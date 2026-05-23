/// opengl32.dll — OpenGL 1.1 base + WGL (Windows-GL bridge).
///
/// Only GL 1.1 core functions are exported directly from opengl32.dll;
/// anything newer (GL 2.0+, ARB/EXT extensions) must be obtained at
/// runtime via `wglGetProcAddress` after a context is current.
///
/// Typical setup:
///   1. CreateWindowExA → HWND
///   2. GetDC(hwnd) → HDC
///   3. ChoosePixelFormat → SetPixelFormat on the HDC
///   4. wglCreateContext(hdc) → HGLRC
///   5. wglMakeCurrent(hdc, hglrc)
///   6. draw, SwapBuffers(hdc) [from gdi32]
///   7. wglMakeCurrent(NULL, NULL); wglDeleteContext(hglrc); ReleaseDC

// ─── PIXELFORMATDESCRIPTOR dwFlags ───────────────────────────────
@flags
pub enum PfdFlags: u32 {
    doublebuffer   = 0x00000001
    stereo         = 0x00000002
    drawToWindow   = 0x00000004
    drawToBitmap   = 0x00000008
    supportGdi     = 0x00000010
    supportOpenGL  = 0x00000020
    genericFormat  = 0x00000040
    needPalette    = 0x00000080
    needSystemPalette = 0x00000100
    swapExchange   = 0x00000200
    swapCopy       = 0x00000400
    swapLayerBuffers = 0x00000800
    genericAccelerated = 0x00001000
}

// ─── PIXELFORMATDESCRIPTOR iPixelType ────────────────────────────
pub enum PfdPixelType: u8 {
    rgba       = 0
    colorIndex = 1
}

// ─── PIXELFORMATDESCRIPTOR iLayerType ────────────────────────────
pub enum PfdLayerType: u8 {
    mainPlane    = 0
    overlayPlane = 1
    underlayPlane = 255   // -1 as u8
}

// ─── GL primitive types (GL_*) ───────────────────────────────────
pub enum GlPrimitive: u32 {
    points         = 0x0000
    lines          = 0x0001
    lineLoop       = 0x0002
    lineStrip      = 0x0003
    triangles      = 0x0004
    triangleStrip  = 0x0005
    triangleFan    = 0x0006
    quads          = 0x0007
    quadStrip      = 0x0008
    polygon        = 0x0009
}

// ─── GL state caps (passed to glEnable / glDisable) ──────────────
pub enum GlCap: u32 {
    blend          = 0x0BE2
    cullFace       = 0x0B44
    depthTest      = 0x0B71
    stencilTest    = 0x0B90
    scissorTest    = 0x0C11
    texture2d      = 0x0DE1
    lighting       = 0x0B50
    light0         = 0x4000
    colorMaterial  = 0x0B57
    normalize      = 0x0BA1
    alphaTest      = 0x0BC0
    fog            = 0x0B60
    dither         = 0x0BD0
    multisample    = 0x809D
    lineSmooth     = 0x0B20
    polygonSmooth  = 0x0B41
}

// ─── glClear bits ────────────────────────────────────────────────
@flags
pub enum GlClearBit: u32 {
    color    = 0x00004000
    depth    = 0x00000100
    stencil  = 0x00000400
    accum    = 0x00000200
}

// ─── glGetString name ────────────────────────────────────────────
pub enum GlString: u32 {
    vendor    = 0x1F00
    renderer  = 0x1F01
    version   = 0x1F02
    extensions = 0x1F03
}

// ─── glBlendFunc factor ──────────────────────────────────────────
pub enum GlBlend: u32 {
    zero               = 0
    one                = 1
    srcColor           = 0x0300
    oneMinusSrcColor   = 0x0301
    srcAlpha           = 0x0302
    oneMinusSrcAlpha   = 0x0303
    dstAlpha           = 0x0304
    oneMinusDstAlpha   = 0x0305
    dstColor           = 0x0306
    oneMinusDstColor   = 0x0307
    srcAlphaSaturate   = 0x0308
}

use windef { HDC }
@extern(C, "opengl32") {
    // ── PIXELFORMATDESCRIPTOR — 40 bytes ─────────────────────────

    pub struct PIXELFORMATDESCRIPTOR {
        nSize:           u16     // = 40
        nVersion:        u16     // = 1
        dwFlags:         u32
        iPixelType:      u8
        cColorBits:      u8
        cRedBits:        u8
        cRedShift:       u8
        cGreenBits:      u8
        cGreenShift:     u8
        cBlueBits:       u8
        cBlueShift:      u8
        cAlphaBits:      u8
        cAlphaShift:     u8
        cAccumBits:      u8
        cAccumRedBits:   u8
        cAccumGreenBits: u8
        cAccumBlueBits:  u8
        cAccumAlphaBits: u8
        cDepthBits:      u8
        cStencilBits:    u8
        cAuxBuffers:     u8
        iLayerType:      u8
        bReserved:       u8
        dwLayerMask:     u32
        dwVisibleMask:   u32
        dwDamageMask:    u32
    }

    /// Opaque GL rendering context.
    @handle pub struct HGLRC {}

    // ── WGL — context management ─────────────────────────────────

    /// Returns the index of a pixel format on `hdc` matching the
    /// requested PIXELFORMATDESCRIPTOR fields, or 0 on failure.
    @lib pub fn ChoosePixelFormat(hdc: HDC, ppfd: *const PIXELFORMATDESCRIPTOR): i32

    /// Sets the pixel format. Each HDC can have its format set only once.
    @lib pub fn SetPixelFormat(
        hdc:    HDC,
        format: i32,
        ppfd:   *const PIXELFORMATDESCRIPTOR
    ): i32

    /// Retrieves the current pixel format index for `hdc`.
    @lib pub fn GetPixelFormat(hdc: HDC): i32

    /// Describes a pixel format. Returns the maximum format index.
    @lib pub fn DescribePixelFormat(
        hdc:    HDC,
        format: i32,
        nBytes: u32,
        ppfd:   *PIXELFORMATDESCRIPTOR
    ): i32

    /// Creates a GL 1.x rendering context for `hdc`.
    /// For GL 3.0+ use the WGL_ARB_create_context extension via
    /// wglGetProcAddress.
    @lib pub fn wglCreateContext(hdc: HDC): HGLRC

    @lib pub fn wglDeleteContext(hglrc: HGLRC): i32

    /// Makes `hglrc` the active context on the calling thread.
    /// Pass NULL for both to detach.
    @lib pub fn wglMakeCurrent(hdc: HDC, hglrc: HGLRC): i32

    @lib pub fn wglGetCurrentContext(): HGLRC
    @lib pub fn wglGetCurrentDC(): *void

    /// Returns the function pointer for an extension entry point.
    /// Cast the result to the appropriate `fn(...)` type.
    @lib pub fn wglGetProcAddress(lpszProc: *const char): *void

    /// Allows two GL contexts to share display lists, textures, FBOs, etc.
    @lib pub fn wglShareLists(hglrc1: HGLRC, hglrc2: HGLRC): i32

    // ── GL 1.1 core (selected) ───────────────────────────────────

    @lib pub fn glClearColor(red: f32, green: f32, blue: f32, alpha: f32)
    @lib pub fn glClear(mask: u32)
    @lib pub fn glClearDepth(depth: f64)
    @lib pub fn glClearStencil(s: i32)

    @lib pub fn glViewport(x: i32, y: i32, width: i32, height: i32)
    @lib pub fn glScissor(x: i32, y: i32, width: i32, height: i32)

    @lib pub fn glEnable(cap: u32)
    @lib pub fn glDisable(cap: u32)
    @lib pub fn glIsEnabled(cap: u32): u8

    @lib pub fn glBlendFunc(sfactor: u32, dfactor: u32)
    @lib pub fn glDepthFunc(func: u32)
    @lib pub fn glDepthMask(flag: u8)
    @lib pub fn glCullFace(mode: u32)
    @lib pub fn glFrontFace(mode: u32)
    @lib pub fn glPolygonMode(face: u32, mode: u32)

    @lib pub fn glBegin(mode: u32)
    @lib pub fn glEnd()

    @lib pub fn glVertex2f(x: f32, y: f32)
    @lib pub fn glVertex3f(x: f32, y: f32, z: f32)
    @lib pub fn glVertex3fv(v: *const f32)
    @lib pub fn glColor3f(red: f32, green: f32, blue: f32)
    @lib pub fn glColor4f(red: f32, green: f32, blue: f32, alpha: f32)
    @lib pub fn glColor3ub(red: u8, green: u8, blue: u8)
    @lib pub fn glNormal3f(nx: f32, ny: f32, nz: f32)
    @lib pub fn glTexCoord2f(s: f32, t: f32)

    // Matrix stack (legacy)
    @lib pub fn glMatrixMode(mode: u32)
    @lib pub fn glLoadIdentity()
    @lib pub fn glLoadMatrixf(m: *const f32)
    @lib pub fn glMultMatrixf(m: *const f32)
    @lib pub fn glPushMatrix()
    @lib pub fn glPopMatrix()
    @lib pub fn glTranslatef(x: f32, y: f32, z: f32)
    @lib pub fn glRotatef(angle: f32, x: f32, y: f32, z: f32)
    @lib pub fn glScalef(x: f32, y: f32, z: f32)
    @lib pub fn glOrtho(left: f64, right: f64, bottom: f64, top: f64, znear: f64, zfar: f64)
    @lib pub fn glFrustum(left: f64, right: f64, bottom: f64, top: f64, znear: f64, zfar: f64)

    // Textures (1.1 core)
    @lib pub fn glGenTextures(n: i32, textures: *u32)
    @lib pub fn glDeleteTextures(n: i32, textures: *const u32)
    @lib pub fn glBindTexture(target: u32, texture: u32)
    @lib pub fn glTexImage2D(
        target:        u32,
        level:         i32,
        internalformat: i32,
        width:         i32,
        height:        i32,
        border:        i32,
        format:        u32,
        type_:         u32,
        pixels:        *const void
    )
    @lib pub fn glTexSubImage2D(
        target: u32, level: i32, xoffset: i32, yoffset: i32,
        width: i32, height: i32, format: u32, type_: u32,
        pixels: *const void
    )
    @lib pub fn glTexParameteri(target: u32, pname: u32, param: i32)
    @lib pub fn glTexParameterf(target: u32, pname: u32, param: f32)
    @lib pub fn glPixelStorei(pname: u32, param: i32)

    // Buffer reads
    @lib pub fn glReadPixels(
        x: i32, y: i32, width: i32, height: i32,
        format: u32, type_: u32, data: *void
    )

    // Misc
    @lib pub fn glGetError(): u32
    @lib pub fn glFlush()
    @lib pub fn glFinish()
    @lib pub fn glHint(target: u32, mode: u32)

    /// Returns a static string (vendor, renderer, version, extensions).
    @lib pub fn glGetString(name: u32): *const char

    @lib pub fn glGetIntegerv(pname: u32, params: *i32)
    @lib pub fn glGetFloatv(pname: u32, params: *f32)
    @lib pub fn glGetBooleanv(pname: u32, params: *u8)
}
