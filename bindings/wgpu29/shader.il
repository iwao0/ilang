/// `WGPUShaderModule` + WGSL source descriptor + the friendly
/// `Shader` wrapper class. The raw `wgpuDeviceCreateShaderModule`
/// lives here because it's logically "produce a Shader" — the
/// `WGPUDevice` import is one-way (this file uses Device; Device
/// doesn't import Shader directly).

use core { WGPUStringView, WGPUChainedStruct, WGPUDevice }

@extern(C, "wgpu_native") {
    @handle pub struct WGPUShaderModule {}

    // WGPUSType_ShaderSourceWGSL = 2. Chain off the descriptor's
    // nextInChain. `code` / entry-point StringViews can use
    // length = WGPU_STRLEN (= u64::MAX) to let wgpu strlen the
    // data.
    pub struct WGPUShaderSourceWGSL {
        chain: WGPUChainedStruct
        code:  WGPUStringView
    }
    pub struct WGPUShaderModuleDescriptor {
        nextInChain: *void
        label:       WGPUStringView
    }
    @lib pub fn wgpuDeviceCreateShaderModule(
        device: WGPUDevice, descriptor: *WGPUShaderModuleDescriptor
    ): WGPUShaderModule
    @lib pub fn wgpuShaderModuleSetLabel(shaderModule: WGPUShaderModule, label: WGPUStringView)
    @lib pub fn wgpuShaderModuleAddRef(shaderModule: WGPUShaderModule)
    @lib pub fn wgpuShaderModuleRelease(shaderModule: WGPUShaderModule)

    // ─── compilation info (async) ─────────────────────────────
    //
    // Callback shape:
    //   void(WGPUCompilationInfoRequestStatus status,
    //        WGPUCompilationInfo const* info, void* ud1, void* ud2)
    // → (status: i32, info: i64, ud1: i64, ud2: i64). Dereference
    // `info` via std.ffi readers to walk the WGPUCompilationMessage
    // array (line / column / type / message).
    pub struct WGPUCompilationInfoCallbackInfo {
        nextInChain: *void
        mode:        CallbackMode
        callback:    fn(i32, i64, i64, i64)
        userdata1:   *void
        userdata2:   *void
    }
    @lib pub fn wgpuShaderModuleGetCompilationInfo(
        shaderModule: WGPUShaderModule,
        callbackInfo: WGPUCompilationInfoCallbackInfo
    ): u64                                          // WGPUFuture

    // ─── SPIR-V shader module (wgpu-native extension) ─────────
    //
    // For when you'd rather compile to SPIR-V offline than ship
    // WGSL. `source` is a flat u32 array; `sourceSize` is the
    // word count (not byte count).
    pub struct WGPUShaderModuleDescriptorSpirV {
        label:      WGPUStringView
        sourceSize: u32
        source:     *const u32
    }
    @lib pub fn wgpuDeviceCreateShaderModuleSpirV(
        device: WGPUDevice, descriptor: *WGPUShaderModuleDescriptorSpirV
    ): WGPUShaderModule
}

// `Shader` wrapper class lives in `shader_class.il`.
