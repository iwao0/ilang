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
use std.ffi { cstrFromString }

pub enum QueryType: u32 {
    occlusion = 0x01
    timestamp = 0x02
}

pub enum PipelineStatisticName: u32 {
    vertexShaderInvocations   = 0x00
    clipperInvocations        = 0x01
    clipperPrimitivesOut      = 0x02
    fragmentShaderInvocations = 0x03
    computeShaderInvocations  = 0x04
}

pub enum NativeQueryType: u32 {
    pipelineStatistics = 0x00030000
}

@extern(C, "wgpu_native") {
    @handle pub struct WGPUQuerySet {}

    // WGPUQueryType: Occlusion=1, Timestamp=2.
    pub struct WGPUQuerySetDescriptor {
        nextInChain: *void
        label:       WGPUStringView
        type:        QueryType
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

    // ─── friendly wrapper ─────────────────────────────────────

    /// Wraps `WGPUQuerySet`. Built via `Device.createQuerySet`.
    /// `release()` drops the host-side refcount; `destroy()`
    /// drops the GPU-side storage eagerly.
    pub class QuerySet {
        pub handle: i64

        pub init(handle: WGPUQuerySet) {
            this.handle = handle as i64
        }
        pub release() {
            if this.handle != 0 {
                wgpuQuerySetRelease(this.handle as WGPUQuerySet)
                this.handle = 0
            }
        }
        pub destroy() {
            if this.handle != 0 {
                wgpuQuerySetDestroy(this.handle as WGPUQuerySet)
            }
        }
        pub setLabel(label: string) {
            wgpuQuerySetSetLabel(this.handle as WGPUQuerySet, WGPUStringView {
                data:   cstrFromString(label) as *void,
                length: 0xFFFFFFFFFFFFFFFF as u64,
            })
        }
        pub getType(): QueryType {
            wgpuQuerySetGetType(this.handle as WGPUQuerySet) as QueryType
        }
        pub getCount(): i64 {
            wgpuQuerySetGetCount(this.handle as WGPUQuerySet) as i64
        }
    }

    /// Configuration for `Device.createQuerySet`.
    pub class QuerySetOptions {
        pub label: string
        pub type:  QueryType
        pub count: i64

        pub init(type: QueryType, count: i64) {
            this.label = ""
            this.type  = type
            this.count = count
        }
    }
}

// ─── pass-encoder timestamp + pipeline-statistics queries ─────
//
// `WriteTimestamp` is also exposed inside the pass encoders so
// you can sample timing between specific draw / dispatch calls
// without ending the pass. Pipeline statistics queries record
// per-stage counters (vertex invocations, primitive count, etc.)
// over the bracketed region.

use command { WGPURenderPassEncoder, WGPUComputePassEncoder }

@extern(C, "wgpu_native") {
    @lib pub fn wgpuRenderPassEncoderWriteTimestamp(
        encoder: WGPURenderPassEncoder,
        querySet: WGPUQuerySet, queryIndex: u32
    )
    @lib pub fn wgpuComputePassEncoderWriteTimestamp(
        encoder: WGPUComputePassEncoder,
        querySet: WGPUQuerySet, queryIndex: u32
    )

    @lib pub fn wgpuRenderPassEncoderBeginPipelineStatisticsQuery(
        encoder: WGPURenderPassEncoder,
        querySet: WGPUQuerySet, queryIndex: u32
    )
    @lib pub fn wgpuRenderPassEncoderEndPipelineStatisticsQuery(
        encoder: WGPURenderPassEncoder
    )
    @lib pub fn wgpuComputePassEncoderBeginPipelineStatisticsQuery(
        encoder: WGPUComputePassEncoder,
        querySet: WGPUQuerySet, queryIndex: u32
    )
    @lib pub fn wgpuComputePassEncoderEndPipelineStatisticsQuery(
        encoder: WGPUComputePassEncoder
    )
}
