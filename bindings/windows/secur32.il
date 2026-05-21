/// secur32.dll — SSPI (Security Support Provider Interface).
///
/// SSPI is the abstraction layer Win32 apps use to talk to
/// Kerberos / NTLM / Negotiate / Schannel (TLS). Three calls
/// drive the protocol exchange:
///
///   AcquireCredentialsHandleW    — get a handle to the local
///                                    credentials (user / machine)
///   InitializeSecurityContextW   — produce / consume tokens
///                                    (client side)
///   AcceptSecurityContextW       — produce / consume tokens
///                                    (server side)
///
/// `EncryptMessage` / `DecryptMessage` then transport the wrapped
/// application data once the context is established.
///
/// `SECURITY_STATUS` values are large negative numbers (e.g.
/// SEC_I_CONTINUE_NEEDED = 0x00090312).

@extern(C, "secur32") {
    /// `SecHandle` / `CredHandle` / `CtxtHandle` are all the same
    /// 16-byte two-pointer shape.
    pub struct SecHandle {
        dwLower: u64
        dwUpper: u64
    }

    /// 64-bit timestamp; used for credential expiry.
    pub struct TimeStamp {
        dwLowDateTime:  u32
        dwHighDateTime: u32
    }

    /// One buffer in a `SecBufferDesc` collection.
    pub struct SecBuffer {
        cbBuffer:   u32
        BufferType: u32
        pvBuffer:   *void
    }

    /// Group of buffers passed to / from SSPI calls.
    pub struct SecBufferDesc {
        ulVersion: u32
        cBuffers:  u32
        pBuffers:  *SecBuffer
    }

    // ── Credentials ──────────────────────────────────────────────

    /// Acquire a credentials handle for the named SSP. Common
    /// SSP names (`pszPackage`): L"Negotiate", L"NTLM",
    /// L"Kerberos", L"Schannel".  `fCredentialUse`: 1=inbound,
    /// 2=outbound, 3=both.
    @lib pub fn AcquireCredentialsHandleW(
        pszPrincipal:    *const u16,
        pszPackage:      *const u16,
        fCredentialUse:  u32,
        pvLogonId:       *void,
        pAuthData:       *void,
        pGetKeyFn:       *void,
        pvGetKeyArgument: *void,
        phCredential:    *SecHandle,
        ptsExpiry:       *TimeStamp
    ): i32

    @lib pub fn FreeCredentialsHandle(phCredential: *SecHandle): i32

    // ── Context setup ────────────────────────────────────────────

    @lib pub fn InitializeSecurityContextW(
        phCredential:     *SecHandle,
        phContext:        *SecHandle,         // *partial — or 0 on first call
        pszTargetName:    *const u16,
        fContextReq:      u32,
        Reserved1:        u32,
        TargetDataRep:    u32,
        pInput:           *SecBufferDesc,
        Reserved2:        u32,
        phNewContext:     *SecHandle,
        pOutput:          *SecBufferDesc,
        pfContextAttr:    *u32,
        ptsExpiry:        *TimeStamp
    ): i32

    @lib pub fn AcceptSecurityContextW(
        phCredential:     *SecHandle,
        phContext:        *SecHandle,
        pInput:           *SecBufferDesc,
        fContextReq:      u32,
        TargetDataRep:    u32,
        phNewContext:     *SecHandle,
        pOutput:          *SecBufferDesc,
        pfContextAttr:    *u32,
        ptsExpiry:        *TimeStamp
    ): i32

    @lib pub fn CompleteAuthToken(
        phContext: *SecHandle,
        pToken:    *SecBufferDesc
    ): i32

    @lib pub fn DeleteSecurityContext(phContext: *SecHandle): i32

    // ── Message wrap / unwrap ────────────────────────────────────

    @lib pub fn EncryptMessage(
        phContext:    *SecHandle,
        fQOP:         u32,
        pMessage:     *SecBufferDesc,
        MessageSeqNo: u32
    ): i32

    @lib pub fn DecryptMessage(
        phContext:    *SecHandle,
        pMessage:     *SecBufferDesc,
        MessageSeqNo: u32,
        pfQOP:        *u32
    ): i32

    @lib pub fn MakeSignature(
        phContext:    *SecHandle,
        fQOP:         u32,
        pMessage:     *SecBufferDesc,
        MessageSeqNo: u32
    ): i32

    @lib pub fn VerifySignature(
        phContext:    *SecHandle,
        pMessage:     *SecBufferDesc,
        MessageSeqNo: u32,
        pfQOP:        *u32
    ): i32

    // ── Querying ─────────────────────────────────────────────────

    @lib pub fn QueryContextAttributesW(
        phContext:    *SecHandle,
        ulAttribute:  u32,
        pBuffer:      *void
    ): i32

    @lib pub fn QueryCredentialsAttributesW(
        phCredential: *SecHandle,
        ulAttribute:  u32,
        pBuffer:      *void
    ): i32

    // ── Enumeration ──────────────────────────────────────────────

    /// Enumerate the installed SSPs. Returns 0 on success and
    /// writes a pointer to an array of `SecPkgInfoW` to
    /// `*ppPackageInfo` (free with `FreeContextBuffer`).
    @lib pub fn EnumerateSecurityPackagesW(
        pcPackages:    *u32,
        ppPackageInfo: **void
    ): i32

    @lib pub fn QuerySecurityPackageInfoW(
        pszPackageName: *const u16,
        ppPackageInfo:  **void
    ): i32

    @lib pub fn FreeContextBuffer(pvContextBuffer: *void): i32

    // ── User identity ────────────────────────────────────────────

    /// `GetUserNameExW(format, lpNameBuffer, &nSize)`. `format`:
    /// NameSamCompatible=2, NameDisplay=3, NameUniqueId=6,
    /// NameCanonical=7, NameUserPrincipal=8, NameDnsDomain=12.
    @lib pub fn GetUserNameExW(
        NameFormat:   i32,
        lpNameBuffer: *u16,
        nSize:        *u32
    ): i32
}
