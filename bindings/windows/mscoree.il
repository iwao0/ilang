/// mscoree.dll — CLR (Common Language Runtime) hosting.
///
/// Used to bring up a managed-code runtime in an unmanaged
/// (C / C++ / ilang) process. The .NET Framework (CLR2 / CLR4) ships
/// with these flat entry points; .NET Core / .NET 5+ replaces them
/// with `nethost.dll` + `hostfxr.dll`.
///
/// Bring-up (.NET Framework 4):
///   let rt: *void = 0 as *void
///   CLRCreateInstance(&CLSID_CLRMetaHost, &IID_ICLRMetaHost, &meta)
///   meta.GetRuntime(L"v4.0.30319", &IID_ICLRRuntimeInfo, &rti)
///   rti.GetInterface(&CLSID_CLRRuntimeHost, &IID_ICLRRuntimeHost,
///                    &rt)
///   rt.Start()
///   rt.ExecuteInDefaultAppDomain(L"asm.dll", L"Ns.Cls", L"Method",
///                                L"arg", &retVal)

@extern(C, "mscoree") {
    /// Modern entry point — `CLRCreateInstance(rclsid, riid, ppvHost)`.
    /// `rclsid` is one of:
    ///   CLSID_CLRMetaHost            ({9280188D-0E8E-4867-B30C-7FA83884E8DE})
    ///   CLSID_CLRMetaHostPolicy      ({2EBCD49A-1B47-4A61-B13A-4A03701E594B})
    ///   CLSID_CLRStrongName          ({B79B0ACD-F5CD-409B-B5A5-A16244610B92})
    @lib pub fn CLRCreateInstance(
        clsid:      *const void,            // *GUID
        riid:       *const void,            // *GUID — IID_ICLRMetaHost etc.
        ppvObject:  **void
    ): i32

    /// Legacy single-host APIs (pre-4.0). Still present on every
    /// modern Windows even though they target CLR 2.0.
    @lib pub fn CorBindToRuntimeEx(
        pwszVersion:  *const u16,
        pwszBuildFlavor: *const u16,
        startupFlags: u32,                  // STARTUP_FLAGS enum
        rclsid:       *const void,
        riid:         *const void,
        ppv:          **void
    ): i32

    @lib pub fn CorBindToCurrentRuntime(
        pwszFileName: *const u16,
        rclsid:       *const void,
        riid:         *const void,
        ppv:          **void
    ): i32

    /// Looks up the CLR version embedded in a managed PE.
    @lib pub fn GetFileVersion(
        szFilename:  *const u16,
        szBuffer:    *u16,
        cchBuffer:   u32,
        dwLength:    *u32
    ): i32

    @lib pub fn GetRequestedRuntimeInfo(
        pExe:                 *const u16,
        pwszVersion:          *const u16,
        pConfigurationFile:   *const u16,
        startupFlags:         u32,
        runtimeInfoFlags:     u32,
        pDirectory:           *u16,
        dwDirectory:          u32,
        dwDirectoryLength:    *u32,
        pVersion:             *u16,
        cchBuffer:            u32,
        dwlength:             *u32
    ): i32

    /// Has any CLR been loaded into this process?
    @lib pub fn GetCORSystemDirectory(
        pBuffer:     *u16,
        cchBuffer:   u32,
        dwLength:    *u32
    ): i32

    @lib pub fn GetCORVersion(
        pbBuffer:    *u16,
        cchBuffer:   u32,
        dwLength:    *u32
    ): i32

    @lib pub fn LoadStringRC(
        iResourceID: u32,
        szBuffer:    *u16,
        iMax:        i32,
        bQuiet:      i32
    ): i32

    /// Strong-name signing helpers (CLR2 era — partial set).
    @lib pub fn StrongNameSignatureGeneration(
        wszFilePath:    *const u16,
        wszKeyContainer: *const u16,
        pbKeyBlob:      *const u8,
        cbKeyBlob:      u32,
        ppbSignatureBlob: **u8,
        pcbSignatureBlob: *u32
    ): i32

    @lib pub fn StrongNameSignatureVerificationEx(
        wszFilePath:    *const u16,
        fForceVerification: i32,
        pfWasVerified:  *i32
    ): i32

    @lib pub fn StrongNameFreeBuffer(pbMemory: *u8)

    @lib pub fn StrongNameErrorInfo(): i32
}
