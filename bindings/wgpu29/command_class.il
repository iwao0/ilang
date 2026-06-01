/// Friendly `CommandEncoder` / `CommandBuffer` / `RenderPass` /
/// `ComputePass` wrappers plus `ColorAttachmentOption`,
/// `DepthStencilAttachmentOption`, `RenderPassOptions` pulled
/// out of `command.il`.

use core { WGPUStringView, WGPUColor }
use buffer { WGPUBuffer }
use buffer_class { Buffer }
use texture { WGPUTextureView }
use texture_class { TextureView }
use pipeline { WGPURenderPipeline, WGPUComputePipeline, IndexFormat }
use pipeline_class { Pipeline, ComputePipeline }
use bind_group { WGPUBindGroup }
use bind_group_class { BindGroup }
use command {
    WGPUCommandEncoder, WGPUCommandBuffer,
    WGPURenderPassEncoder, WGPUComputePassEncoder,
    LoadOp, StoreOp,
    WGPURenderPassColorAttachment,
    WGPURenderPassDepthStencilAttachment,
    WGPURenderPassDescriptor, WGPUComputePassDescriptor,
    wgpuCommandEncoderBeginRenderPass,
    wgpuCommandEncoderBeginComputePass,
    wgpuCommandEncoderFinish,
    wgpuCommandEncoderCopyBufferToBuffer,
    wgpuCommandEncoderClearBuffer,
    wgpuCommandEncoderInsertDebugMarker,
    wgpuCommandEncoderPushDebugGroup,
    wgpuCommandEncoderPopDebugGroup,
    wgpuCommandEncoderSetLabel,
    wgpuCommandEncoderRelease,
    wgpuCommandBufferSetLabel, wgpuCommandBufferRelease,
    wgpuRenderPassEncoderSetPipeline,
    wgpuRenderPassEncoderDraw,
    wgpuRenderPassEncoderDrawIndexed,
    wgpuRenderPassEncoderDrawIndirect,
    wgpuRenderPassEncoderDrawIndexedIndirect,
    wgpuRenderPassEncoderSetVertexBuffer,
    wgpuRenderPassEncoderSetIndexBuffer,
    wgpuRenderPassEncoderSetViewport,
    wgpuRenderPassEncoderSetScissorRect,
    wgpuRenderPassEncoderSetBlendConstant,
    wgpuRenderPassEncoderSetStencilReference,
    wgpuRenderPassEncoderSetBindGroup,
    wgpuRenderPassEncoderEnd,
    wgpuComputePassEncoderSetPipeline,
    wgpuComputePassEncoderSetBindGroup,
    wgpuComputePassEncoderDispatchWorkgroups,
    wgpuComputePassEncoderDispatchWorkgroupsIndirect,
    wgpuComputePassEncoderEnd,
    wgpuComputePassEncoderSetLabel,
    wgpuComputePassEncoderRelease,
}
use std.ffi { cstrFromString }

