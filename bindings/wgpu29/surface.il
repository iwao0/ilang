/// `WGPUSurface` + the four platform-specific source structs +
/// configure / get-current-texture / present + capabilities
/// query. The friendly `Surface` class lives here too — it
/// simply wraps the handle for downstream code that prefers
/// method-style API.
///
/// `compatibleSurface` in `WGPURequestAdapterOptions`
/// (`adapter.il`) is typed as `*void` to avoid pulling this file
/// into the adapter-request module cycle; cast
/// `surface.handle as *void` at the call site.

use core { WGPUStringView, WGPUChainedStruct, WGPUDevice, WGPUInstance, WGPUAdapter }
use adapter { Adapter }
use texture { WGPUTexture, Texture, TextureUsage }
use device { Device }
use std.ffi { readU32 }

pub enum CompositeAlphaMode: u32 {
    auto            = 0x00
    opaque          = 0x01
    premultiplied   = 0x02
    unpremultiplied = 0x03
    inherit         = 0x04
}

pub enum PresentMode: u32 {
    undefined   = 0x00
    fifo        = 0x01
    fifoRelaxed = 0x02
    immediate   = 0x03
    mailbox     = 0x04
}

pub enum SurfaceGetCurrentTextureStatus: u32 {
    successOptimal    = 0x01
    successSuboptimal = 0x02
    timeout           = 0x03
    outdated          = 0x04
    lost              = 0x05
    error             = 0x06
}

@extern(C, "wgpu_native") {
    @handle pub struct WGPUSurface {}

    // ─── platform-specific surface sources ────────────────────
    // WGPUSType_SurfaceSourceMetalLayer = 4. Chain this off the
    // descriptor's nextInChain for the macOS surface.
    pub struct WGPUSurfaceSourceMetalLayer {
        chain: WGPUChainedStruct
        layer: *void              // CAMetalLayer*
    }
    // WGPUSType_SurfaceSourceWindowsHWND = 5. `hinstance` is the
    // module HINSTANCE, `hwnd` the window handle.
    pub struct WGPUSurfaceSourceWindowsHWND {
        chain:     WGPUChainedStruct
        hinstance: *void          // HINSTANCE
        hwnd:      *void          // HWND
    }
    // WGPUSType_SurfaceSourceXlibWindow = 6. `display` is the X
    // `Display*`, `window` the 64-bit XID.
    pub struct WGPUSurfaceSourceXlibWindow {
        chain:   WGPUChainedStruct
        display: *void            // Display*
        window:  u64              // X Window XID
    }
    // WGPUSType_SurfaceSourceWaylandSurface = 7. `display` is
    // `wl_display*`, `surface` is `wl_surface*`.
    pub struct WGPUSurfaceSourceWaylandSurface {
        chain:   WGPUChainedStruct
        display: *void            // wl_display*
        surface: *void            // wl_surface*
    }

    pub struct WGPUSurfaceDescriptor {
        nextInChain: *void        // WGPUChainedStruct*
        label:       WGPUStringView
    }
    // format = WGPUTextureFormat (enum, 4B). usage =
    // WGPUTextureUsage (RenderAttachment = 0x10).
    pub struct WGPUSurfaceConfiguration {
        nextInChain:     *void
        device:          WGPUDevice
        format:          TextureFormat
        usage:           TextureUsage
        width:           u32
        height:          u32
        viewFormatCount: u64
        viewFormats:     *void
        alphaMode:       CompositeAlphaMode
        presentMode:     PresentMode
    }
    pub struct WGPUSurfaceTexture {
        nextInChain: *void
        texture:     WGPUTexture
        status:      SurfaceGetCurrentTextureStatus
    }

    // Out-param: wgpu fills the arrays. The element pointers are
    // owned by wgpu (free with `wgpuSurfaceCapabilitiesFreeMembers`).
    pub struct WGPUSurfaceCapabilities {
        nextInChain:      *void
        usages:           TextureUsage
        formatCount:      u64
        formats:          *void    // WGPUTextureFormat const*
        presentModeCount: u64
        presentModes:     *void    // WGPUPresentMode const*
        alphaModeCount:   u64
        alphaModes:       *void    // WGPUCompositeAlphaMode const*
    }

    @lib pub fn wgpuInstanceCreateSurface(
        instance: WGPUInstance, descriptor: *WGPUSurfaceDescriptor
    ): WGPUSurface
    @lib pub fn wgpuSurfaceGetCapabilities(
        surface: WGPUSurface, adapter: WGPUAdapter,
        capabilities: *WGPUSurfaceCapabilities
    ): u32
    @lib pub fn wgpuSurfaceConfigure(
        surface: WGPUSurface, config: *WGPUSurfaceConfiguration
    )
    @lib pub fn wgpuSurfaceGetCurrentTexture(
        surface: WGPUSurface, surfaceTexture: *WGPUSurfaceTexture
    )
    @lib pub fn wgpuSurfacePresent(surface: WGPUSurface)
    // Drop the configured swap chain without releasing the
    // surface itself — useful before reconfiguring with a new
    // size / format.
    @lib pub fn wgpuSurfaceUnconfigure(surface: WGPUSurface)
    // Free the wgpu-allocated `formats` / `presentModes` /
    // `alphaModes` arrays inside a populated capabilities
    // struct. Call after copying out the values you need.
    @lib pub fn wgpuSurfaceCapabilitiesFreeMembers(capabilities: WGPUSurfaceCapabilities)
    @lib pub fn wgpuSurfaceSetLabel(surface: WGPUSurface, label: WGPUStringView)
    @lib pub fn wgpuSurfaceAddRef(surface: WGPUSurface)
    @lib pub fn wgpuSurfaceRelease(surface: WGPUSurface)

    // ─── friendly wrapper ─────────────────────────────────────

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
