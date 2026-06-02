/// `WGPUTexture` + `WGPUTextureView` + the `TexelCopy*` structs
/// every CPU↔GPU image transfer routes through. `bytesPerRow`
/// in `WGPUTexelCopyBufferLayout` MUST be a multiple of 256
/// (WebGPU spec) — pad the row stride before issuing a copy.
///
/// WGPUTextureDimension: _1D=1, _2D=2, _3D=3.
/// WGPUTextureViewDimension: Undefined=0, _1D=1, _2D=2,
///                           _2DArray=3, Cube=4, CubeArray=5,
///                           _3D=6.
/// WGPUTextureAspect: All=0, StencilOnly=1, DepthOnly=2.
/// Texture usage flags (u64): CopySrc=0x01, CopyDst=0x02,
///   TextureBinding=0x04, StorageBinding=0x08,
///   RenderAttachment=0x10.

use core { WGPUStringView, WGPUExtent3D, WGPUOrigin3D, WGPUDevice }
use buffer { WGPUBuffer }

pub enum TextureAspect: u32 {
    undefined   = 0x00
    all         = 0x01
    stencilOnly = 0x02
    depthOnly   = 0x03
}

pub enum TextureDimension: u32 {
    undefined = 0x00
    _1D       = 0x01
    _2D       = 0x02
    _3D       = 0x03
}

pub enum TextureViewDimension: u32 {
    undefined = 0x00
    _1D       = 0x01
    _2D       = 0x02
    _2DArray  = 0x03
    cube      = 0x04
    cubeArray = 0x05
    _3D       = 0x06
}

@flags
pub enum TextureUsage: u64 {
    none                = 0x00
    copySrc             = 0x01
    copyDst             = 0x02
    textureBinding      = 0x04
    storageBinding      = 0x08
    renderAttachment    = 0x10
    transientAttachment = 0x20
}

@extern(C, "wgpu_native") {
    @handle pub struct WGPUTexture {}
    @handle pub struct WGPUTextureView {}

    pub struct WGPUTextureDescriptor {
        nextInChain:     *void
        label:           WGPUStringView
        usage:           TextureUsage
        dimension:       TextureDimension
        size:            WGPUExtent3D
        format:          TextureFormat
        mipLevelCount:   u32
        sampleCount:     u32
        viewFormatCount: u64
        viewFormats:     *void
    }
    @lib pub fn wgpuDeviceCreateTexture(
        device: WGPUDevice, descriptor: *WGPUTextureDescriptor
    ): WGPUTexture

    pub struct WGPUTextureViewDescriptor {
        nextInChain:     *void
        label:           WGPUStringView
        format:          TextureFormat
        dimension:       TextureViewDimension
        baseMipLevel:    u32
        mipLevelCount:   u32
        baseArrayLayer:  u32
        arrayLayerCount: u32
        aspect:          TextureAspect
        usage:           TextureUsage
    }
    // `descriptor` may be `0 as *void` for the auto-derived
    // view (same format / single mip / all aspects). Cast a
    // non-null descriptor with `(&desc) as *void`.
    @lib pub fn wgpuTextureCreateView(
        texture: WGPUTexture, descriptor: *void
    ): WGPUTextureView

    // ─── texel copy plumbing ──────────────────────────────────
    //
    // Shared between `wgpuQueueWriteTexture` and every
    // CommandEncoder Copy{*}↔Texture op. Layout struct describes
    // CPU-side row stride; Info struct picks a (texture, mip,
    // origin, aspect) sub-region.
    pub struct WGPUTexelCopyBufferLayout {
        offset:       u64
        bytesPerRow:  u32
        rowsPerImage: u32
    }
    pub struct WGPUTexelCopyTextureInfo {
        texture:  WGPUTexture
        mipLevel: u32
        origin:   WGPUOrigin3D
        aspect:   TextureAspect
    }
    pub struct WGPUTexelCopyBufferInfo {
        layout: WGPUTexelCopyBufferLayout
        buffer: WGPUBuffer
    }

    // ─── introspection ────────────────────────────────────────
    @lib pub fn wgpuTextureGetWidth(texture: WGPUTexture): u32
    @lib pub fn wgpuTextureGetHeight(texture: WGPUTexture): u32
    @lib pub fn wgpuTextureGetDepthOrArrayLayers(texture: WGPUTexture): u32
    @lib pub fn wgpuTextureGetMipLevelCount(texture: WGPUTexture): u32
    @lib pub fn wgpuTextureGetSampleCount(texture: WGPUTexture): u32
    @lib pub fn wgpuTextureGetDimension(texture: WGPUTexture): u32     // WGPUTextureDimension
    @lib pub fn wgpuTextureGetFormat(texture: WGPUTexture): u32        // WGPUTextureFormat
    @lib pub fn wgpuTextureGetUsage(texture: WGPUTexture): u64         // WGPUTextureUsage flags
    // Defaulting rule that depends on the texture's dimension
    // + array layer count, so it's a method (not just `Get`).
    @lib pub fn wgpuTextureGetTextureBindingViewDimension(texture: WGPUTexture): u32
    // wgpu-native macOS extension. Returns the underlying Metal
    // `id<MTLTexture>`; null on non-Metal backends.
    @lib pub fn wgpuTextureGetNativeMetalTexture(texture: WGPUTexture): *void

    // Drop the GPU-side storage eagerly (same caveat as
    // BufferDestroy — any in-flight command referencing the
    // texture fails validation).
    @lib pub fn wgpuTextureDestroy(texture: WGPUTexture)
    @lib pub fn wgpuTextureSetLabel(texture: WGPUTexture, label: WGPUStringView)
    @lib pub fn wgpuTextureAddRef(texture: WGPUTexture)
    @lib pub fn wgpuTextureRelease(texture: WGPUTexture)

    @lib pub fn wgpuTextureViewSetLabel(view: WGPUTextureView, label: WGPUStringView)
    @lib pub fn wgpuTextureViewAddRef(view: WGPUTextureView)
    @lib pub fn wgpuTextureViewRelease(view: WGPUTextureView)
}

// `Texture`, `TextureView`, `TextureOptions` wrapper classes live
// in `texture_class.il`.
