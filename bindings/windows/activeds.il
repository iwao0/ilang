/// activeds.dll — ADSI (Active Directory Service Interfaces).
///
/// COM-only — surface is entirely through IADs / IADsContainer /
/// IADsUser / IDirectoryObject / IDirectorySearch etc., bound via
/// the entry point below + CoCreateInstance against well-known
/// CLSIDs.
///
/// Bring-up:
///   ADsGetObject(L"LDAP://CN=Users,DC=example,DC=com",
///                &IID_IADsContainer, &container)
///   container.GetObject(L"user", L"CN=alice", &iads)
///   iads.QueryInterface(&IID_IADsUser, &user)
///   user.SetPassword(L"new-pw")
///
/// ADSI auto-discovers the directory service (LDAP / NDS / NIS /
/// WinNT depending on the URL scheme). For raw LDAP wire access
/// reach for `wldap32` instead.

@flags
pub enum AdsAuthFlags: u32 {
    secureAuth        = 0x00000001
    useEncryption     = 0x00000002
    useSsl            = 0x00000002      // alias
    readOnlyServer    = 0x00000004
    promptCredentials = 0x00000008
    noAuth            = 0x00000010
    fastBind          = 0x00000020
    useSigning        = 0x00000040
    useSealing        = 0x00000080
    useDelegation     = 0x00000100
    serverBind        = 0x00000200
}

@extern(C, "activeds") {
    /// Bind to an ADSI path. `lpszPathName` is one of:
    ///   L"LDAP://CN=...,DC=..."
    ///   L"WinNT://DOMAIN/Computer/User"
    ///   L"GC://..."  (Global Catalog)
    /// `riid` is the requested interface IID (e.g. IID_IADs).
    @lib pub fn ADsGetObject(
        lpszPathName: *const u16,
        riid:         *const void,         // *GUID
        ppObject:     **void
    ): i32

    /// Bind with explicit credentials. `lpszUserName` /
    /// `lpszPassword` are NULL = current user. `dwReserved`:
    /// AdsAuthFlags.
    @lib pub fn ADsOpenObject(
        lpszPathName: *const u16,
        lpszUserName: *const u16,
        lpszPassword: *const u16,
        dwReserved:   u32,
        riid:         *const void,
        ppObject:     **void
    ): i32

    /// Parse an ADSI path string into its pieces.
    @lib pub fn ADsBuildEnumerator(
        pADsContainer: *void,
        ppEnumVariant: **void
    ): i32

    @lib pub fn ADsFreeEnumerator(pEnumVariant: *void): i32

    /// Step through a container enumerator. `pvar` receives a
    /// VARIANT holding an IDispatch* for the next child.
    @lib pub fn ADsEnumerateNext(
        pEnumVariant: *void,
        cElements:    u64,
        pvar:         *void,           // *VARIANT[]
        pcElementsFetched: *u64
    ): i32

    /// Last DS-extended error info (more useful than HRESULT).
    @lib pub fn ADsGetLastError(
        lpError:    *u32,
        lpErrorBuf: *u16,
        dwErrorBufLen: u32,
        lpNameBuf:  *u16,
        dwNameBufLen: u32
    ): i32

    @lib pub fn ADsSetLastError(
        dwErr:        u32,
        pszError:     *const u16,
        pszProvider:  *const u16
    )

    /// Type / property conversion helpers used by IADs property
    /// caches.
    @lib pub fn AllocADsMem(cb: u32): *void
    @lib pub fn FreeADsMem(pMem: *void): i32

    @lib pub fn AllocADsStr(pStr: *const u16): *u16
    @lib pub fn FreeADsStr(pStr: *u16): i32
    @lib pub fn ReallocADsStr(ppStr: **u16, pStr: *const u16): i32

    /// Build a property entry (IADsPropertyEntry) — used in
    /// property-cache PutEx scenarios.
    @lib pub fn ADsBuildVarArrayStr(
        lppPathNames: **const u16,
        dwPathNames:  u32,
        pVar:         *void          // *VARIANT
    ): i32

    @lib pub fn ADsBuildVarArrayInt(
        lpdwObjectTypes: *const u32,
        dwObjectTypes:   u32,
        pVar:            *void
    ): i32
}
