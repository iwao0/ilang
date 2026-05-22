/// cryptui.dll — Certificate UI dialogs (import / select / view /
/// trust prompt).
///
/// Companion to crypt32.dll which exposes the non-UI primitives.

@flags
pub enum CryptUiSelCertFlags: u32 {
    none                  = 0
    multiSelect           = 1
    showSelectionUi       = 2
    enableEditCertCb      = 4
    enableFilterCb        = 8
    enableViewIssueButton = 16
}

@flags
pub enum CryptUiWizImportFlags: u32 {
    none                = 0
    subjectIsCertContext = 0x00000001
    subjectIsFile       = 0x00000002
    importToLocalMachine = 0x00100000
    importToCurrentUser = 0x00200000
    allowCertLocation   = 0x00010000
    noSizeCheck         = 0x00020000
    allowAlertWindowsTitle = 0x00080000
    bareCert            = 0x00040000
}

@flags
pub enum CryptUiWizExportFlags: u32 {
    none                = 0
    exportPrivateKeys   = 0x00000001
    noDeleteAfterExport = 0x00000002
    ignoreErr           = 0x00000004
}

use user32 { HWND }
use crypt32 { HCERTSTORE }

@extern(C, "cryptui") {
    /// CRYPTUI_VIEWCERTIFICATE_STRUCTA — pass to
    /// `CryptUIDlgViewCertificateA` to show the standard
    /// certificate viewer. Fields are simplified — the real
    /// struct has 64 bytes of additional template / property pages.
    pub struct CRYPTUI_VIEWCERTIFICATE_STRUCTA {
        dwSize:              u32
        _pad:                u32
        hwndParent:          HWND
        dwFlags:             u32
        szTitle:             *const char
        pCertContext:        *const void   // *CERT_CONTEXT
        rgszPurposes:        **const char
        cPurposes:           u32
        _pad2:               u32
        pCryptProviderData:  *const void
        fpCryptProviderDataTrustedUsage: i32
        idxSigner:           u32
        idxCert:             u32
        fCounterSigner:      i32
        idxCounterSigner:    u32
        cStores:             u32
        _pad3:               u32
        rghStores:           *const HCERTSTORE
        cPropSheetPages:     u32
        _pad4:               u32
        rgPropSheetPages:    *const void
        nStartPage:          u32
    }

    /// Standard "View Certificate" dialog.
    @lib pub fn CryptUIDlgViewCertificateA(
        pCertViewInfo:    *const CRYPTUI_VIEWCERTIFICATE_STRUCTA,
        pfPropertiesChanged: *i32
    ): i32
    @lib pub fn CryptUIDlgViewCertificateW(
        pCertViewInfo:    *const CRYPTUI_VIEWCERTIFICATE_STRUCTA,
        pfPropertiesChanged: *i32
    ): i32

    /// `CryptUIDlgSelectCertificate*` — pick a cert from a store
    /// or from the current user's "My" store. Returns the chosen
    /// `*CERT_CONTEXT` or NULL on cancel.
    @lib pub fn CryptUIDlgSelectCertificateFromStore(
        hCertStore:    HCERTSTORE,
        hwnd:          HWND,
        pwszTitle:     *const u16,
        pwszDisplayString: *const u16,
        dwDontUseColumn: u32,
        dwFlags:       u32,
        pvReserved:    *void
    ): *void              // *CERT_CONTEXT

    /// "Add to Trusted Roots" prompt.
    @lib pub fn CryptUIWizImport(
        dwFlags:        u32,
        hwndParent:     HWND,
        pwszWizardTitle: *const u16,
        pImportSrc:     *const void,        // *CRYPTUI_WIZ_IMPORT_SRC_INFO
        hDestCertStore: HCERTSTORE
    ): i32

    /// Export to .pfx / .cer / .p7b.
    @lib pub fn CryptUIWizExport(
        dwFlags:        u32,
        hwndParent:     HWND,
        pwszWizardTitle: *const u16,
        pExportInfo:    *const void,        // *CRYPTUI_WIZ_EXPORT_INFO
        pvoid:          *const void
    ): i32

    /// Show "Generate cert request" wizard.
    @lib pub fn CryptUIWizFreeCertRequest(pCertRequest: *void): i32

    /// Show "View Certificate Properties" dialog.
    @lib pub fn CryptUIDlgViewContext(
        dwContextType:  u32,                // CERT_STORE_CERTIFICATE_CONTEXT etc.
        pvContext:      *const void,
        hwnd:           HWND,
        pwszTitle:      *const u16,
        dwFlags:        u32,
        pvReserved:     *void
    ): i32

    /// Build a `*CERT_CHAIN_CONTEXT` and show the chain in a
    /// dialog.
    @lib pub fn CryptUIDlgCertMgr(
        pCryptUICertMgr: *const void        // *CRYPTUI_CERT_MGR_STRUCT
    ): i32
}
