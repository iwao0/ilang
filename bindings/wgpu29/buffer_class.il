/// Friendly `Buffer` + `BufferOptions` wrappers pulled out of
/// `buffer.il`. `Buffer.mapAsync` takes the friendly `Instance`
/// — possible only because `instance_class.il` has no class-file
/// dependencies (it sits at the base of the new class DAG).

use core {
    WGPUStringView, WGPUInstance,
    wgpuInstanceProcessEvents, CallbackMode, MapAsyncStatus,
}
use buffer {
    WGPUBuffer, WGPUBufferMapCallbackInfo, BufferUsage, MapMode,
    BufferMapState,
    wgpuBufferRelease, wgpuBufferDestroy, wgpuBufferSetLabel,
    wgpuBufferGetSize, wgpuBufferGetUsage, wgpuBufferGetMapState,
    wgpuBufferMapAsync,
}
use instance_class { Instance }
use std.ffi { cstrFromString }

// ─── sync wrapper — module-level OUT slot + C callback ───────
//
// Backs `Buffer.mapAsync`. The slot starts at `-1` (no valid
// `WGPUMapAsyncStatus` is negative), gets overwritten with the
// real status when the wgpu callback fires.
let _wgpuBufferMapOut: i32[] = [-1]

@extern(C) {
    pub fn _wgpuOnBufferMap(
        status: i32, msgData: i64, msgLen: i64, ud1: i64, ud2: i64
    ) {
        _wgpuBufferMapOut[0] = status
    }
}

@extern(C, "wgpu_native") {
/// GPU memory region. Wraps `WGPUBuffer`. Built via
/// `Device.createBuffer`; `release()` drops the host-side
/// refcount, `destroy()` drops the GPU-side storage eagerly.
pub class Buffer {
    pub handle: i64

    pub init(handle: WGPUBuffer) {
        this.handle = handle as i64
    }
    pub release() {
        if this.handle != 0 {
            wgpuBufferRelease(this.handle as WGPUBuffer)
            this.handle = 0
        }
    }
    pub destroy() {
        if this.handle != 0 {
            wgpuBufferDestroy(this.handle as WGPUBuffer)
        }
    }
    pub setLabel(label: string) {
        wgpuBufferSetLabel(this.handle as WGPUBuffer, WGPUStringView {
            data:   cstrFromString(label) as *void,
            length: 0xFFFFFFFFFFFFFFFF as u64,
        })
    }
    pub getSize(): u64 { wgpuBufferGetSize(this.handle as WGPUBuffer) }
    pub getUsage(): BufferUsage {
        wgpuBufferGetUsage(this.handle as WGPUBuffer) as BufferUsage
    }
    pub getMapState(): BufferMapState {
        wgpuBufferGetMapState(this.handle as WGPUBuffer) as BufferMapState
    }

    /// Synchronously map a buffer range. Internally pumps
    /// `wgpuInstanceProcessEvents` until the wgpu callback
    /// fires, then returns the resulting status. Use
    /// `getConstMappedRange` / `getMappedRange` afterwards.
    pub mapAsync(
        inst: Instance, mode: MapMode, offset: u64, size: u64
    ): MapAsyncStatus {
        let instHandle = inst.handle as WGPUInstance
        let info = WGPUBufferMapCallbackInfo {
            nextInChain: 0 as *void,
            mode:        CallbackMode.allowProcessEvents,
            callback:    _wgpuOnBufferMap,
            userdata1:   0 as *void,
            userdata2:   0 as *void,
        }
        _wgpuBufferMapOut[0] = -1
        wgpuBufferMapAsync(
            this.handle as WGPUBuffer, mode as u64, offset, size, info
        )
        let i: i64 = 0
        while _wgpuBufferMapOut[0] == -1 && i < 256 {
            wgpuInstanceProcessEvents(instHandle)
            i = i + 1
        }
        _wgpuBufferMapOut[0] as MapAsyncStatus
    }
}

/// Configuration for `Device.createBuffer`. `mappedAtCreation`
/// returns the buffer pre-mapped — call `GetMappedRange`,
/// fill the bytes, then `Unmap` before the first GPU read.
pub class BufferOptions {
    pub label:            string
    pub usage:            BufferUsage
    pub size:             u64
    pub mappedAtCreation: bool

    pub init(usage: BufferUsage, size: u64) {
        this.label            = ""
        this.usage            = usage
        this.size             = size
        this.mappedAtCreation = false
    }
}
}
