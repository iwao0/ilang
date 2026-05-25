/// d3d12 — verify selected enum / constant values match the SDK.
///
/// These are pure value checks — no DLL calls — so the test runs
/// unchanged on machines without a D3D12 GPU. The values listed are
/// the ones we hand back to the runtime in struct literals, so a
/// drift between the binding and the SDK headers would silently
/// mis-render or mis-link.

use std.test as test// HANDLE is referenced by ID3D12Fence::SetEventOnCompletion deep in
// the binding; ilang's import resolver wants it in scope here too.
use windows { HANDLE }
use directx12 {
    D3D12HeapType, D3D12CpuPageProperty, D3D12MemoryPool,
    D3D12ResourceDimension, D3D12TextureLayout, D3D12CommandListType,
    D3D12DescriptorHeapType, D3D12DescriptorHeapFlags,
    D3D12ResourceStates, D3D12ResourceBarrierType,
    D3D12FillMode, D3D12CullMode, D3D12ComparisonFunc,
    D3D12RootParameterType, D3D12DescriptorRangeType,
    D3D12ShaderVisibility, D3D12RootDescriptorFlags,
    D3D12StaticBorderColor,
    D3D12QueryHeapType, D3D12QueryType,
    D3D12RtvDimension, D3D12DsvDimension, D3D12SrvDimension, D3D12UavDimension,
    D3D12TextureCopyType,
    D3D12Feature, D3D12RaytracingTier, D3D12VariableShadingRateTier,
    D3D12MeshShaderTier,
    D3D12BarrierLayout, D3D12BarrierType, D3D12TextureBarrierFlags,
    D3D12RenderPassBeginningAccessType, D3D12RenderPassEndingAccessType,
    D3D12ResolveMode,
    D3D12IndirectArgumentType,
    D3D12StateSubobjectType, D3D12HitGroupType,
    D3D12PipelineStateSubobjectType,
    D3D12ResourceBindingTier, D3D12TiledResourcesTier,
    D3D12ConservativeRasterizationTier, D3D12CrossNodeSharingTier,
    D3D12ResourceHeapTier,
    D3D12ClearFlags,
    D3D12ShadingRate, D3D12ShadingRateCombiner,
    D3D12RaytracingGeometryType, D3D12RaytracingAccelerationStructureType,
    D3D12RaytracingAccelerationStructureCopyMode,
    D3D12RaytracingAccelerationStructurePostbuildInfoType,
    D3D12ElementsLayout,
    D3D12ResidencyPriority,
    D3D12DredEnablement, D3D12DredAllocationType,
    D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING,
    D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES,
    D3D12_APPEND_ALIGNED_ELEMENT,
    D3D12_SHADER_IDENTIFIER_SIZE_IN_BYTES,
    D3D12_RAYTRACING_SHADER_RECORD_BYTE_ALIGNMENT,
    D3D12_RAYTRACING_SHADER_TABLE_BYTE_ALIGNMENT,
}

