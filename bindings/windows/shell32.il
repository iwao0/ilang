/// shell32.dll — Windows Shell: ShellExecute, folder paths, file
/// operations, drag-and-drop, and the system tray notification area.

// ─── ShellExecute return / error codes ───────────────────────────
// Return value of ShellExecuteA is HINSTANCE cast to intptr.
// Values ≤ 32 indicate failure; values > 32 are success (hwnd).
pub enum ShellExecuteError: i32 {
    oom          = 0    // out of memory / resource
    notFound     = 2    // file not found
    pathNotFound = 3    // path not found
    badFormat    = 11   // executable is invalid
    accessDenied = 5    // access denied
    noAssociation = 31  // no app associated with extension
    dllNotFound  = 32   // DLL not found
}

// ─── CSIDL — special folder IDs for SHGetFolderPathA ─────────────
pub enum CSIDL: i32 {
    desktop              = 0x0000
    internet             = 0x0001
    programs             = 0x0002   // Start Menu\Programs
    controls             = 0x0003   // Control Panel
    printers             = 0x0004
    personal             = 0x0005   // My Documents
    favorites            = 0x0006
    startup              = 0x0007
    recent               = 0x0008
    sendto               = 0x0009
    recyclebinFolder     = 0x000A
    startMenu            = 0x000B
    myDocuments          = 0x000C   // = personal
    myMusic              = 0x000D
    myVideo              = 0x000E
    deskTopDirectory     = 0x0010
    drives               = 0x0011   // My Computer
    network              = 0x0012
    nethood              = 0x0013
    fonts                = 0x0014
    templates            = 0x0015
    commonStartMenu      = 0x0016
    commonPrograms       = 0x0017
    commonStartup        = 0x0018
    commonDesktopDirectory = 0x0019
    appdata              = 0x001A   // %APPDATA%
    printhood            = 0x001B
    localAppdata         = 0x001C   // %LOCALAPPDATA%
    altStartup           = 0x001D
    commonAltStartup     = 0x001E
    commonFavorites      = 0x001F
    internetCache        = 0x0020
    cookies              = 0x0021
    history              = 0x0022
    commonAppdata        = 0x0023   // ProgramData
    windows              = 0x0024
    system               = 0x0025
    programFiles         = 0x0026
    myPictures           = 0x0027
    profile              = 0x0028   // user profile root
    systemX86            = 0x0029
    programFilesX86      = 0x002A
    programFilesCommon   = 0x002B
    programFilesCommonX86 = 0x002C
    commonTemplates      = 0x002D
    commonDocuments      = 0x002E
    commonAdminTools     = 0x002F
    adminTools           = 0x0030
    connections          = 0x0031
    commonMusic          = 0x0035
    commonPictures       = 0x0036
    commonVideo          = 0x0037
    resources            = 0x0038
    resourcesLocalized   = 0x0039
    commonOemLinks       = 0x003A
    cdBurnArea           = 0x003B
    computersnearme      = 0x003D
    /// OR with any CSIDL to create the folder if it doesn't exist.
    flagCreate           = 0x8000
    /// OR with any CSIDL to get the "no-alias" path.
    flagDontVerify       = 0x4000
}

// ─── SHFileOperation flags (FOF_*) ───────────────────────────────
@flags
pub enum FileOpFlags: u32 {
    multidestfiles      = 0x0001
    confirmmouse        = 0x0002
    silent              = 0x0004   // no progress dialog
    renameOnCollision   = 0x0008
    noConfirmation      = 0x0010   // yes to all
    wantMappingHandle   = 0x0020
    allowUndo           = 0x0040
    filesOnly           = 0x0080
    simpleprogress      = 0x0100
    noconfirmmkdir      = 0x0200
    noErrorUI           = 0x0400
    nocopysecurityattribs = 0x0800
    norecursion         = 0x1000
    noConnectedElements = 0x2000
    wantNukeWarning     = 0x4000
    noRecursereparse    = 0x8000
}

// ─── SHFileOperation wFunc values ────────────────────────────────
pub enum FileOpFunc: u32 {
    move   = 0x0001
    copy   = 0x0002
    delete = 0x0003
    rename = 0x0004
}

// ─── System tray (Shell_NotifyIcon) message values (NIM_*) ───────
pub enum NotifyIconMessage: u32 {
    add     = 0x00000000
    modify  = 0x00000001
    delete  = 0x00000002
    setFocus = 0x00000003
    setVersion = 0x00000004
}

// ─── NOTIFYICONDATAA flags (NIF_*) ───────────────────────────────
@flags
pub enum NotifyIconDataFlags: u32 {
    message  = 0x00000001
    icon     = 0x00000002
    tip      = 0x00000004
    state    = 0x00000008
    info     = 0x00000010
    guid     = 0x00000020
    realtime = 0x00000040
    showtip  = 0x00000080
}

