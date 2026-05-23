/// userenv.dll — User profile and environment management.
/// Loads user environments, locates user profile directories,
/// and creates the environment block used by CreateProcess for
/// running code as a different user.

use winnt { HANDLE }
@extern(C, "userenv") {
    // ── User profile paths ────────────────────────────────────────

    /// Returns the path to the user-profile directory for the token.
    /// On output `lpcchSize` is the actual character count written
    /// (including NUL).  Pass `0 as *char` for `lpProfileDir` to
    /// query the required size.
    @lib pub fn GetUserProfileDirectoryA(
        hToken:       HANDLE,  // open process token (TOKEN_QUERY)
        lpProfileDir: *char,
        lpcchSize:    *u32
    ): i32

    @lib pub fn GetUserProfileDirectoryW(
        hToken:       HANDLE,
        lpProfileDir: *u16,
        lpcchSize:    *u32
    ): i32

    /// Returns the "All Users" profile directory (e.g. C:\\Users\\Public).
    @lib pub fn GetAllUsersProfileDirectoryA(
        lpProfileDir: *char,
        lpcchSize:    *u32
    ): i32
    @lib pub fn GetAllUsersProfileDirectoryW(
        lpProfileDir: *u16,
        lpcchSize:    *u32
    ): i32

    /// Returns the default-user profile directory.
    @lib pub fn GetDefaultUserProfileDirectoryA(
        lpProfileDir: *char,
        lpcchSize:    *u32
    ): i32
    @lib pub fn GetDefaultUserProfileDirectoryW(
        lpProfileDir: *u16,
        lpcchSize:    *u32
    ): i32

    /// Returns the parent of all user profiles (e.g. C:\\Users).
    @lib pub fn GetProfilesDirectoryA(
        lpProfileDir: *char,
        lpcchSize:    *u32
    ): i32
    @lib pub fn GetProfilesDirectoryW(
        lpProfileDir: *u16,
        lpcchSize:    *u32
    ): i32

    // ── Profile load / unload ─────────────────────────────────────

    /// Loads the user's profile (HKCU registry hive).  Required when
    /// impersonating before reading user-specific registry data.
    /// `lpProfileInfo`: *PROFILEINFOA struct; set `dwSize`=44 and
    /// `lpUserName` before calling.
    @lib pub fn LoadUserProfileA(
        hToken:        HANDLE,
        lpProfileInfo: *void    // *PROFILEINFOA
    ): i32
    @lib pub fn LoadUserProfileW(
        hToken:        HANDLE,
        lpProfileInfo: *void    // *PROFILEINFOA
    ): i32

    @lib pub fn UnloadUserProfile(hToken: HANDLE, hProfile: HANDLE): i32

    // ── Environment block ─────────────────────────────────────────

    /// Builds a NUL-terminated environment block for `hToken`.
    /// Output is a UTF-16 string of `name=value\0name=value\0\0` form.
    /// `bInherit`: 1 = copy current process environment first.
    /// Free with `DestroyEnvironmentBlock`.
    @lib pub fn CreateEnvironmentBlock(
        lpEnvironment: **void,
        hToken:        HANDLE,   // 0 = current user
        bInherit:      i32
    ): i32

    @lib pub fn DestroyEnvironmentBlock(lpEnvironment: *void): i32

    /// Expands %VAR% references using the user's environment block.
    /// `lpSrc`: UTF-16 input string; `lpDst`: UTF-16 output buffer.
    /// `dwSize`: size of `lpDst` in characters (not bytes).
    @lib pub fn ExpandEnvironmentStringsForUserA(
        hToken:    HANDLE,
        lpSrc:     *const char,
        lpDst:     *char,
        dwSize:    u32
    ): i32

    @lib pub fn ExpandEnvironmentStringsForUserW(
        hToken: HANDLE,
        lpSrc:  *const u16,
        lpDst:  *u16,
        dwSize: u32
    ): i32

    // ── Group policy ──────────────────────────────────────────────

    /// Returns the path to the registry hive backing Group Policy
    /// for `hToken` (user policy, not machine).
    @lib pub fn EnterCriticalPolicySection(bMachine: i32): *void
    @lib pub fn LeaveCriticalPolicySection(hSection: *void): i32

    /// Forces a refresh of group policy.  `bMachine`: 1 = machine
    /// policy, 0 = user policy. `dwOptions`: 0 = normal,
    /// 1 = RP_FORCE (re-apply even if unchanged).
    @lib pub fn RefreshPolicy(bMachine: i32): i32
    @lib pub fn RefreshPolicyEx(bMachine: i32, dwOptions: u32): i32

    /// Notifies that user policy has changed.  Combine with
    /// `RegisterGPNotification` to receive updates.
    @lib pub fn RegisterGPNotification(hEvent: HANDLE, bMachine: i32): i32
    @lib pub fn UnregisterGPNotification(hEvent: HANDLE): i32

    // Note: app-data paths are not in userenv — use SHGetFolderPathA
    // (shell32) or SHGetKnownFolderPath for AppData/LocalAppData.
}
