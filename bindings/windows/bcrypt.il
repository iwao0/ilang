/// bcrypt.dll — Cryptography Next Generation (CNG) primitives.
///
/// Replaces the legacy CryptoAPI (crypt32.dll for cert work, advapi32
/// for CryptAcquireContext etc.).  Use this for new hashing, symmetric
/// encryption, signing, key derivation, and random-number generation.
///
/// All functions return NTSTATUS (i32); 0 = STATUS_SUCCESS.

// ─── NTSTATUS values (selected) ──────────────────────────────────
pub enum NtStatus: i32 {
    success            = 0
    invalidHandle      = -1073741816   // 0xC0000008
    invalidParameter   = -1073741811   // 0xC000000D
    noMemory           = -1073741801   // 0xC0000017
    bufferTooSmall     = -1073741789   // 0xC0000023
    notSupported       = -1073741822   // 0xC0000002
    invalidSignature   = -1073700859   // 0xC0090006
    notFound           = -1073741275   // 0xC0000225
    authTagMismatch    = -1073700864   // 0xC000A002
}

// ─── BCryptOpenAlgorithmProvider flags ───────────────────────────
@flags
pub enum BCryptAlgFlags: u32 {
    /// Algorithm provider handle for HMAC instead of plain hash.
    algHandleHmac      = 0x00000008
    /// Provider handle is for asymmetric (public-key) crypto.
    proviPaprivate     = 0x00000010
    /// Cache the handle for reuse.
    cacheable          = 0x00000020
    hashReusable       = 0x00000020
}

// ─── BCrypt key blob types (UTF-16 strings) ──────────────────────
//   BCRYPT_KEY_DATA_BLOB        = "KeyDataBlob"
//   BCRYPT_OPAQUE_KEY_BLOB      = "OpaqueKeyBlob"
//   BCRYPT_AES_WRAP_KEY_BLOB    = "Rfc3565KeyWrapBlob"
//   BCRYPT_DSA_PUBLIC_BLOB      = "DSAPUBLICBLOB"
//   BCRYPT_DSA_PRIVATE_BLOB     = "DSAPRIVATEBLOB"
//   BCRYPT_ECCPUBLIC_BLOB       = "ECCPUBLICBLOB"
//   BCRYPT_ECCPRIVATE_BLOB      = "ECCPRIVATEBLOB"
//   BCRYPT_RSAPUBLIC_BLOB       = "RSAPUBLICBLOB"
//   BCRYPT_RSAPRIVATE_BLOB      = "RSAPRIVATEBLOB"
//   BCRYPT_RSAFULLPRIVATE_BLOB  = "RSAFULLPRIVATEBLOB"
//   BCRYPT_PUBLIC_KEY_BLOB      = "PUBLICBLOB"
//   BCRYPT_PRIVATE_KEY_BLOB     = "PRIVATEBLOB"

// ─── Algorithm identifiers (UTF-16 strings) ──────────────────────
//   Hashes:    L"MD2","MD4","MD5","SHA1","SHA256","SHA384","SHA512"
//   Symmetric: L"AES", L"DES", L"3DES", L"RC2", L"RC4"
//   Signing:   L"RSA","DSA","ECDSA_P256","ECDSA_P384","ECDSA_P521"
//   KEX:       L"DH", L"ECDH_P256","ECDH_P384","ECDH_P521"
//   KDF:       L"PBKDF2", L"SP800_108_CTR_HMAC", L"SP800_56A_CONCAT"
//   Stream:    L"RNG"

// ─── BCryptSetProperty / GetProperty common property names (UTF-16) ─
//   L"ChainingMode"          (BCRYPT_CHAINING_MODE) — L"ChainingModeECB", L"ChainingModeCBC",
//                            L"ChainingModeGCM", L"ChainingModeCFB"
//   L"BlockLength"           (BCRYPT_BLOCK_LENGTH)
//   L"KeyLength"             (BCRYPT_KEY_LENGTH)
//   L"ObjectLength"          (BCRYPT_OBJECT_LENGTH)
//   L"HashDigestLength"      (BCRYPT_HASH_LENGTH)

// ─── BCryptEncrypt / Decrypt flags ───────────────────────────────
@flags
pub enum BCryptCipherFlags: u32 {
    blockPadding       = 0x00000001     // pad to next block
    padNone            = 0x00000000
    authenticatedCipherMode = 0x00200000   // GCM/CCM
    bufferIsParaminfo  = 0x00000080
}

