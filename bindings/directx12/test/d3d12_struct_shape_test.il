/// d3d12 — construct the new struct types and read fields back.
///
/// Pure shape check: build every binding struct the second-pass
/// expansion added, verify field reads return the values we wrote.
/// No DLL calls. Catches accidental field-name / field-type drift
/// between the binding and the SDK without needing a real GPU.

use test
use windows { HANDLE }
use directx12 {
    D3D12_RECT, D3D12_BOX, D3D12_RANGE,
    D3D12_DESCRIPTOR_RANGE, D3D12_DESCRIPTOR_RANGE1,
    D3D12_ROOT_DESCRIPTOR_TABLE, D3D12_ROOT_DESCRIPTOR_TABLE1,
    D3D12_ROOT_CONSTANTS, D3D12_ROOT_DESCRIPTOR, D3D12_ROOT_DESCRIPTOR1,
    D3D12_ROOT_PARAMETER, D3D12_ROOT_PARAMETER1,
    D3D12_STATIC_SAMPLER_DESC, D3D12_ROOT_SIGNATURE_DESC1,
    D3D12_VERSIONED_ROOT_SIGNATURE_DESC,
    D3D12_CONSTANT_BUFFER_VIEW_DESC,
    D3D12_SHADER_RESOURCE_VIEW_DESC,
    D3D12_UNORDERED_ACCESS_VIEW_DESC,
    D3D12_RENDER_TARGET_VIEW_DESC,
    D3D12_DEPTH_STENCIL_VIEW_DESC,
    D3D12_SAMPLER_DESC,
    D3D12_COMPUTE_PIPELINE_STATE_DESC,
    D3D12_CACHED_PIPELINE_STATE, D3D12_SHADER_BYTECODE,
    D3D12_QUERY_HEAP_DESC, D3D12_QUERY_DATA_PIPELINE_STATISTICS,
    D3D12_SUBRESOURCE_FOOTPRINT, D3D12_PLACED_SUBRESOURCE_FOOTPRINT,
    D3D12_TEXTURE_COPY_LOCATION, D3D12_DISCARD_REGION,
    D3D12_TILED_RESOURCE_COORDINATE,
    D3D12_SUBRESOURCE_DATA,
    D3D12_GLOBAL_BARRIER, D3D12_BUFFER_BARRIER, D3D12_TEXTURE_BARRIER,
    D3D12_BARRIER_SUBRESOURCE_RANGE, D3D12_BARRIER_GROUP,
    D3D12_RENDER_PASS_BEGINNING_ACCESS, D3D12_RENDER_PASS_ENDING_ACCESS,
    D3D12_RENDER_PASS_RENDER_TARGET_DESC,
    D3D12_RENDER_PASS_DEPTH_STENCIL_DESC,
    D3D12_INDIRECT_ARGUMENT_DESC, D3D12_COMMAND_SIGNATURE_DESC,
    D3D12_DRAW_ARGUMENTS, D3D12_DRAW_INDEXED_ARGUMENTS,
    D3D12_DISPATCH_ARGUMENTS,
    D3D12_STATE_SUBOBJECT, D3D12_STATE_OBJECT_DESC,
    D3D12_EXPORT_DESC, D3D12_DXIL_LIBRARY_DESC,
    D3D12_HIT_GROUP_DESC,
    D3D12_RAYTRACING_SHADER_CONFIG, D3D12_RAYTRACING_PIPELINE_CONFIG,
    D3D12_RAYTRACING_PIPELINE_CONFIG1,
    D3D12_GPU_VIRTUAL_ADDRESS_AND_STRIDE,
    D3D12_GPU_VIRTUAL_ADDRESS_RANGE,
    D3D12_GPU_VIRTUAL_ADDRESS_RANGE_AND_STRIDE,
    D3D12_RAYTRACING_AABB,
    D3D12_RAYTRACING_GEOMETRY_TRIANGLES_DESC,
    D3D12_RAYTRACING_GEOMETRY_AABBS_DESC,
    D3D12_RAYTRACING_GEOMETRY_DESC,
    D3D12_RAYTRACING_INSTANCE_DESC,
    D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_INPUTS,
    D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_DESC,
    D3D12_RAYTRACING_ACCELERATION_STRUCTURE_PREBUILD_INFO,
    D3D12_DISPATCH_RAYS_DESC,
    D3D12_PROTECTED_RESOURCE_SESSION_DESC,
    D3D12_SAMPLE_POSITION,
    D3D12_FEATURE_DATA_D3D12_OPTIONS,
    D3D12_FEATURE_DATA_D3D12_OPTIONS5,
    D3D12_FEATURE_DATA_SHADER_MODEL,
    D3D12_FEATURE_DATA_ROOT_SIGNATURE,
    D3D12RootParameterType, D3D12DescriptorRangeType,
    D3D12ShaderVisibility, D3D12RootDescriptorFlags,
    D3D12DescriptorRangeFlags,
    D3D12StaticBorderColor, D3D12Filter, D3D12TextureAddressMode,
    D3D12ComparisonFunc, D3D12RootSignatureFlags, D3D12RootSignatureVersion,
    D3D12IndirectArgumentType, D3D12StateSubobjectType, D3D12StateObjectType,
    D3D12HitGroupType, D3D12QueryHeapType, D3D12PipelineStateFlags,
    D3D12RaytracingGeometryType, D3D12RaytracingGeometryFlags,
    D3D12RaytracingAccelerationStructureType,
    D3D12RaytracingAccelerationStructureBuildFlags,
    D3D12ElementsLayout,
    D3D12BarrierSync, D3D12BarrierAccess, D3D12BarrierLayout,
    D3D12BarrierType, D3D12TextureBarrierFlags,
    D3D12RenderPassBeginningAccessType, D3D12RenderPassEndingAccessType,
    D3D12TextureCopyType,
    ID3D12Resource,
}

