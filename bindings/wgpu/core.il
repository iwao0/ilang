/// wgpu binding — shared primitives. No `wgpu_native` symbols
/// here; every per-feature file imports the value-struct types
/// (`WGPUStringView` / `WGPUChainedStruct` / `WGPUColor` /
/// `WGPUExtent3D` / `WGPUOrigin3D`) and the `TextureFormat`
/// enum from this file.
///
/// Pinned to wgpu-native v29.0.0.0 (webgpu.h). Strings are
/// `WGPUStringView { const char* data; size_t length }` (16 B,
/// passed in 2 regs); callback signatures splay the view into
/// two `i64`s.

/// `WGPUTextureFormat` — full v29 enumeration. Discriminants
/// match `webgpu.h`; cast to / from `u32` is identity.
/// `undefined` is the "no value passed" sentinel.
pub enum TextureFormat: u32 {
    undefined            = 0x00
    r8Unorm              = 0x01
    r8Snorm              = 0x02
    r8Uint               = 0x03
    r8Sint               = 0x04
    r16Unorm             = 0x05
    r16Snorm             = 0x06
    r16Uint              = 0x07
    r16Sint              = 0x08
    r16Float             = 0x09
    rg8Unorm             = 0x0A
    rg8Snorm             = 0x0B
    rg8Uint              = 0x0C
    rg8Sint              = 0x0D
    r32Float             = 0x0E
    r32Uint              = 0x0F
    r32Sint              = 0x10
    rg16Unorm            = 0x11
    rg16Snorm            = 0x12
    rg16Uint             = 0x13
    rg16Sint             = 0x14
    rg16Float            = 0x15
    rgba8Unorm           = 0x16
    rgba8UnormSrgb       = 0x17
    rgba8Snorm           = 0x18
    rgba8Uint            = 0x19
    rgba8Sint            = 0x1A
    bgra8Unorm           = 0x1B
    bgra8UnormSrgb       = 0x1C
    rgb10a2Uint          = 0x1D
    rgb10a2Unorm         = 0x1E
    rg11b10Ufloat        = 0x1F
    rgb9e5Ufloat         = 0x20
    rg32Float            = 0x21
    rg32Uint             = 0x22
    rg32Sint             = 0x23
    rgba16Unorm          = 0x24
    rgba16Snorm          = 0x25
    rgba16Uint           = 0x26
    rgba16Sint           = 0x27
    rgba16Float          = 0x28
    rgba32Float          = 0x29
    rgba32Uint           = 0x2A
    rgba32Sint           = 0x2B
    stencil8             = 0x2C
    depth16Unorm         = 0x2D
    depth24Plus          = 0x2E
    depth24PlusStencil8  = 0x2F
    depth32Float         = 0x30
    depth32FloatStencil8 = 0x31
    bc1RgbaUnorm         = 0x32
    bc1RgbaUnormSrgb     = 0x33
    bc2RgbaUnorm         = 0x34
    bc2RgbaUnormSrgb     = 0x35
    bc3RgbaUnorm         = 0x36
    bc3RgbaUnormSrgb     = 0x37
    bc4RUnorm            = 0x38
    bc4RSnorm            = 0x39
    bc5RgUnorm           = 0x3A
    bc5RgSnorm           = 0x3B
    bc6hRgbUfloat        = 0x3C
    bc6hRgbFloat         = 0x3D
    bc7RgbaUnorm         = 0x3E
    bc7RgbaUnormSrgb     = 0x3F
    etc2Rgb8Unorm        = 0x40
    etc2Rgb8UnormSrgb    = 0x41
    etc2Rgb8a1Unorm      = 0x42
    etc2Rgb8a1UnormSrgb  = 0x43
    etc2Rgba8Unorm       = 0x44
    etc2Rgba8UnormSrgb   = 0x45
    eacR11Unorm          = 0x46
    eacR11Snorm          = 0x47
    eacRg11Unorm         = 0x48
    eacRg11Snorm         = 0x49
    astc4x4Unorm         = 0x4A
    astc4x4UnormSrgb     = 0x4B
    astc5x4Unorm         = 0x4C
    astc5x4UnormSrgb     = 0x4D
    astc5x5Unorm         = 0x4E
    astc5x5UnormSrgb     = 0x4F
    astc6x5Unorm         = 0x50
    astc6x5UnormSrgb     = 0x51
    astc6x6Unorm         = 0x52
    astc6x6UnormSrgb     = 0x53
    astc8x5Unorm         = 0x54
    astc8x5UnormSrgb     = 0x55
    astc8x6Unorm         = 0x56
    astc8x6UnormSrgb     = 0x57
    astc8x8Unorm         = 0x58
    astc8x8UnormSrgb     = 0x59
    astc10x5Unorm        = 0x5A
    astc10x5UnormSrgb    = 0x5B
    astc10x6Unorm        = 0x5C
    astc10x6UnormSrgb    = 0x5D
    astc10x8Unorm        = 0x5E
    astc10x8UnormSrgb    = 0x5F
    astc10x10Unorm       = 0x60
    astc10x10UnormSrgb   = 0x61
    astc12x10Unorm       = 0x62
    astc12x10UnormSrgb   = 0x63
    astc12x12Unorm       = 0x64
    astc12x12UnormSrgb   = 0x65
}

@extern(C, "wgpu_native") {
    // ─── fundamental handles ──────────────────────────────────
    //
    // `WGPUDevice` and `WGPUQueue` live in core (not their own
    // `device.il`) so the per-resource files — `shader.il`,
    // `buffer.il`, `pipeline.il`, … — can declare
    // `wgpuDeviceCreate*(device: WGPUDevice, …)` without
    // importing `device.il`, which would otherwise cycle back
    // through the friendly `Device` class hosted there.
    @handle pub struct WGPUDevice {}
    @handle pub struct WGPUQueue {}

    /// `const char* data + size_t length`. Empty label = `{ 0, 0 }`;
    /// `length = WGPU_STRLEN` (= u64::MAX) means "wgpu strlen the
    /// data".
    pub struct WGPUStringView {
        data:   *void
        length: u64
    }
    /// Pointer to the previous-in-chain sType-tagged struct.
    /// Used as the `nextInChain` head for every descriptor that
    /// supports extension chains.
    pub struct WGPUChainedStruct {
        next:  *void
        sType: u32
    }
    /// Clear-colour / blend-constant RGBA in 0..1.
    pub struct WGPUColor {
        r: f64
        g: f64
        b: f64
        a: f64
    }
    /// 3D dimension triple used by texture / copy ops.
    pub struct WGPUExtent3D {
        width:              u32
        height:             u32
        depthOrArrayLayers: u32
    }
    /// 3D origin used by texture / copy ops.
    pub struct WGPUOrigin3D {
        x: u32
        y: u32
        z: u32
    }
}
