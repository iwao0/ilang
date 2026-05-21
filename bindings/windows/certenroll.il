/// certenroll.dll — Certificate Enrollment COM API (Vista+).
///
/// Surface is entirely COM — no flat C exports. Generate a CSR
/// (certificate signing request), submit it to a CA, install the
/// resulting cert. Used by AD CS / Intune SCEP / smart-card
/// enrollment.
///
/// Bring-up:
///   CoCreateInstance(&CLSID_CObjectId, NULL, CLSCTX_INPROC_SERVER,
///                    &IID_IObjectId, &oid)
///   oid.InitializeFromName(XCN_OID_RSA_RSA)
///   CoCreateInstance(&CLSID_CX509PrivateKey, ...)
///   privKey.set_Length(2048)
///   privKey.set_ProviderName(L"Microsoft Software Key Storage Provider")
///   privKey.Create()
///   CoCreateInstance(&CLSID_CX509CertificateRequestPkcs10, ...)
///   req.InitializeFromPrivateKey(...)
///   req.set_Subject(distinguishedName)
///   req.Encode()
///   let csr = req.RawData(EncodingType.base64)

// ─── X509CertificateEnrollmentContext (request scope) ────────────
pub enum X509CertEnrollContext: i32 {
    none           = 0
    user           = 0x1
    machine        = 0x2
    administratorForceMachine = 0x3
}

// ─── EncodingType — for IXmlCertEncoding etc. ────────────────────
pub enum CertEncoding: i32 {
    base64Header   = 0
    base64         = 1
    binary         = 2
    base64RequestHeader = 3
    hex            = 4
    hexAscii       = 5
    base64Any      = 6
    any            = 7
    hexAddr        = 8
    hexAsciiAddr   = 9
    hexRaw         = 10
}

// ─── X509KeySpec — which key class ──────────────────────────────
pub enum X509KeySpec: i32 {
    none         = 0
    keyExchange  = 1
    signature    = 2
}

// ─── KEY_USAGE_PROPERTY values ───────────────────────────────────
@flags
pub enum X509KeyUsage: u32 {
    digitalSignature  = 0x80
    nonRepudiation    = 0x40
    keyEncipherment   = 0x20
    dataEncipherment  = 0x10
    keyAgreement      = 0x08
    keyCertSign       = 0x04
    cRLSign           = 0x02
    encipherOnly      = 0x01
    decipherOnly      = 0x8000
}

// ─── EnrollmentEnrollStatus ──────────────────────────────────────
pub enum EnrollmentEnrollStatus: u32 {
    success           = 0x1
    subjectsRequired  = 0x2
    fail              = 0x10
    pending           = 0x20
    skipped           = 0x40
    denied            = 0x100
    error             = 0x10000
}

/// Documentation-only — no C entry points. Wire via CoCreateInstance.
///
/// CLSIDs (commonly needed):
///   CLSID_CObjectId                       = {884E2000-217D-11DA-B2A4-000E7BBB2B09}
///   CLSID_CObjectIds                      = {884E2002-217D-11DA-B2A4-000E7BBB2B09}
///   CLSID_CX500DistinguishedName          = {884E2007-217D-11DA-B2A4-000E7BBB2B09}
///   CLSID_CX509PrivateKey                 = {884E2017-217D-11DA-B2A4-000E7BBB2B09}
///   CLSID_CX509CertificateRequestPkcs10   = {884E202C-217D-11DA-B2A4-000E7BBB2B09}
///   CLSID_CX509CertificateRequestCmc      = {884E202E-217D-11DA-B2A4-000E7BBB2B09}
///   CLSID_CX509Enrollment                 = {884E2029-217D-11DA-B2A4-000E7BBB2B09}
///   CLSID_CX509EnrollmentWebClassFactory  = {884E202A-217D-11DA-B2A4-000E7BBB2B09}
///   CLSID_CX509ExtensionKeyUsage          = {884E2018-217D-11DA-B2A4-000E7BBB2B09}
///   CLSID_CX509ExtensionEnhancedKeyUsage  = {884E2019-217D-11DA-B2A4-000E7BBB2B09}
///   CLSID_CX509ExtensionTemplate          = {884E2026-217D-11DA-B2A4-000E7BBB2B09}
///   CLSID_CX509ExtensionTemplateName      = {884E2025-217D-11DA-B2A4-000E7BBB2B09}
///
/// IIDs:
///   IID_IObjectId                         = {728AB300-217D-11DA-B2A4-000E7BBB2B09}
///   IID_IX509PrivateKey                   = {728AB320-217D-11DA-B2A4-000E7BBB2B09}
///   IID_IX509CertificateRequestPkcs10     = {728AB344-217D-11DA-B2A4-000E7BBB2B09}
///   IID_IX509Enrollment                   = {728AB348-217D-11DA-B2A4-000E7BBB2B09}
///
/// Well-known OIDs:
///   XCN_OID_RSA_RSA               = "1.2.840.113549.1.1.1"
///   XCN_OID_ECC_PUBLIC_KEY        = "1.2.840.10045.2.1"
///   XCN_OID_PKIX_KP_SERVER_AUTH   = "1.3.6.1.5.5.7.3.1"
///   XCN_OID_PKIX_KP_CLIENT_AUTH   = "1.3.6.1.5.5.7.3.2"
///   XCN_OID_PKIX_KP_CODE_SIGNING  = "1.3.6.1.5.5.7.3.3"
///   XCN_OID_PKIX_KP_EMAIL_PROTECTION = "1.3.6.1.5.5.7.3.4"
// (No flat functions — the surface is entirely COM.)