@extern(C) {
    pub fn run() {
        // ── Heap / memory ──────────────────────────────────────────
        test.expect(D3D12HeapType.default  as i64, 1)
        test.expect(D3D12HeapType.upload   as i64, 2)
        test.expect(D3D12HeapType.readback as i64, 3)
        test.expect(D3D12HeapType.custom   as i64, 4)
        test.expect(D3D12CpuPageProperty.writeBack as i64, 3)
        test.expect(D3D12MemoryPool.l1 as i64, 2)

        // ── Resources / barriers ───────────────────────────────────
        test.expect(D3D12ResourceDimension.buffer    as i64, 1)
        test.expect(D3D12ResourceDimension.texture2d as i64, 3)
        test.expect(D3D12TextureLayout.rowMajor as i64, 1)
        test.expect(D3D12ResourceStates.renderTarget    as i64, 0x4)
        test.expect(D3D12ResourceStates.depthWrite      as i64, 0x10)
        test.expect(D3D12ResourceStates.unorderedAccess as i64, 0x8)
        test.expect(D3D12ResourceStates.present         as i64, 0x0)
        test.expect(D3D12ResourceBarrierType.transition as i64, 0)
        test.expect(D3D12ResourceBarrierType.uav        as i64, 2)

        // ── Command list ───────────────────────────────────────────
        test.expect(D3D12CommandListType.direct  as i64, 0)
        test.expect(D3D12CommandListType.compute as i64, 2)
        test.expect(D3D12CommandListType.copy    as i64, 3)

        // ── Descriptor heap ────────────────────────────────────────
        test.expect(D3D12DescriptorHeapType.cbvSrvUav as i64, 0)
        test.expect(D3D12DescriptorHeapType.sampler   as i64, 1)
        test.expect(D3D12DescriptorHeapType.rtv       as i64, 2)
        test.expect(D3D12DescriptorHeapType.dsv       as i64, 3)
        test.expect(D3D12DescriptorHeapFlags.shaderVisible as i64, 1)

        // ── Raster / depth / blend ─────────────────────────────────
        test.expect(D3D12FillMode.solid     as i64, 3)
        test.expect(D3D12FillMode.wireframe as i64, 2)
        test.expect(D3D12CullMode.back as i64, 3)
        test.expect(D3D12ComparisonFunc.less     as i64, 2)
        test.expect(D3D12ComparisonFunc.always   as i64, 8)
        test.expect(D3D12ComparisonFunc.notEqual as i64, 6)

        // ── Root signatures ────────────────────────────────────────
        test.expect(D3D12RootParameterType.descriptorTable as i64, 0)
        test.expect(D3D12RootParameterType.constants32Bit  as i64, 1)
        test.expect(D3D12RootParameterType.cbv             as i64, 2)
        test.expect(D3D12RootParameterType.srv             as i64, 3)
        test.expect(D3D12RootParameterType.uav             as i64, 4)
        test.expect(D3D12DescriptorRangeType.srv     as i64, 0)
        test.expect(D3D12DescriptorRangeType.uav     as i64, 1)
        test.expect(D3D12DescriptorRangeType.cbv     as i64, 2)
        test.expect(D3D12DescriptorRangeType.sampler as i64, 3)
        test.expect(D3D12ShaderVisibility.all         as i64, 0)
        test.expect(D3D12ShaderVisibility.vertex      as i64, 1)
        test.expect(D3D12ShaderVisibility.pixel       as i64, 5)
        test.expect(D3D12ShaderVisibility.mesh        as i64, 7)
        test.expect(D3D12RootDescriptorFlags.dataStatic                  as i64, 0x8)
        test.expect(D3D12RootDescriptorFlags.dataVolatile                as i64, 0x2)
        test.expect(D3D12StaticBorderColor.opaqueBlack as i64, 1)
        test.expect(D3D12StaticBorderColor.opaqueWhite as i64, 2)

        // ── Queries ────────────────────────────────────────────────
        test.expect(D3D12QueryHeapType.timestamp          as i64, 1)
        test.expect(D3D12QueryHeapType.pipelineStatistics as i64, 2)
        test.expect(D3D12QueryType.binaryOcclusion as i64, 1)
        test.expect(D3D12QueryType.timestamp       as i64, 2)

        // ── View dimensions ────────────────────────────────────────
        test.expect(D3D12RtvDimension.texture2d        as i64, 4)
        test.expect(D3D12DsvDimension.texture2d        as i64, 3)
        test.expect(D3D12SrvDimension.texture2d        as i64, 4)
        test.expect(D3D12SrvDimension.textureCube      as i64, 9)
        test.expect(D3D12SrvDimension.raytracingAccelerationStructure as i64, 11)
        test.expect(D3D12UavDimension.texture3d        as i64, 8)

        // ── Texture copy ───────────────────────────────────────────
        test.expect(D3D12TextureCopyType.subresourceIndex as i64, 0)
        test.expect(D3D12TextureCopyType.placedFootprint  as i64, 1)

        // ── Feature query discriminator ────────────────────────────
        test.expect(D3D12Feature.d3d12Options  as i64, 0)
        test.expect(D3D12Feature.architecture  as i64, 1)
        test.expect(D3D12Feature.featureLevels as i64, 2)
        test.expect(D3D12Feature.shaderModel   as i64, 7)
        test.expect(D3D12Feature.d3d12Options5 as i64, 27)
        test.expect(D3D12Feature.d3d12Options7 as i64, 32)

        // ── Tiers ──────────────────────────────────────────────────
        test.expect(D3D12RaytracingTier.tier_1_1     as i64, 11)
        test.expect(D3D12VariableShadingRateTier.tier_2 as i64, 2)
        test.expect(D3D12MeshShaderTier.tier_1       as i64, 10)
        test.expect(D3D12ResourceBindingTier.tier_3  as i64, 3)
        test.expect(D3D12TiledResourcesTier.tier_4   as i64, 4)
        test.expect(D3D12ConservativeRasterizationTier.tier_3 as i64, 3)
        test.expect(D3D12CrossNodeSharingTier.tier_2 as i64, 3)
        test.expect(D3D12ResourceHeapTier.tier_2     as i64, 2)

        // ── Enhanced barriers (List7) ──────────────────────────────
        test.expect(D3D12BarrierLayout.common              as i64, 0)
        test.expect(D3D12BarrierLayout.renderTarget        as i64, 2)
        test.expect(D3D12BarrierLayout.unorderedAccess     as i64, 3)
        test.expect(D3D12BarrierLayout.depthStencilWrite   as i64, 4)
        test.expect(D3D12BarrierLayout.shaderResource      as i64, 6)
        test.expect(D3D12BarrierLayout.copySource          as i64, 7)
        test.expect(D3D12BarrierLayout.copyDest            as i64, 8)
        test.expect(D3D12BarrierType.global  as i64, 0)
        test.expect(D3D12BarrierType.texture as i64, 1)
        test.expect(D3D12BarrierType.buffer  as i64, 2)
        test.expect(D3D12TextureBarrierFlags.discard as i64, 1)

        // ── Render passes ──────────────────────────────────────────
        test.expect(D3D12RenderPassBeginningAccessType.discard  as i64, 0)
        test.expect(D3D12RenderPassBeginningAccessType.preserve as i64, 1)
        test.expect(D3D12RenderPassBeginningAccessType.clear    as i64, 2)
        test.expect(D3D12RenderPassEndingAccessType.discard  as i64, 0)
        test.expect(D3D12RenderPassEndingAccessType.preserve as i64, 1)
        test.expect(D3D12RenderPassEndingAccessType.resolve  as i64, 2)
        test.expect(D3D12ResolveMode.decompress as i64, 0)
        test.expect(D3D12ResolveMode.average    as i64, 3)

        // ── Indirect args ──────────────────────────────────────────
        test.expect(D3D12IndirectArgumentType.draw         as i64, 0)
        test.expect(D3D12IndirectArgumentType.drawIndexed  as i64, 1)
        test.expect(D3D12IndirectArgumentType.dispatch     as i64, 2)
        test.expect(D3D12IndirectArgumentType.dispatchRays as i64, 9)
        test.expect(D3D12IndirectArgumentType.dispatchMesh as i64, 10)

        // ── State object subobjects (DXR) ──────────────────────────
        test.expect(D3D12StateSubobjectType.globalRootSignature       as i64, 1)
        test.expect(D3D12StateSubobjectType.localRootSignature        as i64, 2)
        test.expect(D3D12StateSubobjectType.dxilLibrary               as i64, 5)
        test.expect(D3D12StateSubobjectType.existingCollection        as i64, 6)
        test.expect(D3D12StateSubobjectType.raytracingShaderConfig    as i64, 9)
        test.expect(D3D12StateSubobjectType.raytracingPipelineConfig  as i64, 10)
        test.expect(D3D12StateSubobjectType.hitGroup                  as i64, 11)
        test.expect(D3D12HitGroupType.triangles           as i64, 0)
        test.expect(D3D12HitGroupType.proceduralPrimitive as i64, 1)

        // ── PSO subobject stream ───────────────────────────────────
        test.expect(D3D12PipelineStateSubobjectType.rootSignature       as i64, 0)
        test.expect(D3D12PipelineStateSubobjectType.cs                  as i64, 6)
        test.expect(D3D12PipelineStateSubobjectType.ms                  as i64, 25)

        // ── Clear / shading rate ──────────────────────────────────
        test.expect(D3D12ClearFlags.depth   as i64, 1)
        test.expect(D3D12ClearFlags.stencil as i64, 2)
        test.expect(D3D12ShadingRate.rate_2x2 as i64, 0x5)
        test.expect(D3D12ShadingRate.rate_4x4 as i64, 0xa)
        test.expect(D3D12ShadingRateCombiner.passthrough as i64, 0)
        test.expect(D3D12ShadingRateCombiner.max         as i64, 3)

        // ── DXR ────────────────────────────────────────────────────
        test.expect(D3D12RaytracingGeometryType.triangles                as i64, 0)
        test.expect(D3D12RaytracingGeometryType.proceduralPrimitiveAabbs as i64, 1)
        test.expect(D3D12RaytracingAccelerationStructureType.topLevel    as i64, 0)
        test.expect(D3D12RaytracingAccelerationStructureType.bottomLevel as i64, 1)
        test.expect(D3D12RaytracingAccelerationStructureCopyMode.clone   as i64, 0)
        test.expect(D3D12RaytracingAccelerationStructureCopyMode.compact as i64, 1)
        test.expect(D3D12RaytracingAccelerationStructurePostbuildInfoType.compactedSize as i64, 0)
        test.expect(D3D12RaytracingAccelerationStructurePostbuildInfoType.currentSize   as i64, 3)
        test.expect(D3D12ElementsLayout.array as i64, 0)
        test.expect(D3D12ElementsLayout.arrayOfPointers as i64, 1)

        // ── Residency / DRED ───────────────────────────────────────
        test.expect(D3D12ResidencyPriority.normal as i64, 0x78000000)
        test.expect(D3D12ResidencyPriority.high   as i64, 0xa0010000)
        test.expect(D3D12DredEnablement.systemControlled as i64, 0)
        test.expect(D3D12DredEnablement.forcedOff        as i64, 1)
        test.expect(D3D12DredEnablement.forcedOn         as i64, 2)
        test.expect(D3D12DredAllocationType.commandQueue as i64, 19)
        test.expect(D3D12DredAllocationType.resource     as i64, 34)

        // ── Constants ──────────────────────────────────────────────
        test.expect(D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING       as i64, 0x1688)
        test.expect(D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES        as i64, 0xFFFFFFFF)
        test.expect(D3D12_APPEND_ALIGNED_ELEMENT                   as i64, 0xFFFFFFFF)
        test.expect(D3D12_SHADER_IDENTIFIER_SIZE_IN_BYTES          as i64, 32)
        test.expect(D3D12_RAYTRACING_SHADER_RECORD_BYTE_ALIGNMENT  as i64, 32)
        test.expect(D3D12_RAYTRACING_SHADER_TABLE_BYTE_ALIGNMENT   as i64, 64)
    }
}

run()
