/// Friendly `Queue` wrapper pulled out of `queue.il`.
/// `Queue.onSubmittedWorkDone` takes the friendly `Instance`
/// (possible now that `instance_class.il` has no class-file deps).

use core {
    WGPUQueue, WGPUInstance, WGPUExtent3D, WGPUOrigin3D,
    wgpuInstanceProcessEvents, CallbackMode,
}
use queue {
    QueueWorkDoneStatus, WGPUQueueWorkDoneCallbackInfo,
    wgpuQueueSubmit, wgpuQueueGetTimestampPeriod,
    wgpuQueueOnSubmittedWorkDone,
    wgpuQueueWriteBuffer, wgpuQueueWriteTexture,
}
use buffer { WGPUBuffer }
use buffer_class { Buffer }
use texture {
    WGPUTexture, TextureAspect,
    WGPUTexelCopyTextureInfo, WGPUTexelCopyBufferLayout,
}
use texture_class { Texture }
use command { WGPUCommandBuffer }
use command_class { CommandBuffer }
use instance_class { Instance }

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

    /// Upload a typed array to `buf` at `bufferOffset`. `elemSize` is
    /// the per-element byte width (1 for `u8`, 4 for `i32`/`u32`/`f32`,
    /// etc.) — generic methods can't yet introspect `sizeof<T>`, so the
    /// caller passes it explicitly. Once we add a `sizeof<T>` intrinsic
    /// the parameter can go away.
    pub writeBuffer<T>(buf: Buffer, bufferOffset: u64, data: T[], elemSize: i64) {
        wgpuQueueWriteBuffer(
            this.handle as WGPUQueue, buf.handle as WGPUBuffer,
            bufferOffset, data, (data.length * elemSize) as u64
        )
    }

    /// CPU → texture upload. Builds the `WGPUTexelCopyTextureInfo`
    /// + `WGPUTexelCopyBufferLayout` from the friendly args, then
    /// calls `wgpuQueueWriteTexture`. `bytesPerRow` must be a
    /// multiple of 256 (WebGPU spec) — pad rows before calling.
    pub writeTexture(
        destTexture: Texture, mipLevel: i64,
        origin: WGPUOrigin3D, aspect: TextureAspect,
        data: u8[], bytesPerRow: u32, rowsPerImage: u32,
        writeSize: WGPUExtent3D
    ) {
        let dest = WGPUTexelCopyTextureInfo {
            texture:  destTexture.handle as WGPUTexture,
            mipLevel: mipLevel as u32,
            origin:   origin,
            aspect:   aspect,
        }
        let layout = WGPUTexelCopyBufferLayout {
            offset:       0 as u64,
            bytesPerRow:  bytesPerRow,
            rowsPerImage: rowsPerImage,
        }
        let size = writeSize
        wgpuQueueWriteTexture(
            this.handle as WGPUQueue, &dest, data, data.length as u64,
            &layout, &size
        )
    }

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
    pub onSubmittedWorkDone(inst: Instance): QueueWorkDoneStatus {
        let instHandle = inst.handle as WGPUInstance
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
