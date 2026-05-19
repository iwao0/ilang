// crypt32.dll — Cryptography API: certificate stores, message
// signing/verification, PKCS encoding, and Data-Protection API (DPAPI).
//
// Most functions return a non-zero value on success; call
// `GetLastError()` for details on failure.

// ─── Certificate store providers (CERT_STORE_PROV_*) ─────────────
// Pass as the second argument to CertOpenStore. Common values:
//   "Memory"       — in-memory cert store
//   "File"         — file-backed
//   "System"       — registry-backed system store
//   "PKCS7"        — PKCS#7 message
//   "PFX"          — PKCS#12 (with password)
//   "Serialized"   — serialized form

// ─── CertOpenStore flags ─────────────────────────────────────────
@flags
pub enum CertStoreFlags: u32 {
    noCrypt          = 0x00000001
    setLocalmachine  = 0x00000002
    deferCloseUntilLastFree = 0x00000004
    deleteFlag       = 0x00000010
    shareStoreFlag   = 0x00000040
    shareContextFlag = 0x00000080
    manifoldFlag     = 0x00000100
    enumArchivedFlag = 0x00000200
    updateKeysetFlag = 0x00000400
    backupRestore    = 0x00000800
    readOnly         = 0x00008000
    openExisting     = 0x00004000
    createNew        = 0x00002000
    maximumAllowed   = 0x00001000
    // location flags — go into the high word
    locationCurrentService    = 0x00040000
    locationServices          = 0x00050000
    locationUsers             = 0x00060000
    locationCurrentUserGroupPolicy = 0x00070000
    locationLocalMachineGroupPolicy = 0x00080000
    locationLocalMachineEnterprise  = 0x00090000
    locationCurrentUser       = 0x00010000
    locationLocalMachine      = 0x00020000
}

// ─── Encoding types (X509_ASN_ENCODING | PKCS_7_ASN_ENCODING) ────
@flags
pub enum CertEncodingType: u32 {
    x509Asn   = 0x00000001
    x509Ndr   = 0x00000002
    pkcs7Asn  = 0x00010000
    pkcs7Ndr  = 0x00020000
    /// Combined value passed to most crypto functions.
    cryptAsn  = 0x00010001    // x509Asn | pkcs7Asn
}

// ─── CertFindCertificateInStore find type ────────────────────────
pub enum CertFindType: u32 {
    any              = 0x00000000
    subjectCert      = 0x000B0000   // exact CERT_CONTEXT match
    issuerName       = 0x00040004   // *CERT_NAME_BLOB
    subjectName      = 0x00070004
    issuerStr        = 0x00040007   // *const u16 (substring)
    subjectStr       = 0x00070007
    hash             = 0x00010000   // *CRYPT_HASH_BLOB (SHA-1 thumbprint)
    sha1Hash         = 0x00010000   // = hash
    keyIdentifier    = 0x000F0000   // *CRYPT_HASH_BLOB
    md5Hash          = 0x00040000
    publicKey        = 0x00060000
    keySpec          = 0x00090000
    enhkeyUsage      = 0x000A0000
}

// ─── Property IDs (CERT_*_PROP_ID) for CertGetCertificateContextProperty
pub enum CertPropId: u32 {
    keyProvHandle      = 1
    keyProvInfo        = 2
    sha1Hash           = 3
    hash               = 3
    md5Hash            = 4
    keyContext         = 5
    keySpec            = 6
    ie30Reserved       = 7
    pubkeyHash         = 8
    enhkeyUsage        = 9
    nextUpdateLocation = 10
    friendlyName       = 11
    pvkFile            = 12
    description        = 13
    accessState        = 14
    signatureHash      = 15
    smartCardData      = 16
    efs                = 17
    feeSignatureHash   = 18
    extendedError      = 19
    rendererProvInfo   = 20
    archived           = 21
    keyIdentifier      = 22
    autoEnroll         = 23
    pubkeyAlgPara      = 22
    crossCertDistPoints = 23
    issuerPublicKeyMd5Hash = 24
    subjectPublicKeyMd5Hash = 25
}

