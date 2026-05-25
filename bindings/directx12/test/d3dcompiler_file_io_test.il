/// d3dcompiler — round-trip a compiled blob through the file I/O
/// entry points:
///   * D3DCompileFromFile  — UTF-16 path → compiled bytecode blob
///   * D3DWriteBlobToFile  — blob → on-disk .cso
///   * D3DReadFileToBlob   — .cso → blob, sized against the original
///
/// Stages two temp files via GetTempFileNameA (one for the HLSL
/// source, one for the .cso output) and promotes their paths to
/// UTF-16 with MultiByteToWideChar before handing them to the
/// d3dcompiler entries. Each step bails cleanly when the underlying
/// call fails (no d3dcompiler_47, no write access, etc.) so CI
/// without the SDK still runs to the end.

use std.test as test
use windows {
use std.ffi { cstrFromString, freeCstr }
    HANDLE, NULL,
    GetProcessHeap, HeapAlloc, HeapFree,
    MultiByteToWideChar,
    GetTempPathA, GetTempFileNameA,
    CreateFileA, WriteFile, CloseHandle, DeleteFileA,
    FileAccess, FileShare, FileCreation,
}
use directx12 {
    D3DCompileFromFile, D3DWriteBlobToFile, D3DReadFileToBlob,
    D3D_SHADER_MACRO, ID3DBlob,
    D3DCompileFlags
}
use ole32 { IUnknown }

