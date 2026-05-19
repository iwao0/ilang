// avrt.dll — Multimedia Class Scheduler Service (MMCSS).
//
// Used by audio / video apps to ask Windows for guaranteed CPU
// scheduling time. The classic example is WASAPI exclusive-mode
// audio playback at low latency.

// ─── AvSetMmThreadPriority levels (AVRT_PRIORITY_*) ──────────────
pub enum AvrtPriority: i32 {
    veryLow  = -2
    low      = -1
    normal   = 0
    high     = 1
    critical = 2
}

@extern(C, "avrt") {
    // ── Task lifecycle ────────────────────────────────────────────

    /// Joins the calling thread to an MMCSS task profile.
    /// `TaskName`: well-known names registered under the registry key
    ///   HKLM\\SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\Multimedia\\SystemProfile\\Tasks
    /// Common values: "Audio", "Pro Audio", "Capture", "Distribution",
    ///                "Games", "Playback", "Window Manager".
    /// `TaskIndex`: pass a u32 = 0 on first call; the kernel writes
    ///              a per-task index here for cleanup.
    /// Returns NULL on failure (call `GetLastError`).
    @lib pub fn AvSetMmThreadCharacteristicsA(
        TaskName:  *const char,
        TaskIndex: *u32
    ): *void

    /// Joins two tasks at once. Useful for threads that need both
    /// audio and video priority.
    @lib pub fn AvSetMmMaxThreadCharacteristicsA(
        FirstTask:  *const char,
        SecondTask: *const char,
        TaskIndex:  *u32
    ): *void

    /// Removes the calling thread from its MMCSS task.
    /// Always call before thread exit.
    @lib pub fn AvRevertMmThreadCharacteristics(AvrtHandle: *void): i32

    // ── Priority within a task ────────────────────────────────────

    /// Adjusts the priority of `AvrtHandle` within its task profile.
    /// `Priority`: AvrtPriority.*. Default after join is `normal`.
    @lib pub fn AvSetMmThreadPriority(
        AvrtHandle: *void,
        Priority:   i32
    ): i32

    // ── Quantum control ───────────────────────────────────────────

    /// Lets a thread temporarily lower its CPU usage so other
    /// (potentially priority-boosted) threads can run.  `bAvrtThrottle`:
    /// 0 = off (default), 1 = on.
    @lib pub fn AvQuerySystemResponsiveness(
        AvrtHandle:                  *void,
        SystemResponsivenessValue:   *u32
    ): i32

    @lib pub fn AvRtCreateThreadOrderingGroup(
        Context:       *void,
        Period:        *void,    // *LARGE_INTEGER
        ThreadOrderingGuid: *const void,
        Timeout:       *void
    ): i32

    @lib pub fn AvRtDeleteThreadOrderingGroup(Context: *void): i32

    @lib pub fn AvRtJoinThreadOrderingGroup(
        Context:               **void,
        ThreadOrderingGuid:    *const void,
        Before:                i32
    ): i32

    @lib pub fn AvRtLeaveThreadOrderingGroup(Context: *void): i32

    @lib pub fn AvRtWaitOnThreadOrderingGroup(Context: *void): i32
}