// ─── CryptProtectData / CryptUnprotectData flags ─────────────────
@flags
pub enum CryptProtectFlags: u32 {
    uiForbidden          = 0x1
    localMachine         = 0x4
    crossPlatform        = 0x10
    auditUiOnDecrypt     = 0x20
    verifyProtection     = 0x40
}

@extern(C, "crypt32") {
    // ── Opaque types ──────────────────────────────────────────────

    /// Certificate store handle (HCERTSTORE).
    pub struct HCERTSTORE {}

    // ── CRYPT_INTEGER_BLOB / CRYPT_DATA_BLOB — 16 bytes ──────────

    /// Generic binary blob: byte count + pointer.
    /// Used for hashes, signatures, raw data, etc.
    pub struct CRYPT_DATA_BLOB {
        cbData: u32
        // 4 bytes implicit padding before pointer
        pbData: *u8
    }

    // ── CERT_CONTEXT — 40 bytes ──────────────────────────────────

    /// Pointer to one decoded certificate.  Read-only; do not free
    /// directly — use `CertFreeCertificateContext`.
    /// The `pCertInfo` field points to a CERT_INFO with subject/issuer
    /// names, validity, public key, and extensions.
    pub struct CERT_CONTEXT {
        dwCertEncodingType: u32
        // 4 bytes implicit padding before pointer
        pbCertEncoded:      *u8
        cbCertEncoded:      u32
        _pad:               u32
        pCertInfo:          *void   // *CERT_INFO
        hCertStore:         *HCERTSTORE
    }

    // ── Store open / close ────────────────────────────────────────

    /// Opens a certificate store.
    /// `lpszStoreProvider`: provider name, e.g. "System".
    /// `dwEncodingType`: 0 unless required by the provider.
    /// `hCryptProv`: 0 for default.
    /// `dwFlags`: CertStoreFlags.* (location | open flags).
    /// `pvPara`: provider-specific (e.g. UTF-16 store name like "MY").
    @lib pub fn CertOpenStore(
        lpszStoreProvider: *const char,
        dwEncodingType:    u32,
        hCryptProv:        u64,
        dwFlags:           u32,
        pvPara:            *const void
    ): *HCERTSTORE

    /// Opens "MY" / "ROOT" / "CA" / "TRUST" stores by name.
    @lib pub fn CertOpenSystemStoreA(
        hProv:            u64,
        szSubsystemProtocol: *const char
    ): *HCERTSTORE

    /// Closes a cert store. `dwFlags`:
    ///   0                        = release reference only
    ///   1 (CERT_CLOSE_FORCE_FLAG)= force close even with open contexts
    @lib pub fn CertCloseStore(hCertStore: *HCERTSTORE, dwFlags: u32): i32

    // ── Certificate enumeration / search ──────────────────────────

    /// Iterates all certificates in the store.
    /// First call: pPrevCertContext = NULL.
    /// Subsequent calls: pass the previous result.  Returns NULL at end.
    @lib pub fn CertEnumCertificatesInStore(
        hCertStore:       *HCERTSTORE,
        pPrevCertContext: *const CERT_CONTEXT
    ): *CERT_CONTEXT

    /// Finds a certificate matching `pvFindPara` according to `dwFindType`.
    /// `dwCertEncodingType`: CertEncodingType.cryptAsn for typical use.
    /// `dwFindFlags`: 0 for default.
    @lib pub fn CertFindCertificateInStore(
        hCertStore:       *HCERTSTORE,
        dwCertEncodingType: u32,
        dwFindFlags:      u32,
        dwFindType:       u32,
        pvFindPara:       *const void,
        pPrevCertContext: *const CERT_CONTEXT
    ): *CERT_CONTEXT

    /// Releases a CERT_CONTEXT obtained from enumeration or find.
    @lib pub fn CertFreeCertificateContext(pCertContext: *const CERT_CONTEXT): i32

    /// Duplicates a CERT_CONTEXT (increases its reference count).
    @lib pub fn CertDuplicateCertificateContext(pCertContext: *const CERT_CONTEXT): *CERT_CONTEXT

    // ── Names / properties ────────────────────────────────────────

    /// Extracts the subject or issuer name as a string.
    /// `dwType`: 4 (CERT_NAME_FRIENDLY_DISPLAY_TYPE),
    ///          3 (CERT_NAME_SIMPLE_DISPLAY_TYPE),
    ///          5 (CERT_NAME_DNS_TYPE),
    ///          6 (CERT_NAME_URL_TYPE).
    /// `dwFlags`: 1 (CERT_NAME_ISSUER_FLAG) for issuer instead of subject.
    /// Returns the number of characters written (including NUL).
    @lib pub fn CertGetNameStringA(
        pCertContext: *const CERT_CONTEXT,
        dwType:       u32,
        dwFlags:      u32,
        pvTypePara:   *const void,
        pszNameString: *char,
        cchNameString: u32
    ): u32

    /// Reads a property (CertPropId.*). `pvData` receives the raw bytes;
    /// pass NULL for `pvData` to query the size.
    @lib pub fn CertGetCertificateContextProperty(
        pCertContext: *const CERT_CONTEXT,
        dwPropId:     u32,
        pvData:       *void,
        pcbData:      *u32
    ): i32

    // ── Signature verification ────────────────────────────────────

    /// Verifies the signature of one certificate against an issuer's
    /// public key. Returns non-zero on valid signature.
    @lib pub fn CryptVerifyCertificateSignature(
        hCryptProv:     u64,
        dwCertEncodingType: u32,
        pbEncoded:      *const u8,
        cbEncoded:      u32,
        pPublicKey:     *const void
    ): i32

    // ── Encode / decode ──────────────────────────────────────────

    /// Encodes a structured object (CERT_INFO etc.) to its ASN.1 DER form.
    @lib pub fn CryptEncodeObject(
        dwCertEncodingType: u32,
        lpszStructType:     *const char,
        pvStructInfo:       *const void,
        pbEncoded:          *u8,
        pcbEncoded:         *u32
    ): i32

    @lib pub fn CryptDecodeObject(
        dwCertEncodingType: u32,
        lpszStructType:     *const char,
        pbEncoded:          *const u8,
        cbEncoded:          u32,
        dwFlags:            u32,
        pvStructInfo:       *void,
        pcbStructInfo:      *u32
    ): i32

    // ── DPAPI (Data Protection API) ──────────────────────────────

    /// Encrypts data using the current user's master key (default)
    /// or the machine's master key (CryptProtectFlags.localMachine).
    /// Decrypt with `CryptUnprotectData`. The output blob is opaque
    /// and includes a MAC; corrupt blobs are detected.
    @lib pub fn CryptProtectData(
        pDataIn:        *const CRYPT_DATA_BLOB,
        szDataDescr:    *const u16,
        pOptionalEntropy: *const CRYPT_DATA_BLOB,
        pvReserved:     *void,
        pPromptStruct:  *const void,
        dwFlags:        u32,
        pDataOut:       *CRYPT_DATA_BLOB     // free `pbData` with LocalFree
    ): i32

    @lib pub fn CryptUnprotectData(
        pDataIn:           *const CRYPT_DATA_BLOB,
        ppszDataDescr:     **u16,
        pOptionalEntropy:  *const CRYPT_DATA_BLOB,
        pvReserved:        *void,
        pPromptStruct:     *const void,
        dwFlags:           u32,
        pDataOut:          *CRYPT_DATA_BLOB
    ): i32

    // ── String → binary / Base64 / Hex ───────────────────────────

    /// Converts a hex / Base64 string to bytes.
    /// `dwFlags`: CRYPT_STRING_BASE64 = 1, CRYPT_STRING_HEX = 4,
    ///            CRYPT_STRING_BASE64HEADER = 0 (PEM with header).
    @lib pub fn CryptStringToBinaryA(
        pszString:    *const char,
        cchString:    u32,
        dwFlags:      u32,
        pbBinary:     *u8,
        pcbBinary:    *u32,
        pdwSkip:      *u32,
        pdwFlags:     *u32
    ): i32

    /// Converts bytes to a hex / Base64 string.
    @lib pub fn CryptBinaryToStringA(
        pbBinary:    *const u8,
        cbBinary:    u32,
        dwFlags:     u32,
        pszString:   *char,
        pcchString:  *u32
    ): i32
}
