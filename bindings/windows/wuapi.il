/// wuapi.dll — Windows Update Agent (WUA).
///
/// Pure COM — no flat functions. CLSID + IIDs below drive the
/// usual CoCreateInstance bootstrap.
///
/// Common pattern:
///   CoCreateInstance(&CLSID_UpdateSession, NULL,
///                    CLSCTX_INPROC_SERVER, &IID_IUpdateSession,
///                    &session)
///   session.CreateUpdateSearcher(&searcher)
///   searcher.Search(L"IsInstalled=0 and Type='Software'",
///                   &result)
///   result.get_Updates(&updates)
///   updates.get_Count(&count)
///   for i in 0..count {
///       updates.get_Item(i, &update)
///       ...
///   }
///
/// Used by Windows Update / WSUS / SCCM clients.

// ─── OperationResultCode (call result for searcher / installer) ──
pub enum WuOperationResult: i32 {
    notStarted   = 0
    inProgress   = 1
    succeeded    = 2
    succeededWithErrors = 3
    failed       = 4
    aborted      = 5
}

// ─── ServerSelection ─────────────────────────────────────────────
pub enum WuServerSelection: i32 {
    default_   = 0
    managed    = 1
    windowsUpdate = 2
    others     = 3
}

// ─── DownloadPriority ────────────────────────────────────────────
pub enum WuDownloadPriority: i32 {
    low        = 1
    normal     = 2
    high       = 3
    extraHigh  = 4
}

// ─── UpdateType ──────────────────────────────────────────────────
pub enum WuUpdateType: i32 {
    software   = 1
    driver     = 2
}

// ─── AutomaticUpdatesNotificationLevel ───────────────────────────
pub enum AuNotificationLevel: i32 {
    notConfigured       = 0
    disabled            = 1
    notifyBeforeDownload = 2
    notifyBeforeInstallation = 3
    scheduledInstallation = 4
}

/// Documentation-only — no C entry points. Wire via CoCreateInstance.
///
/// CLSID_UpdateSession:
///   {4CB43D7F-7EEE-4906-8698-60DA1C38F2FE}
///
/// CLSID_AutomaticUpdates:
///   {BFE18E9C-6D87-4450-B312-F04D90479B33}
///
/// CLSID_UpdateDownloader:
///   {7FD3E47D-4AD1-4BA8-8B5C-DBABBC4FA9A1}
///
/// CLSID_UpdateInstaller:
///   {D2E0FE7F-D23E-48E1-93C0-6FA8CC346474}
///
/// IID_IUpdateSession (3.0):
///   {918EFD1E-B5D8-4C90-8540-AEB9BDC56F9D}
///
/// IID_IUpdateSearcher:
///   {8F45ABF1-F9AE-4B95-A933-F0F66E5056EA}
///
/// IID_ISearchResult:
///   {D40CFF62-E08C-4498-941A-01E25F0FD33C}
///
/// IID_IUpdateCollection:
///   {07F7438C-7709-4CA5-B518-91279288134E}
///
/// IID_IUpdate:
///   {6A92B07A-D821-4682-B423-5C805022CC4D}
///
/// IID_IUpdateDownloader:
///   {68F1C6F9-7ECC-4666-A464-247FE12496C3}
///
/// IID_IUpdateInstaller (4.0):
///   {EF8208EA-2304-492D-9109-23813B0958E1}
///
/// IID_IAutomaticUpdates:
///   {673425BF-C082-4C7C-BDFD-9CA01CDB1F0E}
// (No flat functions — the surface is entirely COM.)
