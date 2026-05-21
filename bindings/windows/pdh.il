/// pdh.dll — Performance Data Helper.
///
/// Higher-level wrapper around the raw registry-based perf counters.
/// Pattern: PdhOpenQuery → PdhAddCounterA → PdhCollectQueryData (×2
/// for two samples to compute a rate) → PdhGetFormattedCounterValue.
///
/// All functions return PDH_STATUS (i32); 0 = ERROR_SUCCESS.

// ─── PDH status codes (selected) ─────────────────────────────────
pub enum PdhStatus: i32 {
    success          = 0
    invalidArgument  = -1073738819   // 0xC0000BBD
    cstatusBadCountername = -1073738825
    invalidHandle    = -1073738824
    noData           = -2147481643   // 0x800007D5
    moreData         = -2147481648
    insufficientBuffer = -1073738824
    noSuchCountername = -1073738824
}

// ─── PdhGetFormattedCounterValue dwFormat ────────────────────────
@flags
pub enum PdhFormat: u32 {
    long      = 0x0100
    double    = 0x0200
    large     = 0x0400
    /// Bit flag: scale the counter value by the multiplier from the
    /// counter info (e.g. % CPU is 0..100 instead of 0..1).
    noScale   = 0x1000
    /// Multiply by 1000 for fractional display.
    x1000     = 0x2000
    noCap100  = 0x4000
}

@extern(C, "pdh") {
    /// Opaque PDH query handle (HQUERY).
    @handle pub struct HPdhQuery {}

    /// Opaque PDH counter handle (HCOUNTER).
    @handle pub struct HPdhCounter {}

    // ── PDH_FMT_COUNTERVALUE — 16 bytes union ────────────────────

    /// Tagged union for counter readings. `CStatus` is 0 on a valid
    /// reading; otherwise it carries a PDH error code. The `value`
    /// member is interpreted according to the format flag passed to
    /// `PdhGetFormattedCounterValue`:
    ///   long   → read `value_lo` as i32
    ///   double → read `value_lo`/`value_hi` as the bits of an f64
    ///   large  → read `value_lo`/`value_hi` as i64
    pub struct PDH_FMT_COUNTERVALUE {
        CStatus:  u32
        _pad:     u32
        value_lo: u32
        value_hi: u32
    }

    // ── Query / counter lifecycle ─────────────────────────────────

    /// Creates a new query. `szDataSource`: NULL = live data from this
    /// machine, or a path to a `.blg`/`.csv`/`.tsv` log file.
    /// `dwUserData`: opaque user-supplied tag (0 if unused).
    @lib pub fn PdhOpenQueryA(
        szDataSource: *const char,
        dwUserData:   u64,
        phQuery:      **HPdhQuery
    ): i32

    @lib pub fn PdhCloseQuery(hQuery: *HPdhQuery): i32

    /// Adds a counter to a query. The path looks like
    ///   "\\Processor(_Total)\\% Processor Time"
    ///   "\\Memory\\Available Bytes"
    @lib pub fn PdhAddCounterA(
        hQuery:      *HPdhQuery,
        szFullCounterPath: *const char,
        dwUserData:  u64,
        phCounter:   **HPdhCounter
    ): i32

    /// Modern variant (Vista+) — better for English-vs-localised
    /// names; honours `PERF_DETAIL_*` user-class hints.
    @lib pub fn PdhAddEnglishCounterA(
        hQuery:      *HPdhQuery,
        szFullCounterPath: *const char,
        dwUserData:  u64,
        phCounter:   **HPdhCounter
    ): i32

    @lib pub fn PdhRemoveCounter(hCounter: *HPdhCounter): i32

    // ── Sampling ──────────────────────────────────────────────────

    /// Collects one sample for every counter in the query.
    /// Most rate-style counters need TWO calls separated by a wait
    /// before they yield a meaningful value.
    @lib pub fn PdhCollectQueryData(hQuery: *HPdhQuery): i32

    /// Async variant: schedules sampling on `Event` every `Interval` ms.
    @lib pub fn PdhCollectQueryDataEx(
        hQuery:    *HPdhQuery,
        dwIntervalTime: u32,
        hNewDataEvent: *void
    ): i32

    /// Single-shot variant that returns the timestamp of the sample
    /// in `pllTimeStamp` (FILETIME-style 100-ns intervals).
    @lib pub fn PdhCollectQueryDataWithTime(
        hQuery:    *HPdhQuery,
        pllTimeStamp: *i64
    ): i32

    // ── Reading counters ─────────────────────────────────────────

    /// Reads the most recently collected sample.
    /// `dwFormat`: PdhFormat.*  (combine the type flag with optional
    /// scale flags).
    @lib pub fn PdhGetFormattedCounterValue(
        hCounter:   *HPdhCounter,
        dwFormat:   u32,
        lpdwType:   *u32,
        pValue:     *PDH_FMT_COUNTERVALUE
    ): i32

    /// Reads the raw (unprocessed) sample.
    @lib pub fn PdhGetRawCounterValue(
        hCounter:    *HPdhCounter,
        lpdwType:    *u32,
        pValue:      *void      // *PDH_RAW_COUNTER
    ): i32

    // ── Counter discovery / browsing ──────────────────────────────

    /// Enumerates counter objects (e.g. "Processor", "Memory") on
    /// `szDataSource` (NULL = local).
    @lib pub fn PdhEnumObjectsA(
        szDataSource:   *const char,
        szMachineName:  *const char,
        mszObjectList:  *char,
        pcchBufferSize: *u32,
        dwDetailLevel:  u32,
        bRefresh:       i32
    ): i32

    @lib pub fn PdhEnumObjectItemsA(
        szDataSource:   *const char,
        szMachineName:  *const char,
        szObjectName:   *const char,
        mszCounterList: *char,
        pcchCounterListSize: *u32,
        mszInstanceList: *char,
        pcchInstanceListSize: *u32,
        dwDetailLevel:  u32,
        dwFlags:        u32
    ): i32

    // ── Path expansion ────────────────────────────────────────────

    /// Expands wildcards in a counter path.
    /// `mszExpandedPathList`: receives a list of NUL-terminated paths
    /// (double-NUL at end).
    @lib pub fn PdhExpandWildCardPathA(
        szDataSource:   *const char,
        szWildCardPath: *const char,
        mszExpandedPathList: *char,
        pcchPathListLength: *u32,
        dwFlags:        u32
    ): i32

    @lib pub fn PdhValidatePathA(szFullPathBuffer: *const char): i32
}
