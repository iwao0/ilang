/// credui.dll — Credentials UI prompts.
///
/// The headline call is `CredUIPromptForCredentialsW` — pops the
/// standard "enter your credentials" dialog and returns the entered
/// username + password. The newer `CredUIPromptForWindowsCredentialsW`
/// uses the unified Win10 credential picker.

// ─── CredUI flags (CREDUI_FLAGS_*) ───────────────────────────────
@flags
pub enum CredUiFlags: u32 {
    incorrectPassword = 0x00001
    doNotPersist      = 0x00002
    requestAdminstrator = 0x00004
    excludeCertificates = 0x00008
    requireCertificate  = 0x00010
    showSaveCheckBox    = 0x00040
    alwaysShowUi        = 0x00080
    requireSmartcard    = 0x00100
    passwordOnlyOk      = 0x00200
    validateUsername    = 0x00400
    completeUsername    = 0x00800
    persist             = 0x01000
    serverCredential    = 0x04000
    expectConfirmation  = 0x20000
    genericCredentials  = 0x40000
    usernameTargetCredentials = 0x80000
    keepUsername        = 0x100000
}

// ─── Return codes ────────────────────────────────────────────────
pub enum CredUiResult: u32 {
    noError      = 0
    cancelled    = 1223      // ERROR_CANCELLED
    noSuchLogon  = 1311
    invalidParameter = 87
    insufficientBuffer = 122
}

use user32 { HWND }

@extern(C, "credui") {
    /// `CREDUI_INFOW` — visual customisation for the prompt.
    /// All fields except `cbSize` (must be 40 on x64) are optional.
    pub struct CREDUI_INFOW {
        cbSize:         u32
        hwndParent:     HWND                  // *HWND
        pszMessageText: *const u16
        pszCaptionText: *const u16
        hbmBanner:      i64                  // *HBITMAP — optional
    }

    /// Wide-string credential prompt. `pszTargetName` doubles as
    /// the registry key for `persist` mode. `pszUserName` (cap
    /// `ulUserNameMaxChars`) and `pszPassword` (cap `ulPasswordMaxChars`)
    /// are filled on OK; the password buffer is wiped on cancel.
    /// `pfSave` is the state of the "remember" checkbox.
    @lib pub fn CredUIPromptForCredentialsW(
        pUiInfo:        *const CREDUI_INFOW,
        pszTargetName:  *const u16,
        pContext:       *void,
        dwAuthError:    u32,
        pszUserName:    *u16,
        ulUserNameMaxChars: u32,
        pszPassword:    *u16,
        ulPasswordMaxChars: u32,
        pfSave:         *i32,
        dwFlags:        u32
    ): u32

    /// Modern (Vista+) credentials picker that integrates with
    /// Windows Hello / smart cards. Returns the credential as a
    /// blob in `*ppvOutAuthBuffer` (caller frees with `CoTaskMemFree`).
    @lib pub fn CredUIPromptForWindowsCredentialsW(
        pUiInfo:           *const CREDUI_INFOW,
        dwAuthError:       u32,
        pulAuthPackage:    *u32,
        pvInAuthBuffer:    *const void,
        ulInAuthBufferSize: u32,
        ppvOutAuthBuffer:  **void,
        pulOutAuthBufferSize: *u32,
        pfSave:            *i32,
        dwFlags:           u32
    ): u32

    /// Parse a target string like "DOMAIN\user" → `(domain, user)`.
    @lib pub fn CredUIParseUserNameW(
        UserName:    *const u16,
        user:        *u16,
        userBufferSize: u32,
        domain:      *u16,
        domainBufferSize: u32
    ): u32

    /// Verify that a target name conforms to the persisted-cred form.
    @lib pub fn CredUIStoreSSOCredW(
        pszRealm:    *const u16,
        pszUsername: *const u16,
        pszPassword: *const u16,
        bPersist:    i32
    ): u32

    /// Confirm or discard the credentials the user entered.
    @lib pub fn CredUIConfirmCredentialsW(
        pszTargetName: *const u16,
        bConfirm:      i32
    ): u32
}
