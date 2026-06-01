/// `WGPUQuerySet` — GPU-side counter buffer. Two kinds:
///   * `Occlusion` (type=1): one u64 sample per query slot,
///     written by `wgpuRenderPassEncoderBeginOcclusionQuery`
///     / `EndOcclusionQuery` (not yet bound).
///   * `Timestamp` (type=2): one u64 sample per query slot,
///     written by `wgpuCommandEncoderWriteTimestamp` or by the
///     `beginningOfPassWriteIndex` / `endOfPassWriteIndex`
///     slots on `WGPUPassTimestampWrites`.
///
/// Samples are GPU-resident; to read them on the CPU, copy via
/// `wgpuCommandEncoderResolveQuerySet` into a CopyDst buffer,
/// then MapAsync that buffer back.

use core { WGPUStringView, WGPUDevice }
use buffer { WGPUBuffer }
use command { WGPUCommandEncoder }

@extern(C, "wgpu_native") {
    @handle pub struct WGPUQuerySet {}

    // WGPUQueryType: Occlusion=1, Timestamp=2.
    pub struct WGPUQuerySetDescriptor {
        nextInChain: *void
        label:       WGPUStringView
        type:        u32           // WGPUQueryType
        count:       u32
    }

    /// Pass-level timestamp writes — chain into
    /// `WGPURenderPassDescriptor.timestampWrites` /
    /// `WGPUComputePassDescriptor.timestampWrites` (cast as
    /// `(&writes) as *void`). Use
    /// `WGPU_QUERY_SET_INDEX_UNDEFINED` (= 0xFFFFFFFF) for either
    /// index to skip that endpoint.
    pub struct WGPUPassTimestampWrites {
        nextInChain:               *void
        querySet:                  WGPUQuerySet
        beginningOfPassWriteIndex: u32
        endOfPassWriteIndex:       u32
    }

    @lib pub fn wgpuDeviceCreateQuerySet(
        device: WGPUDevice, descriptor: *WGPUQuerySetDescriptor
    ): WGPUQuerySet

    @lib pub fn wgpuQuerySetGetType(querySet: WGPUQuerySet): u32   // WGPUQueryType
    @lib pub fn wgpuQuerySetGetCount(querySet: WGPUQuerySet): u32
    // Eager GPU-resource drop (same shape as BufferDestroy /
    // DeviceDestroy); the wrapper handle stays alive.
    @lib pub fn wgpuQuerySetDestroy(querySet: WGPUQuerySet)
    @lib pub fn wgpuQuerySetSetLabel(querySet: WGPUQuerySet, label: WGPUStringView)
    @lib pub fn wgpuQuerySetAddRef(querySet: WGPUQuerySet)
    @lib pub fn wgpuQuerySetRelease(querySet: WGPUQuerySet)

    // ─── command encoder ops on query sets ────────────────────
    //
    // `WriteTimestamp` records a single GPU timestamp into
    // `querySet[queryIndex]`. `ResolveQuerySet` copies a contiguous
    // run of samples (one u64 per slot) into `destination` at
    // `destinationOffset`; the destination buffer needs
    // `CopySrc | CopyDst` usage and 8-byte alignment.
    @lib pub fn wgpuCommandEncoderWriteTimestamp(
        encoder: WGPUCommandEncoder, querySet: WGPUQuerySet, queryIndex: u32
    )
    @lib pub fn wgpuCommandEncoderResolveQuerySet(
        encoder:           WGPUCommandEncoder,
        querySet:          WGPUQuerySet,
        firstQuery:        u32,
        queryCount:        u32,
        destination:       WGPUBuffer,
        destinationOffset: u64
    )
}
