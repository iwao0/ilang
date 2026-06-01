/// `WGPURenderBundle` — pre-recorded draw call sequence that
/// replays cheaply inside a render pass. Build a bundle with
/// `Device.createRenderBundleEncoder()` → record the same set
/// of `setPipeline / setBindGroup / setVertexBuffer / draw*`
/// calls a regular render pass takes → `finish()` to get a
/// `WGPURenderBundle`. Then issue
/// `wgpuRenderPassEncoderExecuteBundles` once per frame instead
/// of re-recording the calls.
///
/// The bundle encoder shares the verb set with `RenderPassEncoder`
/// (setPipeline / setBindGroup / setVertex/IndexBuffer / draw /
/// drawIndexed / draw{,Indexed}Indirect) plus the debug-group
/// triplet — but explicitly NOT viewport / scissor / blend
/// constant / stencil reference, which stay on the parent render
/// pass.
///
/// The encoder descriptor pins the *layout* the bundle is
/// compatible with: colour formats, depth-stencil format, sample
/// count. A bundle baked against `bgra8UnormSrgb + Depth24Plus`
/// can only execute in a render pass whose attachments match
/// (validated at executeBundles time).

use core { WGPUStringView, WGPUDevice }
use buffer { WGPUBuffer }
use bind_group { WGPUBindGroup }
use pipeline { WGPURenderPipeline }
use command { WGPURenderPassEncoder }

@extern(C, "wgpu_native") {
    @handle pub struct WGPURenderBundle {}
    @handle pub struct WGPURenderBundleEncoder {}

    pub struct WGPURenderBundleEncoderDescriptor {
        nextInChain:        *void
        label:              WGPUStringView
        // Array length + pointer for the colour-attachment
        // formats this bundle will write to. Element type =
        // `WGPUTextureFormat` (u32).
        colorFormatCount:   u64
        colorFormats:       *void
        // `WGPUTextureFormat_Undefined` (0) when no depth-stencil
        // attachment is in use.
        depthStencilFormat: u32
        sampleCount:        u32
        depthReadOnly:      u32     // WGPUBool
        stencilReadOnly:    u32     // WGPUBool
    }
    pub struct WGPURenderBundleDescriptor {
        nextInChain: *void
        label:       WGPUStringView
    }

    @lib pub fn wgpuDeviceCreateRenderBundleEncoder(
        device: WGPUDevice, descriptor: *WGPURenderBundleEncoderDescriptor
    ): WGPURenderBundleEncoder

    // `descriptor` may be null for an unlabelled bundle. The
    // encoder is consumed by `finish` — don't use it after.
    @lib pub fn wgpuRenderBundleEncoderFinish(
        encoder: WGPURenderBundleEncoder,
        descriptor: *WGPURenderBundleDescriptor
    ): WGPURenderBundle

    // ─── encoder verbs (mirror RenderPassEncoder where useful) ─
    @lib pub fn wgpuRenderBundleEncoderSetPipeline(
        encoder: WGPURenderBundleEncoder, pipeline: WGPURenderPipeline
    )
    @lib pub fn wgpuRenderBundleEncoderSetBindGroup(
        encoder:            WGPURenderBundleEncoder,
        groupIndex:         u32,
        group:              WGPUBindGroup,
        dynamicOffsetCount: u64,
        dynamicOffsets:     *const u32
    )
    @lib pub fn wgpuRenderBundleEncoderSetVertexBuffer(
        encoder: WGPURenderBundleEncoder, slot: u32,
        buffer:  WGPUBuffer, offset: u64, size: u64
    )
    @lib pub fn wgpuRenderBundleEncoderSetIndexBuffer(
        encoder: WGPURenderBundleEncoder, buffer: WGPUBuffer,
        format: u32,            // WGPUIndexFormat
        offset: u64, size: u64
    )
    @lib pub fn wgpuRenderBundleEncoderDraw(
        encoder: WGPURenderBundleEncoder,
        vertexCount: u32, instanceCount: u32,
        firstVertex: u32, firstInstance: u32
    )
    @lib pub fn wgpuRenderBundleEncoderDrawIndexed(
        encoder:       WGPURenderBundleEncoder,
        indexCount:    u32, instanceCount: u32,
        firstIndex:    u32, baseVertex:    i32,
        firstInstance: u32
    )
    @lib pub fn wgpuRenderBundleEncoderDrawIndirect(
        encoder: WGPURenderBundleEncoder,
        indirectBuffer: WGPUBuffer, indirectOffset: u64
    )
    @lib pub fn wgpuRenderBundleEncoderDrawIndexedIndirect(
        encoder: WGPURenderBundleEncoder,
        indirectBuffer: WGPUBuffer, indirectOffset: u64
    )

    // ─── debug groups (encoder-only) ──────────────────────────
    //
    // Push/pop and insert nest a label that shows up in PIX /
    // RenderDoc / Xcode-frame-debugger captures. `markerLabel` /
    // `groupLabel` accept the same `WGPUStringView` shape as
    // every other label argument — pass
    //   WGPUStringView { data: cstrFromString("name") as *void,
    //                    length: 0xFFFFFFFFFFFFFFFF as u64 }
    // to let wgpu strlen the value.
    @lib pub fn wgpuRenderBundleEncoderInsertDebugMarker(
        encoder: WGPURenderBundleEncoder, markerLabel: WGPUStringView
    )
    @lib pub fn wgpuRenderBundleEncoderPushDebugGroup(
        encoder: WGPURenderBundleEncoder, groupLabel: WGPUStringView
    )
    @lib pub fn wgpuRenderBundleEncoderPopDebugGroup(
        encoder: WGPURenderBundleEncoder
    )

    // ─── ref count + label ────────────────────────────────────
    @lib pub fn wgpuRenderBundleEncoderSetLabel(
        encoder: WGPURenderBundleEncoder, label: WGPUStringView
    )
    @lib pub fn wgpuRenderBundleEncoderAddRef(encoder: WGPURenderBundleEncoder)
    @lib pub fn wgpuRenderBundleEncoderRelease(encoder: WGPURenderBundleEncoder)
    @lib pub fn wgpuRenderBundleAddRef(bundle: WGPURenderBundle)
    @lib pub fn wgpuRenderBundleRelease(bundle: WGPURenderBundle)

    // ─── render-pass execution ────────────────────────────────
    //
    // Replay one-or-more bundles into a live render pass.
    // `bundles` is a `WGPURenderBundle const*` (array). Pass a
    // single-element `WGPURenderBundle[]` for the common case.
    @lib pub fn wgpuRenderPassEncoderExecuteBundles(
        encoder: WGPURenderPassEncoder,
        bundleCount: u64,
        bundles: *WGPURenderBundle
    )
}
