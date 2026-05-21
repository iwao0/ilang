/// d3dcompiler — exercise the post-compile blob helpers.
///
/// Builds on `d3dcompiler_test.il`: compile a tiny VS, then push
/// the resulting bytecode through D3DDisassemble, D3DStripShader,
/// D3DGetBlobPart, and the secondary-data variant `D3DCompile2`.
///
/// On hosts without d3dcompiler_47.dll every entry returns a
/// negative HRESULT and the test bails cleanly — same convention as
/// the rest of the harness.

use test
use windows { HANDLE }
use directx12 {
    D3DCompile, D3DCompile2,
    D3DDisassemble, D3DStripShader, D3DGetBlobPart, D3DReflect,
    D3D_SHADER_MACRO, ID3DBlob,
    D3DCompileFlags
}
use ole32 { GUID, IUnknown }

/// IID_ID3D11ShaderReflection = 8d536ca1-0cca-4956-a837-786963755584.
/// This is what d3dcompiler_47.dll's `D3DReflect` accepts — the
/// equivalent of `IID_ID3D12ShaderReflection`, just a historical
/// name carried forward.
pub const IID_ID3D11ShaderReflection: GUID = GUID {
    Data1: 0x8d536ca1, Data2: 0x0cca, Data3: 0x4956,
    Data4lo: 0x697837a8, Data4hi: 0x84557563,
}

@extern(C) {
    pub fn run() {
        let hlsl = cstrFromString(
            "float4 main(float3 p : POSITION) : SV_POSITION { return float4(p, 1.0); }"
        )
        let entry  = cstrFromString("main")
        let target = cstrFromString("vs_5_0")

        let rawCode: *void = 0 as *void
        let rawErrs: *void = 0 as *void
        let rc = D3DCompile(
            hlsl as *const void,
            76 as u64,
            0 as *const char,
            0 as *const D3D_SHADER_MACRO,
            0 as *void,
            entry,
            target,
            D3DCompileFlags.skipValidation as u32,
            0 as u32,
            &rawCode as **ID3DBlob,
            &rawErrs as **ID3DBlob
        )
        if rc != 0 {
            // No d3dcompiler installed — accept that and exit clean.
            freeCstr(hlsl); freeCstr(entry); freeCstr(target)
            return
        }
        test.expectTrue((rawCode as i64) != 0)

        let code = rawCode as ID3DBlob
        let bytecode = code.GetBufferPointer()
        let bytecodeSize = code.GetBufferSize()
        test.expectTrue((bytecode as i64) != 0)
        test.expectTrue(bytecodeSize > 0)

        // ── D3DDisassemble ─────────────────────────────────────────
        let rawDis: *void = 0 as *void
        let drc = D3DDisassemble(
            bytecode,
            bytecodeSize,
            0 as u32,
            0 as *const char,
            &rawDis as **ID3DBlob
        )
        test.expect(drc as i64, 0)
        test.expectTrue((rawDis as i64) != 0)
        let dis = rawDis as ID3DBlob
        test.expectTrue(dis.GetBufferSize() > 0)

        // ── D3DGetBlobPart: pull the input-signature sub-blob ──────
        // D3D_BLOB_INPUT_SIGNATURE_BLOB = 0.
        let rawPart: *void = 0 as *void
        let prc = D3DGetBlobPart(
            bytecode,
            bytecodeSize,
            0 as u32,                     // INPUT_SIGNATURE
            0 as u32,
            &rawPart as **ID3DBlob
        )
        test.expect(prc as i64, 0)
        test.expectTrue((rawPart as i64) != 0)
        let part = rawPart as ID3DBlob
        test.expectTrue(part.GetBufferSize() > 0)

        // ── D3DStripShader: drop reflection + debug data ───────────
        // STRIP_REFLECTION_DATA(1) | STRIP_DEBUG_INFO(2) = 3.
        let rawStripped: *void = 0 as *void
        let src = D3DStripShader(
            bytecode,
            bytecodeSize,
            3 as u32,
            &rawStripped as **ID3DBlob
        )
        test.expect(src as i64, 0)
        test.expectTrue((rawStripped as i64) != 0)
        let stripped = rawStripped as ID3DBlob
        // Stripped blob is at most the original size.
        test.expectTrue(stripped.GetBufferSize() <= bytecodeSize)

        // ── D3DCompile2: same source, no secondary data ────────────
        let rawCode2: *void = 0 as *void
        let rawErrs2: *void = 0 as *void
        let c2 = D3DCompile2(
            hlsl as *const void,
            76 as u64,
            0 as *const char,
            0 as *const D3D_SHADER_MACRO,
            0 as *void,
            entry,
            target,
            D3DCompileFlags.skipValidation as u32,
            0 as u32,
            0 as u32,
            0 as *const void,
            0 as u64,
            &rawCode2 as **ID3DBlob,
            &rawErrs2 as **ID3DBlob
        )
        test.expect(c2 as i64, 0)
        test.expectTrue((rawCode2 as i64) != 0)

        // ── D3DReflect — turn bytecode into a shader-reflection
        //    interface and confirm the handle round-trips. The
        //    classic IID lives in d3d11shader.h and is still what
        //    d3dcompiler_47 accepts for SM5 shaders.
        let rawRefl: *void = 0 as *void
        let refHr = D3DReflect(
            bytecode,
            bytecodeSize,
            &IID_ID3D11ShaderReflection as *const void,
            &rawRefl
        )
        test.expect(refHr as i64, 0)
        test.expectTrue((rawRefl as i64) != 0)
        test.expect((rawRefl as IUnknown).Release() as i64, 0)

        // ── Release the blobs we acquired ──────────────────────────
        test.expect((rawCode2   as IUnknown).Release() as i64, 0)
        test.expect((rawStripped as IUnknown).Release() as i64, 0)
        test.expect((rawPart    as IUnknown).Release() as i64, 0)
        test.expect((rawDis     as IUnknown).Release() as i64, 0)
        test.expect((rawCode    as IUnknown).Release() as i64, 0)

        freeCstr(hlsl); freeCstr(entry); freeCstr(target)
    }
}

run()
