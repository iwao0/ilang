/// sensapi.dll — Sens API: network connectivity sensing.
///
/// Replaced in modern code by the Network List Manager (INetworkListManager
/// in netprofm.dll), but the flat `IsDestinationReachableA` /
/// `IsNetworkAlive` entry points are tiny and convenient for quick
/// "are we online" checks.

@flags
pub enum NetworkAlive: u32 {
    none      = 0
    lan       = 0x1
    wan       = 0x2
    aol       = 0x4
}

@extern(C, "sensapi") {
    /// `QOCINFO` populated by `IsDestinationReachable`.
    pub struct QOCINFO {
        dwSize:     u32
        dwFlags:    u32
        dwInSpeed:  u32
        dwOutSpeed: u32
    }

    /// `IsNetworkAlive(&flags)` — non-zero when any network
    /// connection is up; `*flags` becomes NetworkAlive.lan / .wan /
    /// .aol.
    @lib pub fn IsNetworkAlive(lpdwFlags: *u32): i32

    /// Heuristic test of whether the named destination is
    /// reachable without going through a costly dial-up.
    @lib pub fn IsDestinationReachableA(
        lpszDestination: *const char,
        lpQOCInfo:       *QOCINFO
    ): i32

    @lib pub fn IsDestinationReachableW(
        lpszDestination: *const u16,
        lpQOCInfo:       *QOCINFO
    ): i32
}
