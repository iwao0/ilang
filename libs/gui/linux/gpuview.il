/// Linux stub for `GpuView`. The X11 / Wayland surface path is
/// not implemented yet — every entry point returns the
/// "unrealized" sentinel (`-1` / null handle) so the public
/// class on `libs/gui/gpuview.il` silently no-ops. macOS is the
/// only target with a working backend right now.

use wgpu {
    WGPUSurface, WGPUDevice, WGPUQueue, WGPURenderPassEncoder,
}

pub fn createNativeGpuView(
    windowHandle: i64,
    x: f64, y: f64, width: f64, height: f64, visible: bool
): i64 {
    -1
}

pub fn setGpuViewFrame(handle: i64, x: f64, y: f64, w: f64, h: f64) {}
pub fn setGpuViewVisible(handle: i64, visible: bool) {}
pub fn setOnGpuViewRender(handle: i64, cb: fn(WGPURenderPassEncoder)) {}

pub fn getGpuViewSurface(handle: i64): WGPUSurface { WGPUSurface.__wrap_handle(0) }
pub fn getGpuViewDevice(handle: i64):  WGPUDevice  { WGPUDevice.__wrap_handle(0) }
pub fn getGpuViewQueue(handle: i64):   WGPUQueue   { WGPUQueue.__wrap_handle(0) }
pub fn getGpuViewFormat(handle: i64):  u32         { 0 as u32 }

pub fn startGpuViewLoop(handle: i64) {}
pub fn stopGpuViewLoop(handle: i64) {}
pub fn requestGpuViewFrame(handle: i64) {}
