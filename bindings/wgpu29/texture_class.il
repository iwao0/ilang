/// Friendly `Texture` / `TextureView` / `TextureOptions`
/// wrappers pulled out of `texture.il`.

use core { WGPUStringView, TextureFormat }
use texture {
    WGPUTexture, WGPUTextureView,
    TextureDimension, TextureUsage,
    wgpuTextureCreateView,
    wgpuTextureRelease, wgpuTextureDestroy, wgpuTextureSetLabel,
    wgpuTextureGetWidth, wgpuTextureGetHeight,
    wgpuTextureGetDepthOrArrayLayers, wgpuTextureGetMipLevelCount,
    wgpuTextureGetSampleCount, wgpuTextureGetDimension,
    wgpuTextureGetFormat, wgpuTextureGetUsage,
    wgpuTextureViewRelease, wgpuTextureViewSetLabel,
}
use std.ffi { cstrFromString }

@extern(C, "wgpu_native") {
/// Wraps `WGPUTextureView`. Built via `Texture.createView`.
pub class TextureView {
    pub handle: i64

    pub init(handle: WGPUTextureView) {
        this.handle = handle as i64
    }
    pub release() {
        if this.handle != 0 {
            wgpuTextureViewRelease(this.handle as WGPUTextureView)
            this.handle = 0
        }
    }
    pub setLabel(label: string) {
        wgpuTextureViewSetLabel(this.handle as WGPUTextureView, WGPUStringView {
            data:   cstrFromString(label) as *void,
            length: 0xFFFFFFFFFFFFFFFF as u64,
        })
    }
}

/// Wraps `WGPUTexture`. Built via `Device.createTexture`.
/// `release()` drops the host-side refcount; `destroy()`
/// drops the GPU-side storage eagerly.
pub class Texture {
    pub handle: i64

    pub init(handle: WGPUTexture) {
        this.handle = handle as i64
    }
    pub release() {
        if this.handle != 0 {
            wgpuTextureRelease(this.handle as WGPUTexture)
            this.handle = 0
        }
    }
    pub destroy() {
        if this.handle != 0 {
            wgpuTextureDestroy(this.handle as WGPUTexture)
        }
    }
    pub setLabel(label: string) {
        wgpuTextureSetLabel(this.handle as WGPUTexture, WGPUStringView {
            data:   cstrFromString(label) as *void,
            length: 0xFFFFFFFFFFFFFFFF as u64,
        })
    }
    /// Default auto-derived view (same format / single mip /
    /// all aspects). Pass a `*WGPUTextureViewDescriptor` to
    /// `wgpuTextureCreateView` directly for custom views.
    pub createView(): TextureView {
        new TextureView(wgpuTextureCreateView(this.handle as WGPUTexture, 0 as *void))
    }
    pub getWidth(): i64               { wgpuTextureGetWidth(this.handle as WGPUTexture) as i64 }
    pub getHeight(): i64              { wgpuTextureGetHeight(this.handle as WGPUTexture) as i64 }
    pub getDepthOrArrayLayers(): i64  { wgpuTextureGetDepthOrArrayLayers(this.handle as WGPUTexture) as i64 }
    pub getMipLevelCount(): i64       { wgpuTextureGetMipLevelCount(this.handle as WGPUTexture) as i64 }
    pub getSampleCount(): i64         { wgpuTextureGetSampleCount(this.handle as WGPUTexture) as i64 }
    pub getDimension(): TextureDimension {
        wgpuTextureGetDimension(this.handle as WGPUTexture) as TextureDimension
    }
    pub getFormat(): TextureFormat {
        wgpuTextureGetFormat(this.handle as WGPUTexture) as TextureFormat
    }
    pub getUsage(): TextureUsage {
        wgpuTextureGetUsage(this.handle as WGPUTexture) as TextureUsage
    }
}

/// Configuration for `Device.createTexture`. Defaults to a
/// single-mip / single-sample 2D texture; reassign fields
/// before passing in.
pub class TextureOptions {
    pub label:              string
    pub usage:              TextureUsage
    pub dimension:          TextureDimension
    pub width:              i64
    pub height:             i64
    pub depthOrArrayLayers: i64
    pub format:             TextureFormat
    pub mipLevelCount:      i64
    pub sampleCount:        i64

    pub init(width: i64, height: i64, format: TextureFormat, usage: TextureUsage) {
        this.label              = ""
        this.usage              = usage
        this.dimension          = TextureDimension._2D
        this.width              = width
        this.height             = height
        this.depthOrArrayLayers = 1
        this.format             = format
        this.mipLevelCount      = 1
        this.sampleCount        = 1
    }
}
}
