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

    // ─── friendly wrapper ─────────────────────────────────────

    /// Compiled WGSL shader module. Wraps `WGPUShaderModule`.
    /// `Device.createWgslShader(code)` (pipeline.il) returns
    /// one; `release()` frees the underlying module.
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
    }
}
