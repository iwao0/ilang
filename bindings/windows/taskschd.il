/// taskschd.dll — Task Scheduler 2.0 (Vista+).
///
/// The Task Scheduler API is COM-only — entry points are
/// CoCreateInstance + IID/CLSID values. No flat C exports.
///
/// Bring-up:
///   CoInitializeEx(NULL, COINIT_MULTITHREADED)
///   CoCreateInstance(&CLSID_TaskScheduler, NULL,
///                    CLSCTX_INPROC_SERVER, &IID_ITaskService,
///                    &service)
///   service.Connect(VARIANT(), VARIANT(), VARIANT(), VARIANT())
///   service.GetFolder(L"\\", &root)
///   root.GetTask(L"my-task", &task)
///   task.Run(VARIANT(), &runningTask)
///
/// The CLSID / IIDs below are documented for callers that wire the
/// COM plumbing manually (no flat function bindings exist).

// ─── Task state enum (TASK_STATE) ────────────────────────────────
pub enum TaskState: i32 {
    unknown   = 0
    disabled  = 1
    queued    = 2
    ready     = 3
    running   = 4
}

// ─── Trigger types (TASK_TRIGGER_TYPE2) ──────────────────────────
pub enum TaskTriggerType: i32 {
    event       = 0
    time        = 1
    daily       = 2
    weekly      = 3
    monthly     = 4
    monthlyDow  = 5
    idle        = 6
    registration = 7
    boot        = 8
    logon       = 9
    sessionStateChange = 11
    custom      = 12
}

// ─── Action types (TASK_ACTION_TYPE) ─────────────────────────────
pub enum TaskActionType: i32 {
    exec        = 0
    comHandler  = 5
    sendEmail   = 6
    showMessage = 7
}

// ─── Logon types (TASK_LOGON_TYPE) ───────────────────────────────
pub enum TaskLogonType: i32 {
    none                       = 0
    password                   = 1
    s4u                        = 2
    interactiveToken           = 3
    group                      = 4
    serviceAccount             = 5
    interactiveTokenOrPassword = 6
}

// ─── Run levels (TASK_RUNLEVEL_TYPE) ─────────────────────────────
pub enum TaskRunLevel: i32 {
    lowest  = 0
    highest = 1
}

// ─── Run flags (TASK_RUN_FLAGS) ──────────────────────────────────
@flags
pub enum TaskRunFlags: i32 {
    noFlags         = 0
    asSelf          = 1
    ignoreConstraints = 2
    useSessionId    = 4
    userSid         = 8
}

// ─── Compatibility levels (TASK_COMPATIBILITY) ───────────────────
pub enum TaskCompatibility: i32 {
    at        = 0     // Win9x / NT4 AT scheduler
    v1        = 1     // XP / 2003 task scheduler
    v2        = 2     // Vista / 7+
    v2_1      = 3     // 8
    v2_2      = 4     // 8.1
    v2_3      = 5     // 10
}

/// Documentation-only — no C entry points. Wire via CoCreateInstance.
///
/// CLSID_TaskScheduler:
///   {0F87369F-A4E5-4CFC-BD3E-73E6154572DD}
///
/// IID_ITaskService:
///   {2FABA4C7-4DA9-4013-9697-20CC3FD40F85}
///
/// IID_ITaskDefinition:
///   {F5BC8FC5-536D-4F77-B852-FBC1356FDEB6}
///
/// IID_ITaskFolder:
///   {8CFAC062-A080-4C15-9A88-AA7C2AF80DFC}
///
/// IID_IRegisteredTask:
///   {9C86F320-DEE3-4DD1-B972-A303F26B061E}
///
/// IID_IRunningTask:
///   {653758FB-7B9A-4F1E-A471-BEEB8E9B834E}
@extern(C, "taskschd") {
    // (No flat functions — the entire surface is COM. This block
    //  exists so the binding loads as a real module and provides a
    //  natural namespace for the type definitions above.)
}
