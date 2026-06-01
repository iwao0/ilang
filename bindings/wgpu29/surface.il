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

use core { WGPUStringView, WGPUChainedStruct, WGPUDevice }
use instance { WGPUInstance }
use adapter { WGPUAdapter }
use texture { WGPUTexture }

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
        format:          u32
        usage:           u64
        width:           u32
        height:          u32
        viewFormatCount: u64
        viewFormats:     *void
        alphaMode:       u32
        presentMode:     u32
    }
    pub struct WGPUSurfaceTexture {
        nextInChain: *void
        texture:     WGPUTexture
        status:      u32          // SuccessOptimal = 1
    }

    // Out-param: wgpu fills the arrays. The element pointers are
    // owned by wgpu (free with `wgpuSurfaceCapabilitiesFreeMembers`).
    pub struct WGPUSurfaceCapabilities {
        nextInChain:      *void
        usages:           u64
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
    }
}