use winnt { HANDLE }
use windef { HICON, HINSTANCE, HWND, POINT }
@extern(C, "shell32") {
    // ── Shell handle type ─────────────────────────────────────────

    /// Drag-and-drop data handle (HDROP). Passed as wParam in
    /// WM_DROPFILES; pass to DragQueryFileA / DragFinish.
    @handle pub struct HDROP {}

    // ── SHFILEOPERATIONA struct — 36 bytes on x64 ─────────────────

    /// Describes a file-system operation for `SHFileOperationA`.
    /// `pFrom` and `pTo` are double-NUL-terminated string lists.
    pub struct SHFILEOPERATIONA {
        hwnd:                  HWND
        wFunc:                 u32      // FileOpFunc
        // 4 bytes implicit padding before pointer
        pFrom:                 *const char
        pTo:                   *const char
        fFlags:                u16      // FileOpFlags
        fAnyOperationsAborted: i32
        // 2 bytes padding before pointer
        hNameMappings:         *void
        lpszProgressTitle:     *const char
    }

    // ── NOTIFYICONDATAA — 956 bytes; use cbSize=956 on Vista+ ─────
    // Simplified: expose only the most-used first fields; the
    // large szTip and szInfo buffers are opaque padding here.
    // Allocate with HeapAlloc(GetProcessHeap(), 8, 956) and cast.

    // ── Execute ───────────────────────────────────────────────────

    /// Opens or launches a file/URL. `lpOperation`: "open", "edit",
    /// "explore", "print", "runas", or NULL for the default verb.
    /// `nShowCmd`: ShowCmd.show etc. Returns a pseudo-HINSTANCE;
    /// values > 32 mean success.
    @lib pub fn ShellExecuteA(
        hwnd:        HWND,
        lpOperation: *const char,
        lpFile:      *const char,
        lpParameters: *const char,
        lpDirectory: *const char,
        nShowCmd:    i32
    ): i64
    @lib pub fn ShellExecuteW(
        hwnd:        HWND,
        lpOperation: *const u16,
        lpFile:      *const u16,
        lpParameters: *const u16,
        lpDirectory: *const u16,
        nShowCmd:    i32
    ): i64

    // ── Folder paths ──────────────────────────────────────────────

    /// Fills `pszPath` (MAX_PATH = 260 bytes) with the path of the
    /// CSIDL folder. `hToken`: 0 for the current user.
    /// `dwFlags`: 0 = return verified path; CSIDL.flagCreate to create.
    @lib pub fn SHGetFolderPathA(
        hwnd:    HWND,
        csidl:   i32,          // CSIDL.*
        hToken:  HANDLE,       // token, or 0
        dwFlags: u32,
        pszPath: *char         // output buffer, at least MAX_PATH (260)
    ): i32
    @lib pub fn SHGetFolderPathW(
        hwnd:    HWND,
        csidl:   i32,          // CSIDL.*
        hToken:  HANDLE,       // token, or 0
        dwFlags: u32,
        pszPath: *u16         // output buffer, at least MAX_PATH (260)
    ): i32

    // ── File operations ───────────────────────────────────────────

    /// Copies, moves, renames, or deletes files or folders.
    /// Returns 0 on success; non-zero error codes are Win32
    /// error codes. Fills `fAnyOperationsAborted` in the struct.
    @lib pub fn SHFileOperationA(lpFileOp: *SHFILEOPERATIONA): i32
    @lib pub fn SHFileOperationW(lpFileOp: *SHFILEOPERATIONA): i32

    // ── Drag and drop ─────────────────────────────────────────────

    /// Registers `hwnd` to accept WM_DROPFILES messages when
    /// files are dragged onto the window.
    /// Call `DragAcceptFiles(hwnd, 0)` to unregister.
    @lib pub fn DragAcceptFiles(hwnd: HWND, fAccept: i32)

    /// Returns the number of files in the drop, or 0xFFFFFFFF if
    /// `iFile` is 0xFFFFFFFF (returns count without copying).
    /// `lpszFile`: buffer of `cch` chars, or NULL to get length.
    @lib pub fn DragQueryFileA(
        hDrop:   HDROP,
        iFile:   u32,
        lpszFile: *char,
        cch:     u32
    ): u32
    @lib pub fn DragQueryFileW(
        hDrop:   HDROP,
        iFile:   u32,
        lpszFile: *u16,
        cch:     u32
    ): u32

    /// Releases memory allocated by the system for `hDrop`.
    /// Always call this after processing WM_DROPFILES.
    @lib pub fn DragFinish(hDrop: HDROP)

    /// Drop-point query. `lppt` is filled with the cursor position
    /// in the receiving window's client coordinates. Returns
    /// non-zero when the point lies inside the receiving window's
    /// client area.
    @lib pub fn DragQueryPoint(hDrop: HDROP, lppt: *POINT): i32

    // ── System tray ───────────────────────────────────────────────

    /// Adds, modifies, or removes a tray icon.
    /// `dwMessage`: NotifyIconMessage.*, `lpdata` points to a
    /// NOTIFYICONDATAA struct with `cbSize` set correctly.
    @lib pub fn Shell_NotifyIconA(dwMessage: u32, lpdata: *void): i32
    @lib pub fn Shell_NotifyIconW(dwMessage: u32, lpdata: *void): i32

    // ── Folder path helpers (extended) ───────────────────────────

    /// Legacy CSIDL → path. Same as SHGetFolderPathA with hToken
    /// = 0; kept for Win9x compatibility code.
    @lib pub fn SHGetSpecialFolderPathA(
        hwnd:    HWND,
        pszPath: *char,
        csidl:   i32,
        fCreate: i32
    ): i32
    @lib pub fn SHGetSpecialFolderPathW(
        hwnd:    HWND,
        pszPath: *u16,
        csidl:   i32,
        fCreate: i32
    ): i32

    /// `SHGetKnownFolderPath(rfid, flags, hToken, ppszPath)` —
    /// modern (Vista+) folder-id API. `rfid` is a KNOWNFOLDERID
    /// GUID pointer. The returned `*ppszPath` is a wide string
    /// allocated by the shell; free with `CoTaskMemFree`.
    @lib pub fn SHGetKnownFolderPath(
        rfid:     *const void,    // *const GUID — pass ole32.GUID
        dwFlags:  u32,
        hToken:   HANDLE,
        ppszPath: **u16
    ): i32

    /// Create a directory and any missing parents.
    @lib pub fn SHCreateDirectoryExA(
        hwnd:    HWND,
        pszPath: *const char,
        psa:     *void           // *SECURITY_ATTRIBUTES, may be 0
    ): i32
    @lib pub fn SHCreateDirectoryExW(
        hwnd:    HWND,
        pszPath: *const u16,
        psa:     *void           // *SECURITY_ATTRIBUTES, may be 0
    ): i32

    // ── Recycle bin ──────────────────────────────────────────────

    /// Empties the recycle bin on `pszRootPath` (or all drives when
    /// `pszRootPath` is NULL). `dwFlags`: 1 = no confirmation,
    /// 2 = no progress, 4 = no sound.
    @lib pub fn SHEmptyRecycleBinA(
        hwnd:        HWND,
        pszRootPath: *const char,
        dwFlags:     u32
    ): i32
    @lib pub fn SHEmptyRecycleBinW(
        hwnd:        HWND,
        pszRootPath: *const u16,
        dwFlags:     u32
    ): i32

    /// Item count + total size of the recycle bin.
    pub struct SHQUERYRBINFO {
        cbSize:       u32
        i64NumItems:  i64
        i64Size:      i64
    }

    @lib pub fn SHQueryRecycleBinA(
        pszRootPath: *const char,
        pSHQueryRBInfo: *SHQUERYRBINFO
    ): i32
    @lib pub fn SHQueryRecycleBinW(
        pszRootPath: *const u16,
        pSHQueryRBInfo: *SHQUERYRBINFO
    ): i32

    // ── Icons ────────────────────────────────────────────────────

    /// Pull large + small icon handles out of an .exe / .dll /
    /// .ico file. Returns the number of icons in the file when
    /// `phiconLarge` / `phiconSmall` are NULL.
    @lib pub fn ExtractIconExA(
        lpszFile:    *const char,
        nIconIndex:  i32,
        phiconLarge: *i64,
        phiconSmall: *i64,
        nIcons:      u32
    ): u32
    @lib pub fn ExtractIconExW(
        lpszFile:    *const u16,
        nIconIndex:  i32,
        phiconLarge: *i64,
        phiconSmall: *i64,
        nIcons:      u32
    ): u32

    /// `ExtractAssociatedIconA` — first icon associated with a
    /// file (looks up the extension's ProgID).
    @lib pub fn ExtractAssociatedIconA(
        hInst:      HINSTANCE,
        pszIconPath: *char,
        piIcon:     *u16
    ): HICON
    @lib pub fn ExtractAssociatedIconW(
        hInst:      HINSTANCE,
        pszIconPath: *u16,
        piIcon:     *u16
    ): HICON

    // ── Privilege / admin checks ─────────────────────────────────

    /// True if the calling process is a member of the local
    /// Administrators group with an elevated token. Wrapper around
    /// `CheckTokenMembership(NULL, AdministratorsAlias, &b)`.
    @lib pub fn IsUserAnAdmin(): i32

    // ── Command-line parsing ─────────────────────────────────────

    /// Win32's own argv parser. Splits a wide command line into an
    /// argv-shaped array. Free the result with kernel32.LocalFree.
    @lib pub fn CommandLineToArgvW(
        lpCmdLine: *const u16,
        pNumArgs:  *i32
    ): **u16

    // ── Path-from-IDList (PIDL) helpers ──────────────────────────

    /// Convert a PIDL (item id list) into a filesystem path.
    @lib pub fn SHGetPathFromIDListA(pidl: *const void, pszPath: *char): i32
    @lib pub fn SHGetPathFromIDListW(pidl: *const void, pszPath: *u16): i32
}
