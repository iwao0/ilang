/// d3d12 — round-trip a tiny root signature through the four
/// serialize / deserialize flat entry points.
///
/// Covers:
///   * D3D12SerializeRootSignature                  (1.0 desc → ID3DBlob)
///   * D3D12SerializeVersionedRootSignature         (1.0 / 1.1 wrapper)
///   * D3D12CreateRootSignatureDeserializer         (blob → ID3D12RootSignatureDeserializer)
///   * D3D12CreateVersionedRootSignatureDeserializer
///
/// None of these touch the GPU — the work is data-only — so the
/// test runs everywhere, including CI without a discrete adapter.

use test
use windows { HANDLE }
use directx12 {
    D3D12SerializeRootSignature,
    D3D12SerializeVersionedRootSignature,
    D3D12CreateRootSignatureDeserializer,
    D3D12CreateVersionedRootSignatureDeserializer,
    D3D12_ROOT_SIGNATURE_DESC,
    D3D12_VERSIONED_ROOT_SIGNATURE_DESC,
    D3D12RootSignatureFlags,
    D3D12RootSignatureVersion,
    IID_ID3D12RootSignatureDeserializer,
    IID_ID3D12VersionedRootSignatureDeserializer,
    IUnknown,
    ID3DBlob,
}

@extern(C) {
    pub fn run() {
        // ── 1.0 path: empty root signature (no params, no samplers) ──
        let desc10 = D3D12_ROOT_SIGNATURE_DESC {
            NumParameters:     0, _pad0: 0,
            pParameters:       0 as *const void,
            NumStaticSamplers: 0, _pad1: 0,
            pStaticSamplers:   0 as *const void,
            Flags:             D3D12RootSignatureFlags.allowInputAssemblerInputLayout as u32,
            _pad2:             0,
        }
        let rawBlob: *void = 0 as *void
        let rawErrs: *void = 0 as *void
        let hr = D3D12SerializeRootSignature(
            &desc10 as *const void,
            D3D12RootSignatureVersion.version_1_0 as u32,
            &rawBlob,
            &rawErrs
        )
        test.expect(hr as i64, 0)
        test.expectTrue((rawBlob as i64) != 0)

        // Hand the serialized bytes back through the deserializer.
        let blob = rawBlob as ID3DBlob
        let bptr = blob.GetBufferPointer()
        let bsz  = blob.GetBufferSize()
        test.expectTrue((bptr as i64) != 0)
        test.expectTrue(bsz > 0)

        let deser: *void = 0 as *void
        let dhr = D3D12CreateRootSignatureDeserializer(
            bptr,
            bsz,
            &IID_ID3D12RootSignatureDeserializer as *const void,
            &deser
        )
        test.expect(dhr as i64, 0)
        test.expectTrue((deser as i64) != 0)
        test.expect((deser as IUnknown).Release() as i64, 0)
        test.expect((rawBlob as IUnknown).Release() as i64, 0)

        // ── 1.1 path: versioned wrapper.
        //
        // The wrapper's union payload at offset 8 holds a DESC1
        // layout. For an empty 1.1 desc — no params, no samplers,
        // just the `allowInputAssemblerInputLayout` flag — every
        // pointer field is NULL and the only non-zero data is
        // `Flags` at offset 32 inside the payload. With the union
        // mapped onto `u0..u5` (each u64, starting at offset 8),
        // `Flags` lands in the low 32 bits of `u4`.
        let versioned = D3D12_VERSIONED_ROOT_SIGNATURE_DESC {
            Version: D3D12RootSignatureVersion.version_1_1 as u32,
            _pad0:   0,
            u0: 0,
            u1: 0,
            u2: 0,
            u3: 0,
            u4: D3D12RootSignatureFlags.allowInputAssemblerInputLayout as u64,
            u5: 0,
        }

        let rawBlob2: *void = 0 as *void
        let rawErrs2: *void = 0 as *void
        let hr2 = D3D12SerializeVersionedRootSignature(
            &versioned as *const void,
            &rawBlob2,
            &rawErrs2
        )
        test.expect(hr2 as i64, 0)
        test.expectTrue((rawBlob2 as i64) != 0)

        let blob2 = rawBlob2 as ID3DBlob
        let deser2: *void = 0 as *void
        let dhr2 = D3D12CreateVersionedRootSignatureDeserializer(
            blob2.GetBufferPointer(),
            blob2.GetBufferSize(),
            &IID_ID3D12VersionedRootSignatureDeserializer as *const void,
            &deser2
        )
        test.expect(dhr2 as i64, 0)
        test.expectTrue((deser2 as i64) != 0)
        test.expect((deser2 as IUnknown).Release() as i64, 0)
        test.expect((rawBlob2 as IUnknown).Release() as i64, 0)
    }
}

run()
