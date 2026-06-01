/// Friendly `Surface` / `SurfaceConfig` / `SurfaceFrame` /
/// `SurfaceCaps` wrappers pulled out of `surface.il`.

use core {
    WGPUStringView, WGPUDevice, WGPUAdapter,
}
use surface {
    WGPUSurface,
    WGPUSurfaceConfiguration, WGPUSurfaceTexture, WGPUSurfaceCapabilities,
    CompositeAlphaMode, PresentMode, SurfaceGetCurrentTextureStatus,
    wgpuSurfaceUnconfigure, wgpuSurfacePresent,
    wgpuSurfaceConfigure, wgpuSurfaceGetCurrentTexture,
    wgpuSurfaceGetCapabilities, wgpuSurfaceCapabilitiesFreeMembers,
}
use texture { WGPUTexture, TextureUsage }
use texture_class { Texture }
use device_class { Device }
use adapter_class { Adapter }
use std.ffi { readU32 }

@extern(C, "wgpu_native") {
/// Wraps `WGPUSurface`. Owned by the gui-side surface host
/// (`GpuView`); not released by this wrapper. User code
/// rarely touches it directly — the surface lifecycle
/// (configure / acquire / present) runs inside gui-core.
pub class Surface {
    pub handle: i64

    pub init(handle: WGPUSurface) {
        this.handle = handle as i64
    }

    /// Drop the configured swap chain without releasing the
    /// surface — useful before re-configuring with a new
    /// size / format.
    pub unconfigure() {
        wgpuSurfaceUnconfigure(this.handle as WGPUSurface)
    }
    pub present() {
        wgpuSurfacePresent(this.handle as WGPUSurface)
    }
    /// Configure the swap chain. Defaults assume a single
    /// view format and no extension chain — for the
    /// multi-format / extension cases call
    /// `wgpuSurfaceConfigure(s.handle as WGPUSurface, &cfg)`.
    pub configure(cfg: SurfaceConfig) {
        let raw = WGPUSurfaceConfiguration {
            nextInChain:     0 as *void,
            device:          cfg.device.handle as WGPUDevice,
            format:          cfg.format,
            usage:           cfg.usage,
            width:           cfg.width as u32,
            height:          cfg.height as u32,
            viewFormatCount: 0 as u64,
            viewFormats:     0 as *void,
            alphaMode:       cfg.alphaMode,
            presentMode:     cfg.presentMode,
        }
        wgpuSurfaceConfigure(this.handle as WGPUSurface, &raw)
    }
    /// Acquire the swap chain's current frame. The returned
    /// `SurfaceFrame` carries the texture and the acquire
    /// status — check status before using `texture`.
    pub getCurrentFrame(): SurfaceFrame {
        let st = WGPUSurfaceTexture {
            nextInChain: 0 as *void,
            texture:     0 as WGPUTexture,
            status:      SurfaceGetCurrentTextureStatus.successOptimal,
        }
        wgpuSurfaceGetCurrentTexture(this.handle as WGPUSurface, &st)
        new SurfaceFrame(new Texture(st.texture), st.status as u32)
    }
    /// Query the per-adapter capabilities (supported usage
    /// flags, formats, present / alpha modes). The
    /// wgpu-allocated arrays are copied into owned ilang
    /// arrays and freed via `wgpuSurfaceCapabilitiesFreeMembers`
    /// before return.
    pub getCapabilities(adapter: Adapter): SurfaceCaps {
        let caps = WGPUSurfaceCapabilities {
            nextInChain:      0 as *void,
            usages:           TextureUsage.none,
            formatCount:      0 as u64,
            formats:          0 as *void,
            presentModeCount: 0 as u64,
            presentModes:     0 as *void,
            alphaModeCount:   0 as u64,
            alphaModes:       0 as *void,
        }
        wgpuSurfaceGetCapabilities(
            this.handle as WGPUSurface,
            adapter.handle as WGPUAdapter,
            &caps
        )
        let out = new SurfaceCaps()
        out.usages = caps.usages
        let i: i64 = 0
        while i < (caps.formatCount as i64) {
            out.formats.push(readU32(caps.formats, i * 4) as TextureFormat)
            i = i + 1
        }
        i = 0
        while i < (caps.presentModeCount as i64) {
            out.presentModes.push(readU32(caps.presentModes, i * 4) as PresentMode)
            i = i + 1
        }
        i = 0
        while i < (caps.alphaModeCount as i64) {
            out.alphaModes.push(readU32(caps.alphaModes, i * 4) as CompositeAlphaMode)
            i = i + 1
        }
        wgpuSurfaceCapabilitiesFreeMembers(caps)
        out
    }
}

/// Configuration for `Surface.configure`.
pub class SurfaceConfig {
    pub device:      Device
    pub format:      TextureFormat
    pub usage:       TextureUsage
    pub width:       i64
    pub height:      i64
    pub alphaMode:   CompositeAlphaMode
    pub presentMode: PresentMode

    pub init(
        device: Device, format: TextureFormat, width: i64, height: i64
    ) {
        this.device      = device
        this.format      = format
        this.usage       = TextureUsage.renderAttachment
        this.width       = width
        this.height      = height
        this.alphaMode   = CompositeAlphaMode.auto
        this.presentMode = PresentMode.fifo
    }
}

/// Snapshot of `wgpuSurfaceGetCurrentTexture`. `status`
/// matches `WGPUSurfaceGetCurrentTextureStatus`: 1 = optimal,
/// 2 = suboptimal (still drawable), 3 = timeout, 4 = outdated
/// (reconfigure), 5 = lost, 6 = error. wgpu-native also uses
/// `0x00030001` for "occluded".
pub class SurfaceFrame {
    pub texture: Texture
    pub status:  u32

    pub init(texture: Texture, status: u32) {
        this.texture = texture
        this.status  = status
    }
}

/// Per-adapter capability snapshot. Built by
/// `Surface.getCapabilities`; the underlying wgpu-allocated
/// arrays are copied into owned ilang arrays before
/// `wgpuSurfaceCapabilitiesFreeMembers` is called.
pub class SurfaceCaps {
    pub usages:       TextureUsage
    pub formats:      TextureFormat[]
    pub presentModes: PresentMode[]
    pub alphaModes:   CompositeAlphaMode[]

    pub init() {
        this.usages       = TextureUsage.none
        this.formats      = []
        this.presentModes = []
        this.alphaModes   = []
    }

}
}