@extern(C, "bcrypt") {
    // ── Opaque handles ────────────────────────────────────────────

    /// Algorithm provider handle (BCRYPT_ALG_HANDLE).
    pub struct BCRYPT_ALG_HANDLE {}

    /// Hash object handle (BCRYPT_HASH_HANDLE).
    pub struct BCRYPT_HASH_HANDLE {}

    /// Key handle (BCRYPT_KEY_HANDLE).
    pub struct BCRYPT_KEY_HANDLE {}

    // ── BCRYPT_AUTH_TAG_LENGTHS / BLOCK_LENGTHS — 8 bytes ────────

    pub struct BCRYPT_KEY_LENGTHS_STRUCT {
        dwMinLength: u32
        dwMaxLength: u32
        dwIncrement: u32
        _pad:        u32
    }

    pub struct BCRYPT_AUTH_TAG_LENGTHS_STRUCT {
        dwMinLength: u32
        dwMaxLength: u32
        dwIncrement: u32
        _pad:        u32
    }

    // ── BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO — 80 bytes on x64 ──

    /// AEAD parameters for GCM/CCM. Set `cbSize` = 80 and
    /// `dwInfoVersion` = 1.  `pbNonce` and `pbTag` must point to
    /// caller-allocated buffers of the right length.
    pub struct BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO {
        cbSize:        u32
        dwInfoVersion: u32
        pbNonce:       *u8
        cbNonce:       u32
        _pad0:         u32
        pbAuthData:    *u8
        cbAuthData:    u32
        _pad1:         u32
        pbTag:         *u8
        cbTag:         u32
        _pad2:         u32
        pbMacContext:  *u8
        cbMacContext:  u32
        cbAAD:         u32
        cbData:        u64
        dwFlags:       u32
        _pad3:         u32
    }

    // ── Algorithm provider ────────────────────────────────────────

    /// Opens an algorithm provider.
    /// `pszAlgId`: UTF-16 algorithm name (e.g. L"SHA256", L"AES").
    /// `pszImplementation`: NULL for the default provider.
    /// `dwFlags`: BCryptAlgFlags.* (0 for hash, .algHandleHmac for HMAC).
    @lib pub fn BCryptOpenAlgorithmProvider(
        phAlgorithm:       **BCRYPT_ALG_HANDLE,
        pszAlgId:          *const u16,
        pszImplementation: *const u16,
        dwFlags:           u32
    ): i32

    @lib pub fn BCryptCloseAlgorithmProvider(
        hAlgorithm: *BCRYPT_ALG_HANDLE,
        dwFlags:    u32
    ): i32

    // ── Property get/set ──────────────────────────────────────────

    @lib pub fn BCryptGetProperty(
        hObject:        *void,    // alg handle / key handle / hash handle
        pszProperty:    *const u16,
        pbOutput:       *u8,
        cbOutput:       u32,
        pcbResult:      *u32,
        dwFlags:        u32
    ): i32

    @lib pub fn BCryptSetProperty(
        hObject:        *void,
        pszProperty:    *const u16,
        pbInput:        *const u8,
        cbInput:        u32,
        dwFlags:        u32
    ): i32

    // ── Hashing / HMAC ────────────────────────────────────────────

    /// Creates a hash object. `pbHashObject` is caller-allocated
    /// scratch space; size from BCryptGetProperty("ObjectLength").
    /// For HMAC, pass the key in `pbSecret`/`cbSecret`.
    @lib pub fn BCryptCreateHash(
        hAlgorithm:     *BCRYPT_ALG_HANDLE,
        phHash:         **BCRYPT_HASH_HANDLE,
        pbHashObject:   *u8,
        cbHashObject:   u32,
        pbSecret:       *const u8,
        cbSecret:       u32,
        dwFlags:        u32
    ): i32

    /// Hashes more data. May be called repeatedly.
    @lib pub fn BCryptHashData(
        hHash:    *BCRYPT_HASH_HANDLE,
        pbInput:  *const u8,
        cbInput:  u32,
        dwFlags:  u32
    ): i32

    /// Finalises the hash. `pbOutput` length = HashDigestLength
    /// (e.g. 32 for SHA-256).
    @lib pub fn BCryptFinishHash(
        hHash:    *BCRYPT_HASH_HANDLE,
        pbOutput: *u8,
        cbOutput: u32,
        dwFlags:  u32
    ): i32

    @lib pub fn BCryptDestroyHash(hHash: *BCRYPT_HASH_HANDLE): i32

    /// Duplicates a hash state — useful for finalising a prefix while
    /// continuing to hash more data.
    @lib pub fn BCryptDuplicateHash(
        hHash:        *BCRYPT_HASH_HANDLE,
        phNewHash:    **BCRYPT_HASH_HANDLE,
        pbHashObject: *u8,
        cbHashObject: u32,
        dwFlags:      u32
    ): i32

    // ── Symmetric encryption (AES, etc.) ─────────────────────────

    /// Imports a raw symmetric key.
    /// `pbKeyObject`: caller scratch (ObjectLength bytes).
    /// `pbSecret`: raw key bytes (16, 24, or 32 for AES).
    @lib pub fn BCryptGenerateSymmetricKey(
        hAlgorithm:    *BCRYPT_ALG_HANDLE,
        phKey:         **BCRYPT_KEY_HANDLE,
        pbKeyObject:   *u8,
        cbKeyObject:   u32,
        pbSecret:      *const u8,
        cbSecret:      u32,
        dwFlags:       u32
    ): i32

    @lib pub fn BCryptDestroyKey(hKey: *BCRYPT_KEY_HANDLE): i32

    /// Encrypts plaintext. For GCM/CCM, pass `pPaddingInfo` →
    /// BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO and `dwFlags` = 0;
    /// for CBC with padding, pass NULL and `dwFlags` = .blockPadding.
    /// First call with `pbOutput` = NULL to size the output.
    @lib pub fn BCryptEncrypt(
        hKey:         *BCRYPT_KEY_HANDLE,
        pbInput:      *const u8,
        cbInput:      u32,
        pPaddingInfo: *const void,
        pbIV:         *u8,
        cbIV:         u32,
        pbOutput:     *u8,
        cbOutput:     u32,
        pcbResult:    *u32,
        dwFlags:      u32
    ): i32

    @lib pub fn BCryptDecrypt(
        hKey:         *BCRYPT_KEY_HANDLE,
        pbInput:      *const u8,
        cbInput:      u32,
        pPaddingInfo: *const void,
        pbIV:         *u8,
        cbIV:         u32,
        pbOutput:     *u8,
        cbOutput:     u32,
        pcbResult:    *u32,
        dwFlags:      u32
    ): i32

    // ── Key derivation ────────────────────────────────────────────

    /// PBKDF2-HMAC.  `hPrf`: an HMAC alg handle (e.g. SHA256 with HMAC flag).
    @lib pub fn BCryptDeriveKeyPBKDF2(
        hPrf:          *BCRYPT_ALG_HANDLE,
        pbPassword:    *const u8,
        cbPassword:    u32,
        pbSalt:        *const u8,
        cbSalt:        u32,
        cIterations:   u64,
        pbDerivedKey:  *u8,
        cbDerivedKey:  u32,
        dwFlags:       u32
    ): i32

    // ── RNG ───────────────────────────────────────────────────────

    /// Fills `pbBuffer` with cryptographically secure random bytes.
    /// `hAlgorithm`: NULL to use the default RNG provider.
    /// `dwFlags`: 0, or BCRYPT_USE_SYSTEM_PREFERRED_RNG (0x00000002)
    ///            (in that case hAlgorithm must be NULL).
    @lib pub fn BCryptGenRandom(
        hAlgorithm: *BCRYPT_ALG_HANDLE,
        pbBuffer:   *u8,
        cbBuffer:   u32,
        dwFlags:    u32
    ): i32

    // ── Signing / verification ────────────────────────────────────

    /// Signs `pbInput` (typically a hash) with the private key in `hKey`.
    /// `pPaddingInfo`: for RSA, *BCRYPT_PKCS1_PADDING_INFO or
    /// *BCRYPT_PSS_PADDING_INFO (cast as *void).
    /// `dwFlags`: BCRYPT_PAD_PKCS1=2, BCRYPT_PAD_PSS=8.
    @lib pub fn BCryptSignHash(
        hKey:         *BCRYPT_KEY_HANDLE,
        pPaddingInfo: *const void,
        pbInput:      *const u8,
        cbInput:      u32,
        pbOutput:     *u8,
        cbOutput:     u32,
        pcbResult:    *u32,
        dwFlags:      u32
    ): i32

    @lib pub fn BCryptVerifySignature(
        hKey:         *BCRYPT_KEY_HANDLE,
        pPaddingInfo: *const void,
        pbHash:       *const u8,
        cbHash:       u32,
        pbSignature:  *const u8,
        cbSignature:  u32,
        dwFlags:      u32
    ): i32

    // ── Key import / export ───────────────────────────────────────

    @lib pub fn BCryptExportKey(
        hKey:         *BCRYPT_KEY_HANDLE,
        hExportKey:   *BCRYPT_KEY_HANDLE,
        pszBlobType:  *const u16,
        pbOutput:     *u8,
        cbOutput:     u32,
        pcbResult:    *u32,
        dwFlags:      u32
    ): i32

    @lib pub fn BCryptImportKey(
        hAlgorithm:   *BCRYPT_ALG_HANDLE,
        hImportKey:   *BCRYPT_KEY_HANDLE,
        pszBlobType:  *const u16,
        phKey:        **BCRYPT_KEY_HANDLE,
        pbKeyObject:  *u8,
        cbKeyObject:  u32,
        pbInput:      *const u8,
        cbInput:      u32,
        dwFlags:      u32
    ): i32

    @lib pub fn BCryptImportKeyPair(
        hAlgorithm:  *BCRYPT_ALG_HANDLE,
        hImportKey:  *BCRYPT_KEY_HANDLE,
        pszBlobType: *const u16,
        phKey:       **BCRYPT_KEY_HANDLE,
        pbInput:     *const u8,
        cbInput:     u32,
        dwFlags:     u32
    ): i32
}