@extern(C) {
    pub fn run() {
        // ── Descriptor ranges (1.0 + 1.1) ─────────────────────────
        let r0 = D3D12_DESCRIPTOR_RANGE {
            RangeType:           D3D12DescriptorRangeType.cbv as u32,
            NumDescriptors:      4,
            BaseShaderRegister:  0,
            RegisterSpace:       0,
            OffsetInDescriptorsFromTableStart: 0xFFFFFFFF,
            _pad0: 0,
        }
        test.expect(r0.NumDescriptors as i64, 4)
        test.expect(r0.RangeType as i64, 2)
        test.expect(r0.OffsetInDescriptorsFromTableStart as i64, 0xFFFFFFFF)

        let r1 = D3D12_DESCRIPTOR_RANGE1 {
            RangeType:           D3D12DescriptorRangeType.uav as u32,
            NumDescriptors:      8,
            BaseShaderRegister:  1,
            RegisterSpace:       0,
            Flags:               D3D12DescriptorRangeFlags.dataStatic as u32,
            OffsetInDescriptorsFromTableStart: 0,
        }
        test.expect(r1.NumDescriptors as i64, 8)
        test.expect(r1.Flags as i64, 0x8)

        // ── Root descriptor table / constants / descriptor ────────
        let dt = D3D12_ROOT_DESCRIPTOR_TABLE { NumDescriptorRanges: 1, _pad0: 0, pDescriptorRanges: 0 as *const D3D12_DESCRIPTOR_RANGE }
        test.expect(dt.NumDescriptorRanges as i64, 1)

        let rc = D3D12_ROOT_CONSTANTS {
            ShaderRegister: 0, RegisterSpace: 0, Num32BitValues: 16, _pad0: 0,
        }
        test.expect(rc.Num32BitValues as i64, 16)

        let rd = D3D12_ROOT_DESCRIPTOR { ShaderRegister: 2, RegisterSpace: 1 }
        test.expect(rd.ShaderRegister as i64, 2)

        let rd1 = D3D12_ROOT_DESCRIPTOR1 {
            ShaderRegister: 3, RegisterSpace: 0,
            Flags: D3D12RootDescriptorFlags.dataVolatile as u32,
            _pad0: 0,
        }
        test.expect(rd1.Flags as i64, 0x2)

        // ── Root parameter (union payload flattened) ──────────────
        let rp = D3D12_ROOT_PARAMETER {
            ParameterType: D3D12RootParameterType.constants32Bit as u32,
            _pad0: 0,
            u0: 0, u1: 0, u2: 0,
            ShaderVisibility: D3D12ShaderVisibility.vertex as u32,
            _pad1: 0,
        }
        test.expect(rp.ParameterType as i64, 1)
        test.expect(rp.ShaderVisibility as i64, 1)

        let rp1 = D3D12_ROOT_PARAMETER1 {
            ParameterType: D3D12RootParameterType.cbv as u32,
            _pad0: 0,
            u0: 0, u1: 0, u2: 0,
            ShaderVisibility: D3D12ShaderVisibility.pixel as u32,
            _pad1: 0,
        }
        test.expect(rp1.ParameterType as i64, 2)
        test.expect(rp1.ShaderVisibility as i64, 5)

        // ── Static sampler + 1.1 root sig + versioned wrapper ─────
        let s = D3D12_STATIC_SAMPLER_DESC {
            Filter:           D3D12Filter.anisotropic as u32,
            AddressU:         D3D12TextureAddressMode.wrap as u32,
            AddressV:         D3D12TextureAddressMode.wrap as u32,
            AddressW:         D3D12TextureAddressMode.wrap as u32,
            MipLODBias:       0.0_f32,
            MaxAnisotropy:    16,
            ComparisonFunc:   D3D12ComparisonFunc.never as u32,
            BorderColor:      D3D12StaticBorderColor.opaqueWhite as u32,
            MinLOD:           0.0_f32,
            MaxLOD:           1024.0_f32,
            ShaderRegister:   0, RegisterSpace: 0,
            ShaderVisibility: D3D12ShaderVisibility.pixel as u32,
            _pad0:            0,
        }
        test.expect(s.MaxAnisotropy as i64, 16)
        test.expect(s.BorderColor   as i64, 2)
        test.expectF64(s.MaxLOD as f64, 1024.0)

        let rsd1 = D3D12_ROOT_SIGNATURE_DESC1 {
            NumParameters: 1, _pad0: 0,
            pParameters:   0 as *const D3D12_ROOT_PARAMETER1,
            NumStaticSamplers: 0, _pad1: 0,
            pStaticSamplers:   0 as *const D3D12_STATIC_SAMPLER_DESC,
            Flags: D3D12RootSignatureFlags.allowInputAssemblerInputLayout as u32,
            _pad2: 0,
        }
        test.expect(rsd1.NumParameters as i64, 1)
        test.expect(rsd1.Flags as i64, 1)

        let v = D3D12_VERSIONED_ROOT_SIGNATURE_DESC {
            Version: D3D12RootSignatureVersion.version_1_1 as u32,
            _pad0: 0,
            u0: 0, u1: 0, u2: 0, u3: 0, u4: 0, u5: 0,
        }
        test.expect(v.Version as i64, 2)

        // ── View descriptors ──────────────────────────────────────
        let cbv = D3D12_CONSTANT_BUFFER_VIEW_DESC {
            BufferLocation: 0x10000, SizeInBytes: 256, _pad0: 0,
        }
        test.expect(cbv.BufferLocation as i64, 0x10000)
        test.expect(cbv.SizeInBytes    as i64, 256)

        let srv = D3D12_SHADER_RESOURCE_VIEW_DESC {
            Format: 28,           // DXGI_FORMAT_R8G8B8A8_UNORM
            ViewDimension: 4,     // D3D12SrvDimension.texture2d
            Shader4ComponentMapping: 0x1688,
            _pad0: 0, u0: 0, u1: 0,
        }
        test.expect(srv.Format as i64, 28)
        test.expect(srv.Shader4ComponentMapping as i64, 0x1688)

        let uav = D3D12_UNORDERED_ACCESS_VIEW_DESC {
            Format: 42,           // R32_UINT
            ViewDimension: 1,     // buffer
            u0: 0, u1: 0,
        }
        test.expect(uav.Format as i64, 42)

        let rtv = D3D12_RENDER_TARGET_VIEW_DESC {
            Format: 28, ViewDimension: 4, u0: 0, u1: 0,
        }
        test.expect(rtv.ViewDimension as i64, 4)

        let dsv = D3D12_DEPTH_STENCIL_VIEW_DESC {
            Format: 40,           // D32_FLOAT
            ViewDimension: 3,     // texture2d
            Flags: 0, _pad0: 0, u0: 0,
        }
        test.expect(dsv.Format as i64, 40)

        // ── Sampler (heap-resident) ───────────────────────────────
        let samp = D3D12_SAMPLER_DESC {
            Filter:         D3D12Filter.minMagMipLinear as u32,
            AddressU:       D3D12TextureAddressMode.clamp as u32,
            AddressV:       D3D12TextureAddressMode.clamp as u32,
            AddressW:       D3D12TextureAddressMode.clamp as u32,
            MipLODBias:     0.0_f32,
            MaxAnisotropy:  0,
            ComparisonFunc: D3D12ComparisonFunc.never as u32,
            BorderColor:    [0.0_f32, 0.0_f32, 0.0_f32, 0.0_f32],
            MinLOD:         0.0_f32,
            MaxLOD:         3.4e38_f32,
        }
        test.expect(samp.AddressU as i64, 3)
        test.expectF64(samp.BorderColor[0] as f64, 0.0)

        // ── Compute PSO desc ──────────────────────────────────────
        let cpso = D3D12_COMPUTE_PIPELINE_STATE_DESC {
            pRootSignature: 0,
            CS:             D3D12_SHADER_BYTECODE { pShaderBytecode: 0 as *const void, BytecodeLength: 0 },
            NodeMask:       0,
            _pad0:          0,
            CachedPSO:      D3D12_CACHED_PIPELINE_STATE { pCachedBlob: 0 as *const void, CachedBlobSizeInBytes: 0 },
            Flags:          D3D12PipelineStateFlags.none as u32,
            _pad1:          0,
        }
        test.expect(cpso.NodeMask as i64, 0)

        // ── Query heap + pipeline statistics ──────────────────────
        let qhd = D3D12_QUERY_HEAP_DESC {
            Type:     D3D12QueryHeapType.timestamp as u32,
            Count:    32,
            NodeMask: 0,
            _pad0:    0,
        }
        test.expect(qhd.Count as i64, 32)

        let pst = D3D12_QUERY_DATA_PIPELINE_STATISTICS {
            IAVertices: 0, IAPrimitives: 0,
            VSInvocations: 0, GSInvocations: 0, GSPrimitives: 0,
            CInvocations: 0, CPrimitives: 0, PSInvocations: 0,
            HSInvocations: 0, DSInvocations: 0, CSInvocations: 0,
        }
        test.expect(pst.IAVertices as i64, 0)

        // ── Texture copy / footprint ──────────────────────────────
        let sf = D3D12_SUBRESOURCE_FOOTPRINT {
            Format: 28, Width: 256, Height: 256, Depth: 1, RowPitch: 1024, _pad0: 0,
        }
        let psf = D3D12_PLACED_SUBRESOURCE_FOOTPRINT { Offset: 0, Footprint: sf }
        test.expect(psf.Footprint.Width as i64, 256)
        test.expect(psf.Footprint.RowPitch as i64, 1024)

        let tcl = D3D12_TEXTURE_COPY_LOCATION {
            pResource: 0 as *void as *ID3D12Resource,
            Type:      D3D12TextureCopyType.placedFootprint as u32,
            _pad0:     0,
            u0: 0, u1: 0, u2: 0, u3: 0, u4: 0, u5: 0, u6: 0, u7: 0,
        }
        test.expect(tcl.Type as i64, 1)

        let dr = D3D12_DISCARD_REGION {
            NumRects: 0, _pad0: 0,
            pRects: 0 as *const D3D12_RECT,
            FirstSubresource: 0, NumSubresources: 1,
        }
        test.expect(dr.NumSubresources as i64, 1)

        let trc = D3D12_TILED_RESOURCE_COORDINATE {
            X: 4, Y: 8, Z: 0, Subresource: 0,
        }
        test.expect(trc.X as i64, 4)
        test.expect(trc.Y as i64, 8)

        let sd = D3D12_SUBRESOURCE_DATA {
            pData: 0 as *const void, RowPitch: 1024, SlicePitch: 0,
        }
        test.expect(sd.RowPitch, 1024)

        // ── Enhanced barriers ─────────────────────────────────────
        let gb = D3D12_GLOBAL_BARRIER {
            SyncBefore:   D3D12BarrierSync.draw as u32,
            SyncAfter:    D3D12BarrierSync.pixelShading as u32,
            AccessBefore: D3D12BarrierAccess.renderTarget as u32,
            AccessAfter:  D3D12BarrierAccess.shaderResource as u32,
        }
        test.expect(gb.AccessBefore as i64, 0x8)
        test.expect(gb.AccessAfter  as i64, 0x80)

        let bb = D3D12_BUFFER_BARRIER {
            SyncBefore: 0, SyncAfter: 0, AccessBefore: 0, AccessAfter: 0,
            pResource: 0 as *void as *ID3D12Resource,
            Offset: 0, Size: 1024,
        }
        test.expect(bb.Size as i64, 1024)

        let bsr = D3D12_BARRIER_SUBRESOURCE_RANGE {
            IndexOrFirstMipLevel: 0, NumMipLevels: 1,
            FirstArraySlice: 0, NumArraySlices: 1,
            FirstPlane: 0, NumPlanes: 1,
        }
        let tb = D3D12_TEXTURE_BARRIER {
            SyncBefore: 0, SyncAfter: 0, AccessBefore: 0, AccessAfter: 0,
            LayoutBefore: D3D12BarrierLayout.renderTarget as u32,
            LayoutAfter:  D3D12BarrierLayout.shaderResource as u32,
            pResource:    0 as *void as *ID3D12Resource,
            Subresources: bsr,
            Flags:        D3D12TextureBarrierFlags.discard as u32,
            _pad0:        0,
        }
        test.expect(tb.LayoutBefore as i64, 2)
        test.expect(tb.LayoutAfter  as i64, 6)
        test.expect(tb.Subresources.NumMipLevels as i64, 1)

        let bg = D3D12_BARRIER_GROUP {
            Type: D3D12BarrierType.texture as u32,
            NumBarriers: 1,
            pBarriers: 0 as *const void,
        }
        test.expect(bg.Type as i64, 1)

        // ── Render-pass access blocks ─────────────────────────────
        let rba = D3D12_RENDER_PASS_BEGINNING_ACCESS {
            Type: D3D12RenderPassBeginningAccessType.clear as u32,
            _pad0: 0,
            u0: 0, u1: 0, u2: 0,
        }
        test.expect(rba.Type as i64, 2)

        let rea = D3D12_RENDER_PASS_ENDING_ACCESS {
            Type: D3D12RenderPassEndingAccessType.preserve as u32,
            _pad0: 0,
            u0: 0, u1: 0, u2: 0, u3: 0, u4: 0, u5: 0,
        }
        test.expect(rea.Type as i64, 1)

        let rprt = D3D12_RENDER_PASS_RENDER_TARGET_DESC {
            cpuDescriptorPtr: 0,
            BeginningAccess:  rba,
            EndingAccess:     rea,
        }
        test.expect(rprt.BeginningAccess.Type as i64, 2)

        let rpds = D3D12_RENDER_PASS_DEPTH_STENCIL_DESC {
            cpuDescriptorPtr:       0,
            DepthBeginningAccess:   rba,
            StencilBeginningAccess: rba,
            DepthEndingAccess:      rea,
            StencilEndingAccess:    rea,
        }
        test.expect(rpds.DepthBeginningAccess.Type as i64, 2)

        // ── Indirect args ─────────────────────────────────────────
        let ia = D3D12_INDIRECT_ARGUMENT_DESC {
            Type: D3D12IndirectArgumentType.dispatch as u32,
            _pad0: 0, u0: 0, u1: 0,
        }
        test.expect(ia.Type as i64, 2)

        let csd = D3D12_COMMAND_SIGNATURE_DESC {
            ByteStride: 12,
            NumArgumentDescs: 1,
            pArgumentDescs: &ia,
            NodeMask: 0,
            _pad0: 0,
        }
        test.expect(csd.ByteStride as i64, 12)

        let da = D3D12_DRAW_ARGUMENTS {
            VertexCountPerInstance: 3, InstanceCount: 1,
            StartVertexLocation: 0, StartInstanceLocation: 0,
        }
        test.expect(da.VertexCountPerInstance as i64, 3)

        let dia = D3D12_DRAW_INDEXED_ARGUMENTS {
            IndexCountPerInstance: 36, InstanceCount: 1,
            StartIndexLocation: 0, BaseVertexLocation: 0,
            StartInstanceLocation: 0,
        }
        test.expect(dia.IndexCountPerInstance as i64, 36)

        let dpa = D3D12_DISPATCH_ARGUMENTS {
            ThreadGroupCountX: 8, ThreadGroupCountY: 4, ThreadGroupCountZ: 1,
        }
        test.expect(dpa.ThreadGroupCountX as i64, 8)

        // ── State-object subobjects (DXR) ─────────────────────────
        let so = D3D12_STATE_SUBOBJECT {
            Type: D3D12StateSubobjectType.dxilLibrary as u32,
            _pad0: 0,
            pDesc: 0 as *const void,
        }
        test.expect(so.Type as i64, 5)

        let sod = D3D12_STATE_OBJECT_DESC {
            Type: D3D12StateObjectType.raytracingPipeline as u32,
            NumSubobjects: 1,
            pSubobjects: &so,
        }
        test.expect(sod.NumSubobjects as i64, 1)

        let ed = D3D12_EXPORT_DESC {
            Name: 0 as *const u16, ExportToRename: 0 as *const u16,
            Flags: 0, _pad0: 0,
        }
        let dxl = D3D12_DXIL_LIBRARY_DESC {
            DXILLibrary: D3D12_SHADER_BYTECODE { pShaderBytecode: 0 as *const void, BytecodeLength: 0 },
            NumExports: 1, _pad0: 0,
            pExports: &ed,
        }
        test.expect(dxl.NumExports as i64, 1)

        let hg = D3D12_HIT_GROUP_DESC {
            HitGroupExport: 0 as *const u16,
            Type: D3D12HitGroupType.triangles as u32,
            _pad0: 0,
            AnyHitShaderImport: 0 as *const u16,
            ClosestHitShaderImport: 0 as *const u16,
            IntersectionShaderImport: 0 as *const u16,
        }
        test.expect(hg.Type as i64, 0)

        let rsc = D3D12_RAYTRACING_SHADER_CONFIG {
            MaxPayloadSizeInBytes: 16, MaxAttributeSizeInBytes: 8,
        }
        test.expect(rsc.MaxPayloadSizeInBytes as i64, 16)

        let rpc = D3D12_RAYTRACING_PIPELINE_CONFIG { MaxTraceRecursionDepth: 2 }
        test.expect(rpc.MaxTraceRecursionDepth as i64, 2)

        let rpc1 = D3D12_RAYTRACING_PIPELINE_CONFIG1 {
            MaxTraceRecursionDepth: 3,
            Flags: 0x100,    // skipTriangles
        }
        test.expect(rpc1.Flags as i64, 0x100)

        // ── DXR geometry / instance / build inputs ────────────────
        let gvas = D3D12_GPU_VIRTUAL_ADDRESS_AND_STRIDE {
            StartAddress: 0x10000, StrideInBytes: 12,
        }
        test.expect(gvas.StrideInBytes as i64, 12)

        let gvr = D3D12_GPU_VIRTUAL_ADDRESS_RANGE {
            StartAddress: 0x20000, SizeInBytes: 0x1000,
        }
        test.expect(gvr.SizeInBytes as i64, 0x1000)

        let gvrs = D3D12_GPU_VIRTUAL_ADDRESS_RANGE_AND_STRIDE {
            StartAddress: 0x30000, SizeInBytes: 0x2000, StrideInBytes: 32,
        }
        test.expect(gvrs.StrideInBytes as i64, 32)

        let aabb = D3D12_RAYTRACING_AABB {
            MinX: -1.0_f32, MinY: -1.0_f32, MinZ: -1.0_f32,
            MaxX:  1.0_f32, MaxY:  1.0_f32, MaxZ:  1.0_f32,
        }
        test.expectF64(aabb.MinX as f64, -1.0)
        test.expectF64(aabb.MaxX as f64,  1.0)

        let tri = D3D12_RAYTRACING_GEOMETRY_TRIANGLES_DESC {
            Transform3x4: 0, IndexFormat: 57, VertexFormat: 6,
            IndexCount: 36, VertexCount: 8,
            IndexBuffer: 0x40000,
            VertexBuffer: gvas,
        }
        test.expect(tri.IndexCount as i64, 36)
        test.expect(tri.VertexBuffer.StrideInBytes as i64, 12)

        let aabbs = D3D12_RAYTRACING_GEOMETRY_AABBS_DESC { AABBCount: 2, AABBs: gvas }
        test.expect(aabbs.AABBCount as i64, 2)

        let gd = D3D12_RAYTRACING_GEOMETRY_DESC {
            Type:  D3D12RaytracingGeometryType.triangles as u32,
            Flags: D3D12RaytracingGeometryFlags.opaque as u32,
            u0: 0, u1: 0, u2: 0, u3: 0, u4: 0, u5: 0, u6: 0,
        }
        test.expect(gd.Flags as i64, 1)

        let inst = D3D12_RAYTRACING_INSTANCE_DESC {
            Transform0_0: 1.0_f32, Transform0_1: 0.0_f32, Transform0_2: 0.0_f32, Transform0_3: 0.0_f32,
            Transform1_0: 0.0_f32, Transform1_1: 1.0_f32, Transform1_2: 0.0_f32, Transform1_3: 0.0_f32,
            Transform2_0: 0.0_f32, Transform2_1: 0.0_f32, Transform2_2: 1.0_f32, Transform2_3: 0.0_f32,
            InstanceIDAndMask:            0xFF000000,
            InstanceContributionAndFlags: 0x00000001,
            AccelerationStructure: 0x50000,
        }
        test.expectF64(inst.Transform1_1 as f64, 1.0)
        test.expect(inst.InstanceIDAndMask as i64, 0xFF000000)

        let inputs = D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_INPUTS {
            Type:        D3D12RaytracingAccelerationStructureType.bottomLevel as u32,
            Flags:       D3D12RaytracingAccelerationStructureBuildFlags.preferFastTrace as u32,
            NumDescs:    1,
            DescsLayout: D3D12ElementsLayout.array as u32,
            u0:          0,
        }
        test.expect(inputs.Flags as i64, 4)

        let bdesc = D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_DESC {
            DestAccelerationStructureData:   0x60000,
            Inputs:                          inputs,
            SourceAccelerationStructureData: 0,
            ScratchAccelerationStructureData: 0x70000,
        }
        test.expect(bdesc.DestAccelerationStructureData as i64, 0x60000)

        let pre = D3D12_RAYTRACING_ACCELERATION_STRUCTURE_PREBUILD_INFO {
            ResultDataMaxSizeInBytes: 0, ScratchDataSizeInBytes: 0,
            UpdateScratchDataSizeInBytes: 0,
        }
        test.expect(pre.ResultDataMaxSizeInBytes as i64, 0)

        let drd = D3D12_DISPATCH_RAYS_DESC {
            RayGenerationShaderRecord: gvr,
            MissShaderTable:           gvrs,
            HitGroupTable:             gvrs,
            CallableShaderTable:       gvrs,
            Width:  1920, Height: 1080, Depth: 1, _pad0: 0,
        }
        test.expect(drd.Width as i64, 1920)
        test.expect(drd.Height as i64, 1080)

        // ── Protected session / sample position ───────────────────
        let prs = D3D12_PROTECTED_RESOURCE_SESSION_DESC { NodeMask: 0, Flags: 0 }
        test.expect(prs.NodeMask as i64, 0)

        let sp = D3D12_SAMPLE_POSITION { X: 4 as i8, Y: -4 as i8 }
        test.expect(sp.X as i64, 4)
        test.expect(sp.Y as i64, -4)

        // ── Feature payloads ──────────────────────────────────────
        let frs = D3D12_FEATURE_DATA_ROOT_SIGNATURE {
            HighestVersion: D3D12RootSignatureVersion.version_1_1 as u32,
        }
        test.expect(frs.HighestVersion as i64, 2)

        let fsm = D3D12_FEATURE_DATA_SHADER_MODEL { HighestShaderModel: 0x66 }   // SM 6.6
        test.expect(fsm.HighestShaderModel as i64, 0x66)
    }
}

run()
