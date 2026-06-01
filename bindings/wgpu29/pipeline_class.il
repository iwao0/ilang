/// Friendly `Pipeline` / `ComputePipeline` / `RenderPipelineOptions`
/// / `ComputePipelineOptions` wrappers pulled out of
/// `pipeline.il`.

use core { WGPUStringView, TextureFormat }
use pipeline {
    WGPURenderPipeline, WGPUComputePipeline,
    PrimitiveTopology, CullMode, FrontFace,
    wgpuRenderPipelineRelease, wgpuRenderPipelineSetLabel,
    wgpuComputePipelineRelease, wgpuComputePipelineSetLabel,
}
use shader_class { Shader }
use std.ffi { cstrFromString }

@extern(C, "wgpu_native") {
/// Render pipeline. Built via `Device.createRenderPipeline`.
pub class Pipeline {
    pub handle: i64

    pub init(handle: WGPURenderPipeline) {
        this.handle = handle as i64
    }
    pub release() {
        if this.handle != 0 {
            wgpuRenderPipelineRelease(this.handle as WGPURenderPipeline)
            this.handle = 0
        }
    }
    pub setLabel(label: string) {
        wgpuRenderPipelineSetLabel(this.handle as WGPURenderPipeline, WGPUStringView {
            data:   cstrFromString(label) as *void,
            length: 0xFFFFFFFFFFFFFFFF as u64,
        })
    }
}

/// Compute pipeline. Built via `Device.createComputePipeline`.
pub class ComputePipeline {
    pub handle: i64

    pub init(handle: WGPUComputePipeline) {
        this.handle = handle as i64
    }
    pub release() {
        if this.handle != 0 {
            wgpuComputePipelineRelease(this.handle as WGPUComputePipeline)
            this.handle = 0
        }
    }
    pub setLabel(label: string) {
        wgpuComputePipelineSetLabel(this.handle as WGPUComputePipeline, WGPUStringView {
            data:   cstrFromString(label) as *void,
            length: 0xFFFFFFFFFFFFFFFF as u64,
        })
    }
}

/// Configuration for `Device.createComputePipeline`. The
/// pipeline layout is auto-derived from the shader module's
/// reflection (pass a layout-bearing variant by calling
/// `wgpuDeviceCreateComputePipeline` directly).
pub class ComputePipelineOptions {
    pub label:      string
    pub shader:     Shader
    pub entryPoint: string

    pub init(shader: Shader, entryPoint: string) {
        this.label      = ""
        this.shader     = shader
        this.entryPoint = entryPoint
    }
}

/// Configuration for `Device.createRenderPipeline`. Defaults
/// cover the "draw a single coloured triangle list against
/// the surface format" path; tweak fields before passing in.
pub class RenderPipelineOptions {
    pub vertexShader:   Shader
    pub vertexEntry:    string
    pub fragmentShader: Shader
    pub fragmentEntry:  string
    /// `TextureFormat` of the colour target. Use the value
    /// `GpuView.format` reports so the pipeline matches the
    /// surface.
    pub colorFormat:    TextureFormat
    pub topology:       PrimitiveTopology
    pub cullMode:       CullMode
    pub frontFace:      FrontFace

    /// Same shader module for vertex + fragment is the
    /// common case (one WGSL file with `vs_main` + `fs_main`).
    pub init(vertex: Shader, fragment: Shader, colorFormat: TextureFormat) {
        this.vertexShader   = vertex
        this.vertexEntry    = "vs_main"
        this.fragmentShader = fragment
        this.fragmentEntry  = "fs_main"
        this.colorFormat    = colorFormat
        this.topology       = PrimitiveTopology.triangleList
        this.cullMode       = CullMode.none
        this.frontFace      = FrontFace.ccw
    }
}
}
