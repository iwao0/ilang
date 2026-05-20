/// d3dcompiler_47.dll — HLSL shader compilation.
///
/// The runtime DLL is `d3dcompiler_47.dll` (the latest stable version);
/// the import library `d3dcompiler.lib` in the Windows 10 SDK forwards
/// to the same DLL.  At runtime the DLL must be present — it's part
/// of standard Windows 10 / 11 but for redist scenarios you may need
/// to ship it alongside your binary.
///
/// Most apps shouldn't compile shaders at runtime in production;
/// pre-compile to DXIL/DXBC via dxc.exe / fxc.exe at build time
/// and use D3DReadFileToBlob to load the .cso instead.

// ─── D3DCompile flags1 (D3DCOMPILE_*) ────────────────────────────
@flags
pub enum D3DCompileFlags: u32 {
    debug                       = 0x1
    skipValidation              = 0x2
    skipOptimization            = 0x4
    packMatrixRowMajor          = 0x8
    packMatrixColumnMajor       = 0x10
    partialPrecision            = 0x20
    forceVsSoftwareNoOpt        = 0x40
    forcePsSoftwareNoOpt        = 0x80
    noPreshader                 = 0x100
    avoidFlowControl            = 0x200
    preferFlowControl           = 0x400
    enableStrictness            = 0x800
    enableBackwardsCompatibility = 0x1000
    ieeeStrictness              = 0x2000
    optimizationLevel0          = 0x4000
    optimizationLevel1          = 0x0
    optimizationLevel2          = 0xc000
    optimizationLevel3          = 0x8000
    warningsAreErrors           = 0x40000
    resourcesMayAlias           = 0x80000
    enableUnboundedDescriptorTables = 0x100000
    allResourcesBound           = 0x200000
    debugNameForSource          = 0x400000
    debugNameForBinary          = 0x800000
}

// ─── D3DCompile flags2 (D3DCOMPILE_EFFECT_*) — usually 0 ─────────
@flags
pub enum D3DCompileEffectFlags: u32 {
    none                = 0
    childEffect         = 1
    allowSlowOps        = 2
}