@extern(C) {
    /// Convert an ANSI null-terminated buffer (passed as `i64`) to
    /// a fresh HeapAlloc'd UTF-16 buffer. Returns the buffer as
    /// `i64` so the public-ABI rule (no raw pointer params) stays
    /// satisfied; cast to `*const u16` at the call site.
    pub fn ansiToWide(srcPtr: i64): i64 {
        let src = srcPtr as *const char
        let need = MultiByteToWideChar(0 as u32, 0 as u32, src, -1 as i32, 0 as *u16, 0 as i32)
        let bytes = (need * 2) as u64
        let buf = HeapAlloc(GetProcessHeap(), 8 as u32, bytes)
        let _ = MultiByteToWideChar(
            0 as u32, 0 as u32, src, -1 as i32,
            buf as *u16, need
        )
        buf as i64
    }

    pub fn freeWide(p: i64) {
        HeapFree(GetProcessHeap(), 0 as u32, p as *void)
    }

    /// Stage a temp file under `dirPtr`'s ANSI path with `prefixPtr`
    /// as the GetTempFileNameA prefix. Writes `cbytes` bytes from
    /// `contentsPtr`. Returns the ANSI path as a heap buffer
    /// (caller frees via `HeapFree`), or 0 on any failure.
    pub fn writeTempFile(dirPtr: i64, prefixPtr: i64,
                         contentsPtr: i64, cbytes: u32): i64 {
        let dir = dirPtr as *const char
        let prefix = prefixPtr as *const char
        let contents = contentsPtr as *const void
        let path = HeapAlloc(GetProcessHeap(), 8 as u32, 260 as u64)
        let uniq = GetTempFileNameA(dir, prefix, 0 as u32, path as *char)
        if uniq == 0 as u32 {
            HeapFree(GetProcessHeap(), 0 as u32, path)
            return 0
        }
        let file = CreateFileA(
            path as *const char,
            FileAccess.write as u32,
            FileShare.read as u32,
            NULL,
            FileCreation.createAlways as u32,
            0x80 as u32,                       // FILE_ATTRIBUTE_NORMAL
            NULL
        )
        if (file as i64) == -1 {
            HeapFree(GetProcessHeap(), 0 as u32, path)
            return 0
        }
        let written: u32 = 0
        let ok = WriteFile(file, contents, cbytes, &written, NULL)
        let _ = CloseHandle(file)
        if ok == 0 || written != cbytes {
            let _ = DeleteFileA(path as *const char)
            HeapFree(GetProcessHeap(), 0 as u32, path)
            return 0
        }
        path as i64
    }

    pub fn run() {
        // ── 1. Stage temp HLSL ─────────────────────────────────────
        let dir = HeapAlloc(GetProcessHeap(), 8 as u32, 260 as u64)
        let n = GetTempPathA(260 as u32, dir as *char)
        if n == 0 as u32 {
            HeapFree(GetProcessHeap(), 0 as u32, dir)
            return
        }

        let hlsl = cstrFromString(
            "float4 main(float3 p : POSITION) : SV_POSITION { return float4(p, 1.0); }"
        )
        let pfxHlsl = cstrFromString("hl")
        let hlslPathPtr = writeTempFile(dir as i64, pfxHlsl as i64,
                                         hlsl as i64, 76 as u32)
        freeCstr(hlsl)
        freeCstr(pfxHlsl)
        if hlslPathPtr == 0 {
            HeapFree(GetProcessHeap(), 0 as u32, dir)
            return
        }

        // ── 2. D3DCompileFromFile ──────────────────────────────────
        let wPath = ansiToWide(hlslPathPtr)
        let entry  = cstrFromString("main")
        let target = cstrFromString("vs_5_0")
        let rawCode: *void = NULL
        let rawErrs: *void = NULL
        let crc = D3DCompileFromFile(
            wPath as *const u16,
            0 as *const D3D_SHADER_MACRO,
            0 as *void,
            entry,
            target,
            D3DCompileFlags.skipValidation as u32,
            0 as u32,
            &rawCode as **ID3DBlob,
            &rawErrs as **ID3DBlob
        )
        freeCstr(entry)
        freeCstr(target)
        freeWide(wPath)

        if crc != 0 {
            // d3dcompiler_47 missing or compile failed — unrelated to
            // the binding shape; tidy up and stop.
            let _ = DeleteFileA(hlslPathPtr as *const char)
            HeapFree(GetProcessHeap(), 0 as u32, hlslPathPtr as *void)
            HeapFree(GetProcessHeap(), 0 as u32, dir)
            return
        }
        test.expectTrue((rawCode as i64) != 0)
        let code = rawCode as ID3DBlob
        let codeSize = code.GetBufferSize()
        test.expectTrue(codeSize > 0)

        // ── 3. D3DWriteBlobToFile + D3DReadFileToBlob ──────────────
        let pfxCso = cstrFromString("cs")
        let csoPath = HeapAlloc(GetProcessHeap(), 8 as u32, 260 as u64)
        let csoUniq = GetTempFileNameA(dir as *const char, pfxCso as *const char, 0 as u32, csoPath as *char)
        freeCstr(pfxCso)
        if csoUniq == 0 as u32 {
            test.expect((rawCode as IUnknown).Release() as i64, 0)
            let _ = DeleteFileA(hlslPathPtr as *const char)
            HeapFree(GetProcessHeap(), 0 as u32, hlslPathPtr as *void)
            HeapFree(GetProcessHeap(), 0 as u32, csoPath)
            HeapFree(GetProcessHeap(), 0 as u32, dir)
            return
        }

        let wOut = ansiToWide(csoPath as i64)
        let wHr = D3DWriteBlobToFile(rawCode as *ID3DBlob, wOut as *const u16, 1 as i32)
        test.expect(wHr as i64, 0)

        let rawBack: *void = NULL
        let rHr = D3DReadFileToBlob(wOut as *const u16, &rawBack as **ID3DBlob)
        test.expect(rHr as i64, 0)
        let back = rawBack as ID3DBlob
        test.expect(back.GetBufferSize() as i64, codeSize as i64)

        // ── 4. Cleanup ─────────────────────────────────────────────
        test.expect((rawBack as IUnknown).Release() as i64, 0)
        test.expect((rawCode as IUnknown).Release() as i64, 0)
        freeWide(wOut)
        let _ = DeleteFileA(csoPath as *const char)
        let _ = DeleteFileA(hlslPathPtr as *const char)
        HeapFree(GetProcessHeap(), 0 as u32, csoPath)
        HeapFree(GetProcessHeap(), 0 as u32, hlslPathPtr as *void)
        HeapFree(GetProcessHeap(), 0 as u32, dir)
    }
}

run()
