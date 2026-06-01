/// `WGPUQueue` ops — submit + queue-write data uploads + the
/// friendly `Queue` wrapper class. The `WGPUQueue` handle itself
/// lives in `device.il` next to `WGPUDevice`.

use core {
    WGPUQueue, WGPUExtent3D, WGPUStringView, WGPUInstance,
    wgpuInstanceProcessEvents, CallbackMode,
}
use buffer { WGPUBuffer }
use texture { WGPUTexelCopyTextureInfo, WGPUTexelCopyBufferLayout }
use command { WGPUCommandBuffer, CommandBuffer }

pub enum QueueWorkDoneStatus: u32 {
    success           = 0x01
    callbackCancelled = 0x02
    error             = 0x03
}

// ─── sync wrapper — module-level OUT slot + C callback ───────
//
// Backs `Queue.onSubmittedWorkDone`. Slot starts at `-1` (no
// valid `WGPUQueueWorkDoneStatus` is negative), gets overwritten
// by the wgpu callback.
let _wgpuQueueWorkDoneOut: i32[] = [-1]

@extern(C) {
    pub fn _wgpuOnQueueWorkDone(
        status: i32, msgData: i64, msgLen: i64, ud1: i64, ud2: i64
    ) {
        _wgpuQueueWorkDoneOut[0] = status
    }
}

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
        mode:        CallbackMode
        callback:    fn(i32, i64, i64, i64, i64)
        userdata1:   *void
        userdata2:   *void
    }
    @lib pub fn wgpuQueueOnSubmittedWorkDone(
        queue: WGPUQueue, callbackInfo: WGPUQueueWorkDoneCallbackInfo
    ): u64                                     // WGPUFuture
    @lib pub fn wgpuQueueSetLabel(queue: WGPUQueue, label: WGPUStringView)
    @lib pub fn wgpuQueueAddRef(queue: WGPUQueue)
    @lib pub fn wgpuQueueRelease(queue: WGPUQueue)

    // wgpu-native extension. Same shape as `wgpuQueueSubmit` but
    // returns a `WGPUSubmissionIndex` (u64) that can be passed
    // to `wgpuDevicePoll(..., submissionIndex)` to wait on that
    // specific submission rather than every queued one.
    @lib pub fn wgpuQueueSubmitForIndex(
        queue: WGPUQueue, commandCount: u64, commands: *WGPUCommandBuffer
    ): u64

    // wgpu-native extension. Conversion factor (ns per timestamp
    // tick) for samples written by `WriteTimestamp` /
    // `ResolveQuerySet` on a Timestamp QuerySet. Multiply each
    // u64 sample by this to get nanoseconds. Returns 0.0 when
    // the timestamp feature isn't enabled.
    @lib pub fn wgpuQueueGetTimestampPeriod(queue: WGPUQueue): f32
    // wgpu-native macOS extension. Returns the underlying Metal
    // `id<MTLCommandQueue>`; null on non-Metal backends.
    @lib pub fn wgpuQueueGetNativeMetalCommandQueue(queue: WGPUQueue): *void

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

        /// Submit a list of pre-recorded command buffers.
        /// `cmds[0]` is presented first.
        pub submit(cmds: CommandBuffer[]) {
            let raw: WGPUCommandBuffer[] = []
            let i: i64 = 0
            while i < cmds.length {
                raw.push(cmds[i].handle as WGPUCommandBuffer)
                i = i + 1
            }
            wgpuQueueSubmit(this.handle as WGPUQueue, raw.length as u64, raw)
        }
        /// Single-buffer convenience for the common case.
        pub submitOne(cmd: CommandBuffer) {
            let cmds: WGPUCommandBuffer[] = [cmd.handle as WGPUCommandBuffer]
            wgpuQueueSubmit(this.handle as WGPUQueue, 1 as u64, cmds)
        }
        // `writeBuffer` / `writeTexture` take `*const void`
        // data (and `writeTexture` a `WGPUTexelCopyTextureInfo`
        // with chained pointers); ilang's friendly-method gate
        // rejects pointer-bearing parameters. Use the raw
        // `wgpuQueueWriteBuffer(q.handle as WGPUQueue, …)` /
        // `wgpuQueueWriteTexture(…)` exported through the
        // umbrella for those.

        /// ns per timestamp tick — multiply each u64 sample
        /// resolved out of a Timestamp QuerySet by this. 0.0 if
        /// the timestamp feature isn't enabled on the device.
        pub getTimestampPeriod(): f32 {
            wgpuQueueGetTimestampPeriod(this.handle as WGPUQueue)
        }

        /// Synchronously wait for every previously submitted
        /// command buffer to finish executing. Internally pumps
        /// `wgpuInstanceProcessEvents` until the wgpu callback
        /// fires, then returns the resulting status.
        ///
        /// `instHandle` is the owning `WGPUInstance` (used to
        /// pump events). Passed as a raw handle for symmetry
        /// with `Buffer.mapAsync` / `Adapter.requestDevice`.
        pub onSubmittedWorkDone(instHandle: WGPUInstance): QueueWorkDoneStatus {
            let info = WGPUQueueWorkDoneCallbackInfo {
                nextInChain: 0 as *void,
                mode:        CallbackMode.allowProcessEvents,
                callback:    _wgpuOnQueueWorkDone,
                userdata1:   0 as *void,
                userdata2:   0 as *void,
            }
            _wgpuQueueWorkDoneOut[0] = -1
            wgpuQueueOnSubmittedWorkDone(this.handle as WGPUQueue, info)
            let i: i64 = 0
            while _wgpuQueueWorkDoneOut[0] == -1 && i < 256 {
                wgpuInstanceProcessEvents(instHandle)
                i = i + 1
            }
            _wgpuQueueWorkDoneOut[0] as QueueWorkDoneStatus
        }
    }
}
