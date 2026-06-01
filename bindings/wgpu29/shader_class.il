/// Friendly `Shader` wrapper for `WGPUShaderModule`. Pulled out
/// of `shader.il` so the feature file is pure C-ABI declarations;
/// downstream class files (`device_class.il`, `pipeline_class.il`)
/// import this without picking up cycles.

use shader { WGPUShaderModule, wgpuShaderModuleRelease }

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
    }
}