@extern(C, "wgpu_native") {
/// Wraps `WGPURenderPassEncoder` for the gui-side `onRender`
/// callback. The underlying encoder's lifecycle (begin / end
/// / submit / present) is owned by gui-core; this wrapper
/// exposes only the recording verbs.
pub class RenderPass {
    pub handle: i64

    pub init(handle: WGPURenderPassEncoder) {
        this.handle = handle as i64
    }

    pub setPipeline(pipeline: Pipeline) {
        wgpuRenderPassEncoderSetPipeline(
            this.handle as WGPURenderPassEncoder,
            pipeline.handle as WGPURenderPipeline
        )
    }

    /// Record a draw of `vertexCount` vertices into
    /// `instanceCount` instances, starting at
    /// `firstVertex` / `firstInstance`. For the hello-triangle
    /// case use `draw(3, 1, 0, 0)`.
    pub draw(
        vertexCount: i64, instanceCount: i64,
        firstVertex: i64, firstInstance: i64
    ) {
        wgpuRenderPassEncoderDraw(
            this.handle as WGPURenderPassEncoder,
            vertexCount as u32, instanceCount as u32,
            firstVertex as u32, firstInstance as u32
        )
    }
    pub drawIndexed(
        indexCount:    i64, instanceCount: i64,
        firstIndex:    i64, baseVertex:    i64,
        firstInstance: i64
    ) {
        wgpuRenderPassEncoderDrawIndexed(
            this.handle as WGPURenderPassEncoder,
            indexCount as u32, instanceCount as u32,
            firstIndex as u32, baseVertex as i32,
            firstInstance as u32
        )
    }
    /// Indirect draw — params read from `indirectBuffer` at
    /// `indirectOffset` (4 × u32 for `draw`, 5 × u32 for
    /// `drawIndexed`).
    pub drawIndirect(buffer: Buffer, offset: u64) {
        wgpuRenderPassEncoderDrawIndirect(
            this.handle as WGPURenderPassEncoder,
            buffer.handle as WGPUBuffer, offset
        )
    }
    pub drawIndexedIndirect(buffer: Buffer, offset: u64) {
        wgpuRenderPassEncoderDrawIndexedIndirect(
            this.handle as WGPURenderPassEncoder,
            buffer.handle as WGPUBuffer, offset
        )
    }

    /// Bind a vertex buffer to `slot` (matches the index in
    /// the pipeline's `vertex.buffers[]`). `size` =
    /// `0xFFFFFFFFFFFFFFFF` means "to end of buffer".
    pub setVertexBuffer(slot: i64, buffer: Buffer, offset: u64, size: u64) {
        wgpuRenderPassEncoderSetVertexBuffer(
            this.handle as WGPURenderPassEncoder, slot as u32,
            buffer.handle as WGPUBuffer, offset, size
        )
    }
    pub setIndexBuffer(
        buffer: Buffer, format: IndexFormat,
        offset: u64, size: u64
    ) {
        wgpuRenderPassEncoderSetIndexBuffer(
            this.handle as WGPURenderPassEncoder,
            buffer.handle as WGPUBuffer,
            format, offset, size
        )
    }

    /// Bind a `BindGroup` to `groupIndex` (matches
    /// `@group(N)` in WGSL). No dynamic offsets — the
    /// common case.
    pub setBindGroup(groupIndex: i64, group: BindGroup) {
        wgpuRenderPassEncoderSetBindGroup(
            this.handle as WGPURenderPassEncoder,
            groupIndex as u32,
            group.handle as WGPUBindGroup,
            0 as u64, 0 as *const u32
        )
    }

    pub setViewport(
        x: f32, y: f32, width: f32, height: f32,
        minDepth: f32, maxDepth: f32
    ) {
        wgpuRenderPassEncoderSetViewport(
            this.handle as WGPURenderPassEncoder,
            x, y, width, height, minDepth, maxDepth
        )
    }
    pub setScissorRect(x: i64, y: i64, width: i64, height: i64) {
        wgpuRenderPassEncoderSetScissorRect(
            this.handle as WGPURenderPassEncoder,
            x as u32, y as u32, width as u32, height as u32
        )
    }
    /// Updates the constant-blend colour used by pipelines
    /// whose blend state references `ConstantColor` /
    /// `OneMinusConstantColor`.
    pub setBlendConstant(color: WGPUColor) {
        wgpuRenderPassEncoderSetBlendConstant(
            this.handle as WGPURenderPassEncoder, &color
        )
    }
    /// Updates the stencil-test reference value (the integer
    /// `ref` slot in the depth-stencil state).
    pub setStencilReference(reference: i64) {
        wgpuRenderPassEncoderSetStencilReference(
            this.handle as WGPURenderPassEncoder, reference as u32
        )
    }

    /// Ends the pass — every recording verb after this is an
    /// error. The encoder lifecycle (begin → end → submit →
    /// present) typically runs inside gui-core; call `end()`
    /// directly only when you're driving an off-screen pass
    /// yourself.
    pub end() {
        wgpuRenderPassEncoderEnd(this.handle as WGPURenderPassEncoder)
    }
}

/// Wraps `WGPUCommandBuffer` — the immutable output of a
/// `CommandEncoder.finish()` ready for `Queue.submit`.
pub class CommandBuffer {
    pub handle: i64

    pub init(handle: WGPUCommandBuffer) {
        this.handle = handle as i64
    }
    pub release() {
        if this.handle != 0 {
            wgpuCommandBufferRelease(this.handle as WGPUCommandBuffer)
            this.handle = 0
        }
    }
    pub setLabel(label: string) {
        wgpuCommandBufferSetLabel(this.handle as WGPUCommandBuffer, WGPUStringView {
            data:   cstrFromString(label) as *void,
            length: 0xFFFFFFFFFFFFFFFF as u64,
        })
    }
}

/// Wraps `WGPUComputePassEncoder`. Built via
/// `CommandEncoder.beginComputePass`.
pub class ComputePass {
    pub handle: i64

    pub init(handle: WGPUComputePassEncoder) {
        this.handle = handle as i64
    }
    pub release() {
        if this.handle != 0 {
            wgpuComputePassEncoderRelease(this.handle as WGPUComputePassEncoder)
            this.handle = 0
        }
    }
    pub setLabel(label: string) {
        wgpuComputePassEncoderSetLabel(this.handle as WGPUComputePassEncoder, WGPUStringView {
            data:   cstrFromString(label) as *void,
            length: 0xFFFFFFFFFFFFFFFF as u64,
        })
    }
    pub setPipeline(pipeline: ComputePipeline) {
        wgpuComputePassEncoderSetPipeline(
            this.handle as WGPUComputePassEncoder,
            pipeline.handle as WGPUComputePipeline
        )
    }
    pub setBindGroup(groupIndex: i64, group: BindGroup) {
        wgpuComputePassEncoderSetBindGroup(
            this.handle as WGPUComputePassEncoder,
            groupIndex as u32,
            group.handle as WGPUBindGroup,
            0 as u64, 0 as *const u32
        )
    }
    pub dispatchWorkgroups(x: i64, y: i64, z: i64) {
        wgpuComputePassEncoderDispatchWorkgroups(
            this.handle as WGPUComputePassEncoder,
            x as u32, y as u32, z as u32
        )
    }
    pub dispatchWorkgroupsIndirect(buffer: Buffer, offset: u64) {
        wgpuComputePassEncoderDispatchWorkgroupsIndirect(
            this.handle as WGPUComputePassEncoder,
            buffer.handle as WGPUBuffer, offset
        )
    }
    pub end() {
        wgpuComputePassEncoderEnd(this.handle as WGPUComputePassEncoder)
    }
}

/// Wraps `WGPUCommandEncoder`. Built via
/// `Device.createCommandEncoder`; record copy / debug / pass
/// verbs, then `finish()` to obtain the submittable buffer.
pub class CommandEncoder {
    pub handle: i64

    pub init(handle: WGPUCommandEncoder) {
        this.handle = handle as i64
    }
    pub release() {
        if this.handle != 0 {
            wgpuCommandEncoderRelease(this.handle as WGPUCommandEncoder)
            this.handle = 0
        }
    }
    pub setLabel(label: string) {
        wgpuCommandEncoderSetLabel(this.handle as WGPUCommandEncoder, WGPUStringView {
            data:   cstrFromString(label) as *void,
            length: 0xFFFFFFFFFFFFFFFF as u64,
        })
    }

    /// Begin a render pass from the supplied options.
    /// Supports multiple colour attachments + optional
    /// depth-stencil. For occlusion / timestamp slots drive
    /// the raw `wgpuCommandEncoderBeginRenderPass(enc, &desc)`
    /// directly.
    pub beginRenderPass(opts: RenderPassOptions): RenderPass {
        let attachments: WGPURenderPassColorAttachment[] = []
        let i: i64 = 0
        while i < opts.colors.length {
            let c = opts.colors[i]
            attachments.push(WGPURenderPassColorAttachment {
                nextInChain:   0 as *void,
                view:          c.view.handle as WGPUTextureView,
                depthSlice:    c.depthSlice as u32,
                resolveTarget: c.resolveTarget.handle as WGPUTextureView,
                loadOp:        c.loadOp,
                storeOp:       c.storeOp,
                clearValue:    c.clearValue,
            })
            i = i + 1
        }
        let ds = WGPURenderPassDepthStencilAttachment {
            view:              0 as WGPUTextureView,
            depthLoadOp:       LoadOp.undefined,
            depthStoreOp:      StoreOp.undefined,
            depthClearValue:   0.0,
            depthReadOnly:     0 as u32,
            stencilLoadOp:     LoadOp.undefined,
            stencilStoreOp:    StoreOp.undefined,
            stencilClearValue: 0 as u32,
            stencilReadOnly:   0 as u32,
        }
        let dsPtr: *void = 0 as *void
        if opts.hasDepth {
            let d = opts.depthStencil
            let dRO: u32 = 0 as u32
            let sRO: u32 = 0 as u32
            if d.depthReadOnly   { dRO = 1 as u32 }
            if d.stencilReadOnly { sRO = 1 as u32 }
            ds = WGPURenderPassDepthStencilAttachment {
                view:              d.view.handle as WGPUTextureView,
                depthLoadOp:       d.depthLoadOp,
                depthStoreOp:      d.depthStoreOp,
                depthClearValue:   d.depthClearValue,
                depthReadOnly:     dRO,
                stencilLoadOp:     d.stencilLoadOp,
                stencilStoreOp:    d.stencilStoreOp,
                stencilClearValue: d.stencilClearValue as u32,
                stencilReadOnly:   sRO,
            }
            dsPtr = (&ds) as *void
        }
        let desc = WGPURenderPassDescriptor {
            nextInChain:            0 as *void,
            label:                  WGPUStringView {
                data:   cstrFromString(opts.label) as *void,
                length: 0xFFFFFFFFFFFFFFFF as u64,
            },
            colorAttachmentCount:   attachments.length as u64,
            colorAttachments:       attachments,
            depthStencilAttachment: dsPtr,
            occlusionQuerySet:      0 as *void,
            timestampWrites:        0 as *void,
        }
        new RenderPass(wgpuCommandEncoderBeginRenderPass(
            this.handle as WGPUCommandEncoder, &desc
        ))
    }
    /// Begin a compute pass without timestamp writes — pass
    /// `0 as *void` for the descriptor.
    pub beginComputePass(): ComputePass {
        new ComputePass(wgpuCommandEncoderBeginComputePass(
            this.handle as WGPUCommandEncoder, 0 as *WGPUComputePassDescriptor
        ))
    }
    pub copyBufferToBuffer(
        src: Buffer, srcOffset: u64,
        dst: Buffer, dstOffset: u64, size: u64
    ) {
        wgpuCommandEncoderCopyBufferToBuffer(
            this.handle as WGPUCommandEncoder,
            src.handle as WGPUBuffer, srcOffset,
            dst.handle as WGPUBuffer, dstOffset, size
        )
    }
    pub clearBuffer(buf: Buffer, offset: u64, size: u64) {
        wgpuCommandEncoderClearBuffer(
            this.handle as WGPUCommandEncoder,
            buf.handle as WGPUBuffer, offset, size
        )
    }
    pub insertDebugMarker(label: string) {
        wgpuCommandEncoderInsertDebugMarker(
            this.handle as WGPUCommandEncoder,
            WGPUStringView {
                data:   cstrFromString(label) as *void,
                length: 0xFFFFFFFFFFFFFFFF as u64,
            }
        )
    }
    pub pushDebugGroup(label: string) {
        wgpuCommandEncoderPushDebugGroup(
            this.handle as WGPUCommandEncoder,
            WGPUStringView {
                data:   cstrFromString(label) as *void,
                length: 0xFFFFFFFFFFFFFFFF as u64,
            }
        )
    }
    pub popDebugGroup() {
        wgpuCommandEncoderPopDebugGroup(this.handle as WGPUCommandEncoder)
    }
    /// Finish recording and produce the submittable
    /// `CommandBuffer`. The encoder is consumed — using it
    /// after `finish` is an error.
    pub finish(): CommandBuffer {
        new CommandBuffer(wgpuCommandEncoderFinish(
            this.handle as WGPUCommandEncoder, 0 as *void
        ))
    }
}

/// One colour attachment for `RenderPassOptions`.
/// Mirrors `WGPURenderPassColorAttachment`. Construct with
/// `(view, loadOp, storeOp, clearValue)`; `resolveTarget`
/// defaults to null (no MSAA resolve) and `depthSlice` to
/// `WGPU_DEPTH_SLICE_UNDEFINED` (0xFFFFFFFF).
pub class ColorAttachmentOption {
    pub view:          TextureView
    pub resolveTarget: TextureView   // nullable; TextureView(0) = none
    pub loadOp:        LoadOp
    pub storeOp:       StoreOp
    pub clearValue:    WGPUColor
    pub depthSlice:    u32

    pub init(view: TextureView, loadOp: LoadOp, storeOp: StoreOp, clearValue: WGPUColor) {
        this.view          = view
        this.resolveTarget = new TextureView(0 as WGPUTextureView)
        this.loadOp        = loadOp
        this.storeOp       = storeOp
        this.clearValue    = clearValue
        this.depthSlice    = 0xFFFFFFFF as u32
    }
}

/// Optional depth-stencil attachment for `RenderPassOptions`.
/// Mirrors `WGPURenderPassDepthStencilAttachment`. Construct
/// with `(view)` then assign the relevant load/store/clear
/// fields before calling
/// `RenderPassOptions.setDepthStencil(...)`.
pub class DepthStencilAttachmentOption {
    pub view:              TextureView
    pub depthLoadOp:       LoadOp
    pub depthStoreOp:      StoreOp
    pub depthClearValue:   f32
    pub depthReadOnly:     bool
    pub stencilLoadOp:     LoadOp
    pub stencilStoreOp:    StoreOp
    pub stencilClearValue: i64
    pub stencilReadOnly:   bool

    pub init(view: TextureView) {
        this.view              = view
        this.depthLoadOp       = LoadOp.undefined
        this.depthStoreOp      = StoreOp.undefined
        this.depthClearValue   = 1.0
        this.depthReadOnly     = false
        this.stencilLoadOp     = LoadOp.undefined
        this.stencilStoreOp    = StoreOp.undefined
        this.stencilClearValue = 0
        this.stencilReadOnly   = false
    }
}

/// Configuration for `CommandEncoder.beginRenderPass`.
/// Holds an array of colour attachments and an optional
/// depth-stencil attachment (selected by `hasDepth`).
/// For occlusion / timestamp passes drive
/// `wgpuCommandEncoderBeginRenderPass` directly.
pub class RenderPassOptions {
    pub label:        string
    pub colors:       ColorAttachmentOption[]
    pub depthStencil: DepthStencilAttachmentOption
    pub hasDepth:     bool

    pub init(colors: ColorAttachmentOption[]) {
        this.label        = ""
        this.colors       = colors
        this.hasDepth     = false
        this.depthStencil = new DepthStencilAttachmentOption(
            new TextureView(0 as WGPUTextureView)
        )
    }

    /// Attach a depth-stencil view to this pass.
    pub setDepthStencil(d: DepthStencilAttachmentOption) {
        this.depthStencil = d
        this.hasDepth     = true
    }
}
}
