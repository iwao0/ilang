/// Friendly `RenderBundle` / `RenderBundleEncoder` wrappers
/// pulled out of `render_bundle.il`.

use core { WGPUStringView }
use buffer { WGPUBuffer }
use buffer_class { Buffer }
use bind_group { WGPUBindGroup }
use bind_group_class { BindGroup }
use pipeline { WGPURenderPipeline, IndexFormat }
use pipeline_class { Pipeline }
use render_bundle {
    WGPURenderBundle, WGPURenderBundleEncoder,
    WGPURenderBundleDescriptor,
    wgpuRenderBundleRelease, wgpuRenderBundleSetLabel,
    wgpuRenderBundleEncoderRelease, wgpuRenderBundleEncoderSetLabel,
    wgpuRenderBundleEncoderFinish,
    wgpuRenderBundleEncoderSetPipeline,
    wgpuRenderBundleEncoderSetBindGroup,
    wgpuRenderBundleEncoderSetVertexBuffer,
    wgpuRenderBundleEncoderSetIndexBuffer,
    wgpuRenderBundleEncoderDraw,
    wgpuRenderBundleEncoderDrawIndexed,
    wgpuRenderBundleEncoderDrawIndirect,
    wgpuRenderBundleEncoderDrawIndexedIndirect,
}
use std.ffi { cstrFromString }

@extern(C, "wgpu_native") {
/// Wraps `WGPURenderBundle`. Built via
/// `RenderBundleEncoder.finish`.
pub class RenderBundle {
    pub handle: i64

    pub init(handle: WGPURenderBundle) {
        this.handle = handle as i64
    }
    pub release() {
        if this.handle != 0 {
            wgpuRenderBundleRelease(this.handle as WGPURenderBundle)
            this.handle = 0
        }
    }
    pub setLabel(label: string) {
        wgpuRenderBundleSetLabel(this.handle as WGPURenderBundle, WGPUStringView {
            data:   cstrFromString(label) as *void,
            length: 0xFFFFFFFFFFFFFFFF as u64,
        })
    }
}

/// Wraps `WGPURenderBundleEncoder`. The encoder is consumed
/// by `finish` — drop the wrapper after calling it.
pub class RenderBundleEncoder {
    pub handle: i64

    pub init(handle: WGPURenderBundleEncoder) {
        this.handle = handle as i64
    }
    pub release() {
        if this.handle != 0 {
            wgpuRenderBundleEncoderRelease(this.handle as WGPURenderBundleEncoder)
            this.handle = 0
        }
    }
    pub setLabel(label: string) {
        wgpuRenderBundleEncoderSetLabel(this.handle as WGPURenderBundleEncoder, WGPUStringView {
            data:   cstrFromString(label) as *void,
            length: 0xFFFFFFFFFFFFFFFF as u64,
        })
    }
    pub setPipeline(pipeline: Pipeline) {
        wgpuRenderBundleEncoderSetPipeline(
            this.handle as WGPURenderBundleEncoder,
            pipeline.handle as WGPURenderPipeline
        )
    }
    pub setBindGroup(groupIndex: i64, group: BindGroup) {
        wgpuRenderBundleEncoderSetBindGroup(
            this.handle as WGPURenderBundleEncoder,
            groupIndex as u32,
            group.handle as WGPUBindGroup,
            0 as u64, 0 as *const u32
        )
    }
    pub setVertexBuffer(slot: i64, buffer: Buffer, offset: u64, size: u64) {
        wgpuRenderBundleEncoderSetVertexBuffer(
            this.handle as WGPURenderBundleEncoder, slot as u32,
            buffer.handle as WGPUBuffer, offset, size
        )
    }
    pub setIndexBuffer(buffer: Buffer, format: IndexFormat, offset: u64, size: u64) {
        wgpuRenderBundleEncoderSetIndexBuffer(
            this.handle as WGPURenderBundleEncoder,
            buffer.handle as WGPUBuffer,
            format as u32, offset, size
        )
    }
    pub draw(
        vertexCount: i64, instanceCount: i64,
        firstVertex: i64, firstInstance: i64
    ) {
        wgpuRenderBundleEncoderDraw(
            this.handle as WGPURenderBundleEncoder,
            vertexCount as u32, instanceCount as u32,
            firstVertex as u32, firstInstance as u32
        )
    }
    pub drawIndexed(
        indexCount:    i64, instanceCount: i64,
        firstIndex:    i64, baseVertex:    i64,
        firstInstance: i64
    ) {
        wgpuRenderBundleEncoderDrawIndexed(
            this.handle as WGPURenderBundleEncoder,
            indexCount as u32, instanceCount as u32,
            firstIndex as u32, baseVertex as i32,
            firstInstance as u32
        )
    }
    pub drawIndirect(buffer: Buffer, offset: u64) {
        wgpuRenderBundleEncoderDrawIndirect(
            this.handle as WGPURenderBundleEncoder,
            buffer.handle as WGPUBuffer, offset
        )
    }
    pub drawIndexedIndirect(buffer: Buffer, offset: u64) {
        wgpuRenderBundleEncoderDrawIndexedIndirect(
            this.handle as WGPURenderBundleEncoder,
            buffer.handle as WGPUBuffer, offset
        )
    }
    /// Finish recording and produce a replayable
    /// `RenderBundle`. Encoder is consumed.
    pub finish(): RenderBundle {
        new RenderBundle(wgpuRenderBundleEncoderFinish(
            this.handle as WGPURenderBundleEncoder,
            0 as *WGPURenderBundleDescriptor
        ))
    }
}
}
