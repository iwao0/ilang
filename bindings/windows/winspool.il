/// winspool.drv — Print Spooler API.
///
/// Three workflows:
///   1. Enumerate printers     — EnumPrintersA
///   2. Submit a print job     — OpenPrinterA / StartDocPrinterA /
///                                StartPagePrinter / WritePrinter /
///                                EndPagePrinter / EndDocPrinter /
///                                ClosePrinter
///   3. Manage queues          — GetJobA / SetJobA / EnumJobsA

// ─── PRINTER_ENUM_* flags ────────────────────────────────────────
@flags
pub enum PrinterEnumFlags: u32 {
    default_       = 0x00000001
    local_         = 0x00000002
    connections    = 0x00000004
    favorite       = 0x00000004
    name           = 0x00000008
    remote         = 0x00000010
    shared         = 0x00000020
    network        = 0x00000040
    category3d     = 0x00004000
}

// ─── Job control codes (JOB_CONTROL_*) ───────────────────────────
pub enum JobControl: u32 {
    pause         = 1
    resume        = 2
    cancel        = 3
    restart       = 4
    delete        = 5
    sentToPrinter = 6
    lastPageEjected = 7
    retain        = 8
    release       = 9
}

@extern(C, "winspool", "winspool.drv") {
    /// Printer handle.
    @handle pub struct HPRINTER {}

    /// 1-byte device-mode struct skeleton (size varies — Vista+ is
    /// 220 bytes for DEVMODEA). Treat as opaque.
    pub struct DEVMODEA {
        _opaque: u8[220]
    }

    /// PRINTER_INFO_1 — name + description + comment.
    pub struct PRINTER_INFO_1A {
        Flags:       u32
        pDescription: *char
        pName:        *char
        pComment:     *char
    }

    /// PRINTER_INFO_2A subset — practical fields.
    pub struct PRINTER_INFO_2A {
        pServerName:    *char
        pPrinterName:   *char
        pShareName:     *char
        pPortName:      *char
        pDriverName:    *char
        pComment:       *char
        pLocation:      *char
        pDevMode:       *DEVMODEA
        pSepFile:       *char
        pPrintProcessor: *char
        pDatatype:      *char
        pParameters:    *char
        pSecurityDescriptor: *void
        Attributes:     u32
        Priority:       u32
        DefaultPriority: u32
        StartTime:      u32
        UntilTime:      u32
        Status:         u32
        cJobs:          u32
        AveragePPM:     u32
    }

    /// DOC_INFO_1A — describes a print job.
    pub struct DOC_INFO_1A {
        pDocName:    *char
        pOutputFile: *char       // NULL = spool to default printer
        pDatatype:   *char       // "RAW" or "TEXT" or "EMF"
    }

    /// JOB_INFO_1A — short job status form.
    pub struct JOB_INFO_1A {
        JobId:        u32
        pPrinterName: *char
        pMachineName: *char
        pUserName:    *char
        pDocument:    *char
        pDatatype:    *char
        pStatus:      *char
        Status:       u32
        Priority:     u32
        Position:     u32
        TotalPages:   u32
        PagesPrinted: u32
        Submitted:    u8[16]      // SYSTEMTIME
    }

    // ── Printer enumeration / open / close ──────────────────────

    /// `EnumPrintersA(flags, name, level, pPrinterEnum, cbBuf,
    ///                pcbNeeded, pcReturned)`.
    /// Pass `cbBuf = 0` and `pPrinterEnum = NULL` to query the
    /// required size; then reallocate and call again.
    @lib pub fn EnumPrintersA(
        Flags:         u32,
        Name:          *const char,    // server name or NULL
        Level:         u32,             // 1, 2, 4, or 5
        pPrinterEnum:  *void,           // buffer for PRINTER_INFO_<Level>
        cbBuf:         u32,
        pcbNeeded:     *u32,
        pcReturned:    *u32
    ): i32
    @lib pub fn EnumPrintersW(
        Flags:         u32,
        Name:          *const u16,    // server name or NULL
        Level:         u32,             // 1, 2, 4, or 5
        pPrinterEnum:  *void,           // buffer for PRINTER_INFO_<Level>
        cbBuf:         u32,
        pcbNeeded:     *u32,
        pcReturned:    *u32
    ): i32

    @lib pub fn OpenPrinterA(
        pPrinterName: *const char,
        phPrinter:    *HPRINTER,
        pDefault:     *void              // *PRINTER_DEFAULTSA; NULL for read-only
    ): i32
    @lib pub fn OpenPrinterW(
        pPrinterName: *const u16,
        phPrinter:    *HPRINTER,
        pDefault:     *void              // *PRINTER_DEFAULTSA; NULL for read-only
    ): i32

    @lib pub fn ClosePrinter(hPrinter: HPRINTER): i32

    /// Get the default printer name. `pszBuffer` is NULL-terminated
    /// on success; `*pcchBuffer` is in / out (capacity / actual).
    @lib pub fn GetDefaultPrinterA(pszBuffer: *char, pcchBuffer: *u32): i32
    @lib pub fn GetDefaultPrinterW(pszBuffer: *u16, pcchBuffer: *u32): i32

    @lib pub fn SetDefaultPrinterA(pszPrinter: *const char): i32
    @lib pub fn SetDefaultPrinterW(pszPrinter: *const u16): i32

    // ── Document / page lifecycle ────────────────────────────────

    /// `StartDocPrinterA` returns a non-zero job id on success.
    @lib pub fn StartDocPrinterA(
        hPrinter:     HPRINTER,
        Level:        u32,                // 1 for DOC_INFO_1A
        pDocInfo:     *DOC_INFO_1A
    ): u32
    @lib pub fn StartDocPrinterW(
        hPrinter:     HPRINTER,
        Level:        u32,                // 1 for DOC_INFO_1A
        pDocInfo:     *DOC_INFO_1A
    ): u32

    @lib pub fn EndDocPrinter(hPrinter: HPRINTER): i32

    @lib pub fn StartPagePrinter(hPrinter: HPRINTER): i32
    @lib pub fn EndPagePrinter(hPrinter: HPRINTER): i32

    @lib pub fn WritePrinter(
        hPrinter:     HPRINTER,
        pBuf:         *const void,
        cbBuf:        u32,
        pcWritten:    *u32
    ): i32

    @lib pub fn FlushPrinter(
        hPrinter:    HPRINTER,
        pBuf:        *const void,
        cbBuf:       u32,
        pcWritten:   *u32,
        cSleep:      u32
    ): i32

    @lib pub fn AbortPrinter(hPrinter: HPRINTER): i32

    // ── Job control ──────────────────────────────────────────────

    @lib pub fn GetJobA(
        hPrinter:    HPRINTER,
        JobId:       u32,
        Level:       u32,                 // 1, 2, 3, or 4
        pJob:        *void,
        cbBuf:       u32,
        pcbNeeded:   *u32
    ): i32
    @lib pub fn GetJobW(
        hPrinter:    HPRINTER,
        JobId:       u32,
        Level:       u32,                 // 1, 2, 3, or 4
        pJob:        *void,
        cbBuf:       u32,
        pcbNeeded:   *u32
    ): i32

    @lib pub fn SetJobA(
        hPrinter:    HPRINTER,
        JobId:       u32,
        Level:       u32,
        pJob:        *void,               // NULL when Command-only
        Command:     u32                  // JobControl
    ): i32
    @lib pub fn SetJobW(
        hPrinter:    HPRINTER,
        JobId:       u32,
        Level:       u32,
        pJob:        *void,               // NULL when Command-only
        Command:     u32                  // JobControl
    ): i32

    @lib pub fn EnumJobsA(
        hPrinter:    HPRINTER,
        FirstJob:    u32,
        NoJobs:      u32,
        Level:       u32,
        pJob:        *void,
        cbBuf:       u32,
        pcbNeeded:   *u32,
        pcReturned:  *u32
    ): i32
    @lib pub fn EnumJobsW(
        hPrinter:    HPRINTER,
        FirstJob:    u32,
        NoJobs:      u32,
        Level:       u32,
        pJob:        *void,
        cbBuf:       u32,
        pcbNeeded:   *u32,
        pcReturned:  *u32
    ): i32

    // ── Status / driver names ────────────────────────────────────

    @lib pub fn GetPrinterA(
        hPrinter:    HPRINTER,
        Level:       u32,
        pPrinter:    *void,               // buffer
        cbBuf:       u32,
        pcbNeeded:   *u32
    ): i32
    @lib pub fn GetPrinterW(
        hPrinter:    HPRINTER,
        Level:       u32,
        pPrinter:    *void,               // buffer
        cbBuf:       u32,
        pcbNeeded:   *u32
    ): i32

    @lib pub fn GetPrinterDriverA(
        hPrinter:    HPRINTER,
        pEnvironment: *const char,        // NULL = default
        Level:       u32,                 // 1..8
        pDriverInfo: *void,
        cbBuf:       u32,
        pcbNeeded:   *u32
    ): i32
    @lib pub fn GetPrinterDriverW(
        hPrinter:    HPRINTER,
        pEnvironment: *const u16,        // NULL = default
        Level:       u32,                 // 1..8
        pDriverInfo: *void,
        cbBuf:       u32,
        pcbNeeded:   *u32
    ): i32
}
