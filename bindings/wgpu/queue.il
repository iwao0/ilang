/// `WGPUQueue` ops — submit + queue-write data uploads + the
/// friendly `Queue` wrapper class. The `WGPUQueue` handle itself
/// lives in `device.il` next to `WGPUDevice`.

use core { WGPUQueue, WGPUExtent3D }
use buffer { WGPUBuffer }
use texture { WGPUTexelCopyTextureInfo, WGPUTexelCopyBufferLayout }
use command { WGPUCommandBuffer }

@extern(C, "wgpu_native") {
    // `commands` is a `WGPUCommandBuffer const*` (array). Pass a
    // 1-element `WGPUCommandBuffer[]` (the array → pointer
    // coercion).
    @lib pub fn wgpuQueueSubmit(
        queue: WGPUQueue, commandCount: u64, commands: *WGPUCommandBuffer
    )

    // `wgpuQueueWriteBuffer` is the no-staging fast path for
    // small / single-shot uploads (uniform writes, per-frame
    // transform matrices). For larger data prefer `MapAsync` +
    // a CopyDst buffer.
    @lib pub fn wgpuQueueWriteBuffer(
        queue: WGPUQueue, buffer: WGPUBuffer, bufferOffset: u64,
        data: *const void, size: u64
    )
    // CPU → texture upload. `dataLayout.bytesPerRow` must be a
    // multiple of 256 (same WebGPU spec rule as
    // CopyTextureToBuffer); pad rows before calling.
    @lib pub fn wgpuQueueWriteTexture(
        queue:       WGPUQueue,
        destination: *WGPUTexelCopyTextureInfo,
        data:        *const void,
        dataSize:    u64,
        dataLayout:  *WGPUTexelCopyBufferLayout,
        writeSize:   *WGPUExtent3D
    )

    // ─── completion signal + lifetime ─────────────────────────
    //
    // `onSubmittedWorkDone` resolves when every previously
    // submitted command buffer has finished executing. Callback
    //   void(WGPUQueueWorkDoneStatus status, WGPUStringView msg,
    //        void* ud1, void* ud2)
    // → (status: i32, msg.data: i64, msg.len: i64, ud1: i64, ud2: i64).
    pub struct WGPUQueueWorkDoneCallbackInfo {
        nextInChain: *void
        mode:        u32                       // WGPUCallbackMode
        callback:    fn(i32, i64, i64, i64, i64)
        userdata1:   *void
        userdata2:   *void
    }
    @lib pub fn wgpuQueueOnSubmittedWorkDone(
        queue: WGPUQueue, callbackInfo: WGPUQueueWorkDoneCallbackInfo
    ): u64                                     // WGPUFuture
    @lib pub fn wgpuQueueRelease(queue: WGPUQueue)

    // ─── friendly wrapper ─────────────────────────────────────

    /// Wraps `WGPUQueue`. The queue is owned by the underlying
    /// `WGPUDevice` (one queue per device); the gui-side surface
    /// host hands one out via `view.queue`. Not released by this
    /// wrapper.
    pub class Queue {
        pub handle: i64

        pub init(handle: WGPUQueue) {
            this.handle = handle as i64
        }
    }
}
