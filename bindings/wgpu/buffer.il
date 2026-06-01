/// `WGPUBuffer` — GPU memory region. Create with
/// `wgpuDeviceCreateBuffer`; upload with `Queue.WriteBuffer` (in
/// `queue.il`) or `MapAsync` + `GetConstMappedRange`. The map
/// callback is bare-fn (no closure capture); plumb its status
/// through a global cell.
///
/// Usage flag bits (u64):
///   MapRead  = 0x01,  MapWrite = 0x02,
///   CopySrc  = 0x04,  CopyDst  = 0x08,
///   Index    = 0x10,  Vertex   = 0x20,
///   Uniform  = 0x40,  Storage  = 0x80,
///   Indirect = 0x100, QueryResolve = 0x200.

use core { WGPUStringView }
use device { WGPUDevice }

@extern(C, "wgpu_native") {
    @handle pub struct WGPUBuffer {}

    /// `mappedAtCreation` is `WGPUBool` (u32). When set, the
    /// buffer comes back pre-mapped — call `GetMappedRange`
    /// (write) immediately, fill the bytes, then `Unmap` before
    /// the first GPU read.
    pub struct WGPUBufferDescriptor {
        nextInChain:      *void
        label:            WGPUStringView
        usage:            u64
        size:             u64
        mappedAtCreation: u32
    }
    @lib pub fn wgpuDeviceCreateBuffer(
        device: WGPUDevice, descriptor: *WGPUBufferDescriptor
    ): WGPUBuffer

    // Async map. Callback shape:
    //   void(WGPUMapAsyncStatus status, WGPUStringView message,
    //        void* userdata1, void* userdata2)
    // → (status: i32, msg.data: i64, msg.len: i64, ud1: i64, ud2: i64).
    pub struct WGPUBufferMapCallbackInfo {
        nextInChain: *void
        mode:        u32                            // CallbackMode
        callback:    fn(i32, i64, i64, i64, i64)
        userdata1:   *void
        userdata2:   *void
    }
    @lib pub fn wgpuBufferMapAsync(
        buffer: WGPUBuffer, mode: u64, offset: u64, size: u64,
        callbackInfo: WGPUBufferMapCallbackInfo
    ): u64
    // Mapped pointer is owned by wgpu until the matching unmap.
    @lib pub fn wgpuBufferGetConstMappedRange(
        buffer: WGPUBuffer, offset: u64, size: u64
    ): *const void
    @lib pub fn wgpuBufferUnmap(buffer: WGPUBuffer)

    @lib pub fn wgpuBufferGetSize(buffer: WGPUBuffer): u64
    // WGPUBufferMapState: Unmapped=1, Pending=2, Mapped=3.
    @lib pub fn wgpuBufferGetMapState(buffer: WGPUBuffer): u32
    // Same flag set as WGPUBufferDescriptor.usage.
    @lib pub fn wgpuBufferGetUsage(buffer: WGPUBuffer): u64

    // Releases GPU resources eagerly without dropping the
    // host-side wrapper. Subsequent operations fail in wgpu's
    // validation — call only when you know no in-flight command
    // can still reference the buffer.
    @lib pub fn wgpuBufferDestroy(buffer: WGPUBuffer)
    @lib pub fn wgpuBufferRelease(buffer: WGPUBuffer)
}
