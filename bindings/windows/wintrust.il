/// wintrust.dll — Authenticode / catalog signature verification.
///
/// `WinVerifyTrust` is the universal "is this file signed and
/// trusted?" entry point. Drive it with `WINTRUST_DATA` pointing
/// at a `WINTRUST_FILE_INFO` (typical case).
///
/// Common `pgActionID` GUIDs (cast a 16-byte GUID literal):
///   WINTRUST_ACTION_GENERIC_VERIFY_V2 =
///     {00aac56b-cd44-11d0-8cc2-00c04fc295ee}
///
/// Common return codes:
///   ERROR_SUCCESS                 = 0           — signed and trusted
///   TRUST_E_NOSIGNATURE           = 0x800B0100  — no signature
///   TRUST_E_BAD_DIGEST            = 0x80096010
///   TRUST_E_EXPLICIT_DISTRUST     = 0x800B0111
///   CRYPT_E_SECURITY_SETTINGS     = 0x80092026
///   CERT_E_REVOKED                = 0x80092010
///   CERT_E_UNTRUSTEDROOT          = 0x800B0109

// ─── WTD_UI_* — UI control for WinVerifyTrust ────────────────────
pub enum WtdUi: u32 {
    all     = 1
    none    = 2
    noBad   = 3
    noGood  = 4
}

// ─── WTD_REVOCATION_CHECK_* ──────────────────────────────────────
pub enum WtdRevocation: u32 {
    none         = 0
    wholeChain   = 1
}

// ─── WTD_CHOICE_* — which structure to read ──────────────────────
pub enum WtdChoice: u32 {
    file      = 1
    catalog   = 2
    blob      = 3
    signer    = 4
    cert      = 5
}

// ─── WTD_STATEACTION_* ───────────────────────────────────────────
pub enum WtdStateAction: u32 {
    ignore          = 0
    verify          = 1
    close           = 2
    autoCache       = 3
    autoCacheFlush  = 4
}

@extern(C, "wintrust") {
    /// File-based info — point `pcwszFilePath` at the file to verify.
    pub struct WINTRUST_FILE_INFO {
        cbStruct:       u32
        _pad:           u32
        pcwszFilePath:  *const u16
        hFile:          i64                // *HANDLE — optional, 0
        pgKnownSubject: *const void        // optional, 0
    }

    pub struct WINTRUST_DATA {
        cbStruct:               u32
        _pad0:                  u32
        pPolicyCallbackData:    *void
        pSIPClientData:         *void
        dwUIChoice:             u32        // WtdUi
        fdwRevocationChecks:    u32        // WtdRevocation
        dwUnionChoice:          u32        // WtdChoice (file/catalog/...)
        _pad1:                  u32
        pInfo:                  *void      // *WINTRUST_FILE_INFO or sibling
        dwStateAction:          u32        // WtdStateAction
        _pad2:                  u32
        hWVTStateData:          i64        // *HANDLE — cleared by close
        pwszURLReference:       *const u16
        dwProvFlags:            u32
        dwUIContext:            u32
        pSignatureSettings:     *void
    }

    /// Verify `pWVTData` against `pgActionID`. Returns 0 on success
    /// (signed + trusted), or one of the TRUST_E_* / CERT_E_* codes
    /// above. Pass the same call again with `dwStateAction = close`
    /// to release the cached chain.
    @lib pub fn WinVerifyTrust(
        hwnd:        i64,           // *HWND, or 0 for non-interactive
        pgActionID:  *const void,   // *GUID
        pWVTData:    *void
    ): i32

    /// Newer variant — same shape, returns a typed `HRESULT` and
    /// supports per-call policy customisation.
    @lib pub fn WinVerifyTrustEx(
        hwnd:        i64,
        pgActionID:  *const void,
        pWintrustData: *WINTRUST_DATA
    ): i32

    /// Look up the signer cert chain hash. Useful for showing
    /// "this was signed by …" UI.
    @lib pub fn WTHelperProvDataFromStateData(
        hStateData: i64
    ): *void

    @lib pub fn WTHelperGetProvSignerFromChain(
        pProvData:     *void,
        idxSigner:     u32,
        fCounterSigner: i32,
        idxCounterSigner: u32
    ): *void

    @lib pub fn WTHelperGetProvCertFromChain(
        pSgnr:         *void,
        idxCert:       u32
    ): *void
}
