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
    wgpuBufferMapAsync, wgpuBufferUnmap,
    wgpuBufferGetMappedRange, wgpuBufferGetConstMappedRange,
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

    /// Release the host-side mapped range obtained via
    /// `mapAsync` + `getMappedRangeRaw` / `getConstMappedRangeRaw`,
    /// or via a buffer created with `mappedAtCreation: true`. The
    /// raw pointer becomes invalid after this call.
    pub unmap() {
        wgpuBufferUnmap(this.handle as WGPUBuffer)
    }

    /// Writable mapped range as a raw pointer (i64). Use
    /// `std.ffi.writeU32` / `writeF32` / `writeI32` / `writeU8`
    /// etc. to fill the bytes. Valid until `unmap()` (or the next
    /// `release()` / `destroy()`). For buffers created with
    /// `MapWrite` or `mappedAtCreation: true`.
    ///
    /// The raw pointer is returned as `i64` because friendly
    /// methods can't surface `*void` to caller code — pass the
    /// `i64` straight to the `std.ffi.read*` / `write*` helpers
    /// (they take `*void` / `*const void`, so cast at the call
    /// site).
    pub getMappedRangeRaw(offset: u64, size: u64): i64 {
        wgpuBufferGetMappedRange(this.handle as WGPUBuffer, offset, size) as i64
    }
    /// Read-only mapped range as a raw pointer (i64). Pairs with
    /// `MapMode.read` after `mapAsync`. Same lifetime rules as
    /// `getMappedRangeRaw`.
    pub getConstMappedRangeRaw(offset: u64, size: u64): i64 {
        wgpuBufferGetConstMappedRange(this.handle as WGPUBuffer, offset, size) as i64
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
