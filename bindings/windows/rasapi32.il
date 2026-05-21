/// rasapi32.dll — Remote Access Service (RAS): dial-up + VPN.
///
/// Modern code uses VPNv2 CSP / NRPT instead, but RAS remains the
/// flat-API way to query / create / dial / hang up classic VPN
/// connections from a desktop app.
///
/// Pattern (enumerate active connections):
///   let conns: RASCONNW[8] = ...
///   conns[0].dwSize = sizeof(RASCONNW)            // 412 on x64
///   let cb: u32 = 8 * 412
///   let n:  u32 = 0
///   RasEnumConnectionsW(conns, &cb, &n)

@flags
pub enum RasFlags: u32 {
    useCustomScripting = 0x00000001
    useSwitchInf       = 0x00000002
    skipPwdRetrieval   = 0x00000004
    paused             = 0x00000008
    connected          = 0x00000010
}

@extern(C, "rasapi32") {
    /// Connection handle (HRASCONN). Treat as opaque.
    @handle pub struct HRASCONN {}

    /// `RASCONNW` — 412 bytes on x64. Truncated representation of
    /// active connection info.
    pub struct RASCONNW {
        dwSize:           u32
        _pad:             u32
        hrasconn:         HRASCONN
        szEntryName:      u16[257]
        szDeviceType:     u16[17]
        szDeviceName:     u16[129]
        szPhonebook:      u16[260]
        dwSubEntry:       u32
        guidEntry:        u8[16]
        dwFlags:          u32
        luid:             u8[8]            // LUID
        guidCorrelationId: u8[16]
    }

    /// `RASDIALPARAMSW` — 1064 bytes on x64. Truncated — we expose
    /// only the fields callers commonly touch.
    pub struct RASDIALPARAMSW {
        dwSize:           u32
        _pad:             u32
        szEntryName:      u16[257]
        szPhoneNumber:    u16[129]
        szCallbackNumber: u16[129]
        szUserName:       u16[257]
        szPassword:       u16[257]
        szDomain:         u16[16]
        dwSubEntry:       u32
        dwCallbackId:     u64
    }

    /// `RASENTRYW` is huge (3072 bytes); we expose it as a
    /// padding-only struct since callers usually pass NULL or
    /// fill it field-by-field via byte-wise writes.
    pub struct RASENTRYW {
        _opaque: u8[3072]
    }

    // ── Enumeration ──────────────────────────────────────────────

    /// `RasEnumConnectionsW(rgrasconn, &cb, &count)`. `cb` is in/out
    /// (size in bytes). On first call pass cb = sizeof(RASCONNW),
    /// rgrasconn[0].dwSize = sizeof(RASCONNW). Returns ERROR_SUCCESS (0).
    @lib pub fn RasEnumConnectionsW(
        rgrasconn: *RASCONNW,
        lpcb:      *u32,
        lpcConnections: *u32
    ): u32

    /// Enumerate phonebook entries.
    @lib pub fn RasEnumEntriesW(
        reserved:   *const u16,
        lpszPhonebook: *const u16,
        lprasentryname: *void,           // *RASENTRYNAMEW
        lpcb:           *u32,
        lpcEntries:     *u32
    ): u32

    // ── Connection control ──────────────────────────────────────

    /// `RasDialW(extension, phonebook, params, notifierType,
    ///           notifier, &handle)`. `notifierType`: 0 = no
    /// notification, 1 = notifier is a HWND, 2 = notifier is
    /// a callback function pointer.
    @lib pub fn RasDialW(
        lpRasDialExtensions: *void,
        lpszPhonebook:       *const u16,
        lpRasDialParams:     *RASDIALPARAMSW,
        dwNotifierType:      u32,
        lpvNotifier:         *void,
        lphRasConn:          *HRASCONN
    ): u32

    @lib pub fn RasHangUpW(hrasconn: HRASCONN): u32

    @lib pub fn RasGetConnectStatusW(
        hrasconn:        HRASCONN,
        lprasconnstatus: *void        // *RASCONNSTATUSW (160 bytes)
    ): u32

    /// Last error from the dialer with friendly text.
    @lib pub fn RasGetErrorStringW(
        uErrorValue:      u32,
        lpszErrorString:  *u16,
        cBufSize:         u32
    ): u32

    // ── Phonebook entry CRUD ────────────────────────────────────

    @lib pub fn RasGetEntryPropertiesW(
        lpszPhonebook: *const u16,
        lpszEntry:     *const u16,
        lpRasEntry:    *RASENTRYW,
        lpdwEntryInfoSize: *u32,
        lpbDeviceConfig: *void,
        lpdwDeviceConfigSize: *u32
    ): u32

    @lib pub fn RasSetEntryPropertiesW(
        lpszPhonebook: *const u16,
        lpszEntry:     *const u16,
        lpRasEntry:    *const RASENTRYW,
        dwEntryInfoSize: u32,
        lpbDeviceConfig: *const void,
        dwDeviceConfigSize: u32
    ): u32

    @lib pub fn RasDeleteEntryW(
        lpszPhonebook: *const u16,
        lpszEntry:     *const u16
    ): u32

    @lib pub fn RasRenameEntryW(
        lpszPhonebook: *const u16,
        lpszOldEntry:  *const u16,
        lpszNewEntry:  *const u16
    ): u32

    @lib pub fn RasValidateEntryNameW(
        lpszPhonebook: *const u16,
        lpszEntry:     *const u16
    ): u32

    // ── Phonebook dialog ────────────────────────────────────────

    /// Pop the standard "Connect To" dialog. Returns non-zero when
    /// the user dialed in successfully.
    @lib pub fn RasDialDlgW(
        lpszPhonebook: *const u16,
        lpszEntry:     *const u16,
        lpszPhoneNumber: *const u16,
        lpInfo:        *void          // *RASDIALDLG, may be 0
    ): i32

    // ── Stats ───────────────────────────────────────────────────

    /// 80 bytes — incoming/outgoing bytes + frames etc.
    @lib pub fn RasGetConnectionStatistics(
        hrasconn:        HRASCONN,
        lpStatistics:    *void       // *RAS_STATS
    ): u32

    @lib pub fn RasClearConnectionStatistics(hrasconn: HRASCONN): u32
}
