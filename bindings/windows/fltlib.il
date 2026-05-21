/// fltlib.dll — Filter Manager: enumerate / control minifilters
/// (registered by .sys drivers via FltRegisterFilter).
///
/// Practical use cases:
///   * Anti-virus / Endpoint security tooling — find which
///     minifilter is attached to which volume
///   * Diagnostic tools — list which filters are loaded

@extern(C, "fltlib") {
    /// 24-byte filter info entry.
    pub struct FILTER_AGGREGATE_BASIC_INFORMATION {
        Flags:                          u32
        Type:                           u32
        Generic_Flags:                  u32
        Generic_FrameID:                u32
        Generic_NumberOfInstances:      u32
        Generic_FilterNameLength:       u16
        Generic_FilterNameBufferOffset: u16
        Generic_FilterAltitudeLength:   u16
        Generic_FilterAltitudeBufferOffset: u16
    }

    /// Open a handle to the filter manager. `phPort` is a `*HANDLE`.
    /// The legacy `FilterConnectCommunicationPort` shape.
    @lib pub fn FilterConnectCommunicationPort(
        lpPortName:    *const u16,
        dwOptions:     u32,
        lpContext:     *const void,
        wSizeOfContext: u16,
        lpSecurityAttributes: *void,
        hPort:         *i64
    ): i32

    /// Send a message to / receive from the kernel-mode counterpart.
    @lib pub fn FilterSendMessage(
        hPort:        i64,
        lpInBuffer:   *void,
        dwInBufferSize: u32,
        lpOutBuffer:  *void,
        dwOutBufferSize: u32,
        lpBytesReturned: *u32
    ): i32

    @lib pub fn FilterGetMessage(
        hPort:           i64,
        lpMessageBuffer: *void,
        dwMessageBufferSize: u32,
        lpOverlapped:    *void
    ): i32

    @lib pub fn FilterReplyMessage(
        hPort:           i64,
        lpReplyBuffer:   *const void,
        dwReplyBufferSize: u32
    ): i32

    // ── Filter / instance / volume enumeration ──────────────────

    /// Enumerate filters (one frame). Repeat with the same handle
    /// until ERROR_NO_MORE_ITEMS (259).
    @lib pub fn FilterFindFirst(
        dwInformationClass: u32,        // 0 BasicInformation, 1 ExtendedInformation, 2 Aggregate
        lpBuffer:           *void,
        dwBufferSize:       u32,
        lpBytesReturned:    *u32,
        lpFilterFind:       *i64
    ): i32

    @lib pub fn FilterFindNext(
        hFilterFind:        i64,
        dwInformationClass: u32,
        lpBuffer:           *void,
        dwBufferSize:       u32,
        lpBytesReturned:    *u32
    ): i32

    @lib pub fn FilterFindClose(hFilterFind: i64): i32

    /// Enumerate volumes that have at least one filter attached.
    @lib pub fn FilterVolumeFindFirst(
        dwInformationClass: u32,
        lpBuffer:           *void,
        dwBufferSize:       u32,
        lpBytesReturned:    *u32,
        lpVolumeFind:       *i64
    ): i32

    @lib pub fn FilterVolumeFindNext(
        hVolumeFind:        i64,
        dwInformationClass: u32,
        lpBuffer:           *void,
        dwBufferSize:       u32,
        lpBytesReturned:    *u32
    ): i32

    @lib pub fn FilterVolumeFindClose(hVolumeFind: i64): i32

    /// Enumerate per-volume filter instances.
    @lib pub fn FilterVolumeInstanceFindFirst(
        lpVolumeName:       *const u16,
        dwInformationClass: u32,
        lpBuffer:           *void,
        dwBufferSize:       u32,
        lpBytesReturned:    *u32,
        lpVolumeInstanceFind: *i64
    ): i32

    /// Attach / detach a filter to / from a volume.
    @lib pub fn FilterAttach(
        lpFilterName:   *const u16,
        lpVolumeName:   *const u16,
        lpInstanceName: *const u16,
        dwCreatedInstanceNameLength: u32,
        lpCreatedInstanceName: *u16
    ): i32

    @lib pub fn FilterAttachAtAltitude(
        lpFilterName:    *const u16,
        lpVolumeName:    *const u16,
        lpAltitude:      *const u16,
        lpInstanceName:  *const u16,
        dwCreatedInstanceNameLength: u32,
        lpCreatedInstanceName: *u16
    ): i32

    @lib pub fn FilterDetach(
        lpFilterName:   *const u16,
        lpVolumeName:   *const u16,
        lpInstanceName: *const u16
    ): i32

    @lib pub fn FilterLoad(lpFilterName: *const u16): i32
    @lib pub fn FilterUnload(lpFilterName: *const u16): i32
}