// `@extern(C, "d3dcompiler_47", "d3dcompiler")` — multi-lib form.
// The JIT loader walks the list from left to right and uses the
// first DLL that opens; the AOT linker picks the first name whose
// loader-side probe succeeded. This handles two SKU shapes:
//   * runtime: only `d3dcompiler_47.dll` ships
//   * SDK:     `d3dcompiler.lib` forwards to the same DLL
// On Windows we get JIT-correct behaviour via the first name and
// keep the fallback for environments where only the SDK alias is
// reachable.
@extern(C, "d3dcompiler_47", "d3dcompiler") {
    // `ID3DBlob` lives in `d3d12_com.il` as a `@com pub interface`
    // (with `GetBufferPointer` / `GetBufferSize` / `Release`); use
    // that definition wherever a blob handle is needed. The opaque
    // `pub struct ID3DBlob {}` that used to sit here shadowed the
    // COM interface and made hover / method dispatch ambiguous.

    // ── D3D_SHADER_MACRO — 16 bytes ──────────────────────────────

    /// `#define NAME DEFINITION` entry. Terminate the array with
    /// `{ NULL, NULL }`.
    pub struct D3D_SHADER_MACRO {
        Name:       *const char
        Definition: *const char
    }

    // ── HLSL compilation ─────────────────────────────────────────

    /// Compiles HLSL source to bytecode.
    ///
    /// `pSrcData`/`SrcDataSize`: source text (no NUL required).
    /// `pSourceName`: optional path for error messages.
    /// `pDefines`: optional NULL-terminated array of `D3D_SHADER_MACRO`.
    /// `pInclude`: ID3DInclude*, or `1 as *void` for
    ///             D3D_COMPILE_STANDARD_FILE_INCLUDE.
    /// `pEntrypoint`: e.g. "VSMain", "PSMain".
    /// `pTarget`: shader profile, e.g. "vs_5_0", "ps_5_1", "cs_6_0".
    /// `Flags1`: D3DCompileFlags.*.
    /// `Flags2`: D3DCompileEffectFlags.* (use 0 for non-effect shaders).
    /// `ppCode`: receives the compiled-bytecode blob.
    /// `ppErrorMsgs`: optional, receives a UTF-8 error string blob.
    @lib pub fn D3DCompile(
        pSrcData:    *const void,
        SrcDataSize: u64,
        pSourceName: *const char,
        pDefines:    *const D3D_SHADER_MACRO,
        pInclude:    *void,
        pEntrypoint: *const char,
        pTarget:     *const char,
        Flags1:      u32,
        Flags2:      u32,
        ppCode:      **ID3DBlob,
        ppErrorMsgs: **ID3DBlob
    ): i32

    /// Compiles HLSL from a file. `pFileName` is UTF-16.
    @lib pub fn D3DCompileFromFile(
        pFileName:   *const u16,
        pDefines:    *const D3D_SHADER_MACRO,
        pInclude:    *void,
        pEntrypoint: *const char,
        pTarget:     *const char,
        Flags1:      u32,
        Flags2:      u32,
        ppCode:      **ID3DBlob,
        ppErrorMsgs: **ID3DBlob
    ): i32

    /// Extended compile with an explicit secondary data block (used
    /// for private root signatures or debug data).
    @lib pub fn D3DCompile2(
        pSrcData:               *const void,
        SrcDataSize:            u64,
        pSourceName:            *const char,
        pDefines:               *const D3D_SHADER_MACRO,
        pInclude:               *void,
        pEntrypoint:            *const char,
        pTarget:                *const char,
        Flags1:                 u32,
        Flags2:                 u32,
        SecondaryDataFlags:     u32,
        pSecondaryData:         *const void,
        SecondaryDataSize:      u64,
        ppCode:                 **ID3DBlob,
        ppErrorMsgs:            **ID3DBlob
    ): i32

    // ── Bytecode introspection ───────────────────────────────────

    /// Disassembles compiled bytecode to a human-readable text blob.
    /// `Flags`: 0 or D3D_DISASM_* combo (1 = ENABLE_COLOR_CODE,
    /// 2 = ENABLE_DEFAULT_VALUE_PRINTS, 4 = ENABLE_INSTRUCTION_NUMBERING).
    @lib pub fn D3DDisassemble(
        pSrcData:    *const void,
        SrcDataSize: u64,
        Flags:       u32,
        szComments:  *const char,
        ppDisassembly: **ID3DBlob
    ): i32

    /// Returns an ID3D12ShaderReflection / ID3D12LibraryReflection
    /// for the given bytecode.  `pInterface`: IID_ID3D12ShaderReflection
    /// (or the library variant).
    @lib pub fn D3DReflect(
        pSrcData:    *const void,
        SrcDataSize: u64,
        pInterface:  *const void,
        ppReflector: **void
    ): i32

    // ── Bytecode read / write ────────────────────────────────────

    /// Reads a compiled-shader file (`.cso`) into a blob.
    @lib pub fn D3DReadFileToBlob(
        pFileName: *const u16,
        ppContents: **ID3DBlob
    ): i32

    @lib pub fn D3DWriteBlobToFile(
        pBlob:    *ID3DBlob,
        pFileName: *const u16,
        bOverwrite: i32
    ): i32

    /// Strips debug / reflection info from compiled bytecode to
    /// reduce binary size before shipping.
    /// `uStripFlags`: D3DCOMPILER_STRIP_* (1 = REFLECTION_DATA,
    /// 2 = DEBUG_INFO, 4 = TEST_BLOBS, 8 = PRIVATE_DATA, 0x10 = ROOT_SIGNATURE).
    @lib pub fn D3DStripShader(
        pShaderBytecode:     *const void,
        BytecodeLength:      u64,
        uStripFlags:         u32,
        ppStrippedBlob:      **ID3DBlob
    ): i32

    /// Gets a specific sub-part of compiled bytecode (e.g. just the
    /// root signature blob).
    /// `Part`: D3D_BLOB_PART (e.g. 0 = INPUT_SIGNATURE, 1 = OUTPUT_SIGNATURE,
    /// 5 = ROOT_SIGNATURE, 11 = DEBUG_INFO).
    @lib pub fn D3DGetBlobPart(
        pSrcData:    *const void,
        SrcDataSize: u64,
        Part:        u32,
        Flags:       u32,
        ppPart:      **ID3DBlob
    ): i32

    /// Creates an empty ID3DBlob of the requested size.
    @lib pub fn D3DCreateBlob(
        Size:    u64,
        ppBlob:  **ID3DBlob
    ): i32

    /// Standard include-handler value: pass for `pInclude` to
    /// enable default #include resolution from `pSourceName`'s
    /// directory.
    /// D3D_COMPILE_STANDARD_FILE_INCLUDE = ((ID3DInclude*)(UINT_PTR)1)
}
