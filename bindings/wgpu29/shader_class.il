/// Friendly `Shader` wrapper for `WGPUShaderModule`. Pulled out
/// of `shader.il` so the feature file is pure C-ABI declarations;
/// downstream class files (`device_class.il`, `pipeline_class.il`)
/// import this without picking up cycles.

use core {
    WGPUInstance, wgpuInstanceProcessEvents, CallbackMode,
}
use device { CompilationInfoRequestStatus, CompilationMessageType }
use shader {
    WGPUShaderModule, WGPUCompilationInfoCallbackInfo,
    wgpuShaderModuleRelease, wgpuShaderModuleGetCompilationInfo,
}
use instance_class { Instance }
use std.ffi { readU32, readU64, stringFromBytes }

// ─── sync wrapper — module-level OUT slot + C callback ───────
//
// Backs `Shader.getCompilationInfo`. The slot starts at `-1` (a
// sentinel — no valid status is negative, and the info pointer
// is never read until the status leaves -1). Both the request
// status and the `WGPUCompilationInfo const *` pointer captured
// from the wgpu callback land here so the synchronous wrapper
// can walk the message array after the spin completes.
let _wgpuCompInfoStatusOut: i32[] = [-1]
let _wgpuCompInfoPtrOut:    i64[] = [0]

@extern(C) {
    pub fn _wgpuOnCompilationInfo(
        status: i32, info: i64, ud1: i64, ud2: i64
    ) {
        _wgpuCompInfoPtrOut[0]    = info
        _wgpuCompInfoStatusOut[0] = status
    }
}

@extern(C, "wgpu_native") {
/// Compiled WGSL shader module. Wraps `WGPUShaderModule`.
/// `Device.createWgslShader(code)` returns one; `release()`
/// frees the underlying module.
pub class Shader {
    pub handle: i64

    pub init(handle: WGPUShaderModule) {
        this.handle = handle as i64
    }
    pub release() {
        if this.handle != 0 {
            wgpuShaderModuleRelease(this.handle as WGPUShaderModule)
            this.handle = 0
        }
    }

    /// Synchronously fetch WGSL parse / validation diagnostics
    /// for this shader. Internally pumps
    /// `wgpuInstanceProcessEvents` until the wgpu callback fires,
    /// then walks the resulting `WGPUCompilationInfo` message
    /// array into owned ilang values. The returned
    /// `CompilationInfo` is safe to keep across the callback
    /// pointer's invalidation (every string is copied).
    pub getCompilationInfo(inst: Instance): CompilationInfo {
        let instHandle = inst.handle as WGPUInstance
        let info = WGPUCompilationInfoCallbackInfo {
            nextInChain: 0 as *void,
            mode:        CallbackMode.allowProcessEvents,
            callback:    _wgpuOnCompilationInfo,
            userdata1:   0 as *void,
            userdata2:   0 as *void,
        }
        _wgpuCompInfoStatusOut[0] = -1
        _wgpuCompInfoPtrOut[0]    = 0
        wgpuShaderModuleGetCompilationInfo(
            this.handle as WGPUShaderModule, info
        )
        let i: i64 = 0
        while _wgpuCompInfoStatusOut[0] == -1 && i < 256 {
            wgpuInstanceProcessEvents(instHandle)
            i = i + 1
        }
        let out = new CompilationInfo()
        out.status = _wgpuCompInfoStatusOut[0] as CompilationInfoRequestStatus

        // WGPUCompilationInfo layout (pointer-sized + size_t + ptr):
        //   off  0: WGPUChainedStruct const* nextInChain
        //   off  8: size_t                   messageCount
        //   off 16: WGPUCompilationMessage const* messages
        let infoPtr = _wgpuCompInfoPtrOut[0]
        if infoPtr != 0 {
            let count    = readU64(infoPtr as *const void, 8 as i64) as i64
            let msgsBase = readU64(infoPtr as *const void, 16 as i64) as i64

            // WGPUCompilationMessage layout (64 B w/ 4 B tail pad):
            //   off  0: WGPUChainedStruct const* nextInChain
            //   off  8: WGPUStringView           message
            //              .data (off  8)
            //              .length (off 16)
            //   off 24: WGPUCompilationMessageType type (u32)
            //   off 28: (padding)
            //   off 32: u64 lineNum
            //   off 40: u64 linePos
            //   off 48: u64 offset
            //   off 56: u64 length
            let stride: i64 = 64
            let k: i64 = 0
            while k < count {
                let m = msgsBase + k * stride
                let msgData = readU64(m as *const void, 8  as i64) as i64
                let msgLen  = readU64(m as *const void, 16 as i64) as i64
                let cm = new CompilationMessage()
                cm.message = stringFromBytes(msgData as *const void, msgLen)
                cm.type    = readU32(m as *const void, 24 as i64) as CompilationMessageType
                cm.lineNum = readU64(m as *const void, 32 as i64)
                cm.linePos = readU64(m as *const void, 40 as i64)
                cm.offset  = readU64(m as *const void, 48 as i64)
                cm.length  = readU64(m as *const void, 56 as i64)
                out.messages.push(cm)
                k = k + 1
            }
        }
        out
    }
}

/// Single diagnostic from `Shader.getCompilationInfo`. `message`
/// is an owned copy of the WGSL-side `WGPUStringView` payload —
/// safe to keep after the callback pointer goes stale.
/// `lineNum` / `linePos` are 1-based; `offset` / `length` are
/// byte spans into the original WGSL source.
pub class CompilationMessage {
    pub message: string
    pub type:    CompilationMessageType
    pub lineNum: u64
    pub linePos: u64
    pub offset:  u64
    pub length:  u64

    pub init() {
        this.message = ""
        this.type    = CompilationMessageType.info
        this.lineNum = 0 as u64
        this.linePos = 0 as u64
        this.offset  = 0 as u64
        this.length  = 0 as u64
    }
}

/// Result of `Shader.getCompilationInfo`. `status` is the
/// `WGPUCompilationInfoRequestStatus` reported by wgpu;
/// `messages` is the (possibly empty) list of diagnostics
/// pulled out of the C-side `WGPUCompilationInfo`.
pub class CompilationInfo {
    pub status:   CompilationInfoRequestStatus
    pub messages: CompilationMessage[]

    pub init() {
        this.status   = CompilationInfoRequestStatus.success
        this.messages = []
    }
}
}
