// SDL2 timing + event-queue helpers. Mirrors SDL_timer.h plus
// the event-queue subset of SDL_events.h (SDL_PumpEvents,
// SDL_PollEvent, SDL_HasEvent / SDL_FlushEvent and friends).
// Timer-callback APIs (SDL_AddTimer / SDL_RemoveTimer) need a C
// function pointer and aren't exposed.

/// SDL_EventType — subset covering the common cases. Stored in
/// `Event.eventType` so callers can dispatch on `pollEvent`.
enum EventType: u32 {
    firstEvent              = 0
    quit                    = 0x100
    appTerminating          = 0x101
    appLowMemory            = 0x102
    appWillEnterBackground  = 0x103
    appDidEnterBackground   = 0x104
    appWillEnterForeground  = 0x105
    appDidEnterForeground   = 0x106
    localeChanged           = 0x107
    displayEvent            = 0x150
    windowEvent             = 0x200
    sysWmEvent              = 0x201
    keyDown                 = 0x300
    keyUp                   = 0x301
    textEditing             = 0x302
    textInput               = 0x303
    keymapChanged           = 0x304
    mouseMotion             = 0x400
    mouseButtonDown         = 0x401
    mouseButtonUp           = 0x402
    mouseWheel              = 0x403
    joyAxisMotion           = 0x600
    joyBallMotion           = 0x601
    joyHatMotion            = 0x602
    joyButtonDown           = 0x603
    joyButtonUp             = 0x604
    joyDeviceAdded          = 0x605
    joyDeviceRemoved        = 0x606
    controllerAxisMotion    = 0x650
    controllerButtonDown    = 0x651
    controllerButtonUp      = 0x652
    controllerDeviceAdded   = 0x653
    controllerDeviceRemoved = 0x654
    controllerDeviceRemapped = 0x655
    controllerTouchpadDown  = 0x656
    controllerTouchpadMotion = 0x657
    controllerTouchpadUp    = 0x658
    controllerSensorUpdate  = 0x659
    fingerDown              = 0x700
    fingerUp                = 0x701
    fingerMotion            = 0x702
    dollarGesture           = 0x800
    dollarRecord            = 0x801
    multiGesture            = 0x802
    clipboardUpdate         = 0x900
    dropFile                = 0x1000
    dropText                = 0x1001
    dropBegin               = 0x1002
    dropComplete            = 0x1003
    audioDeviceAdded        = 0x1100
    audioDeviceRemoved      = 0x1101
    sensorUpdate            = 0x1200
    renderTargetsReset      = 0x2000
    renderDeviceReset       = 0x2001
    userEvent               = 0x8000
    lastEvent               = 0xFFFF
}

/// State value passed to / returned from `eventState`.
/// `disable` is an alias for `ignore` (both mean 0, matching SDL).
enum EventState: i32 {
    query   = -1
    ignore  = 0
    disable = 0   // alias for ignore
    enable  = 1
}

/// `SDL_eventaction` — what `peepEvents` does with the matched events.
enum PeepAction: i32 {
    add  = 0   // append to the queue
    peek = 1   // copy without removing
    get  = 2   // copy and remove
}

@extern(C) {
    // ─── timer ───────────────────────────────────────────────────
    @lib("SDL2") fn SDL_Delay(ms: u32)
    @lib("SDL2") fn SDL_GetTicks(): u32
    @lib("SDL2") fn SDL_GetTicks64(): u64
    @lib("SDL2") fn SDL_GetPerformanceCounter(): u64
    @lib("SDL2") fn SDL_GetPerformanceFrequency(): u64
    @lib("SDL2") fn SDL_AddTimer(
        interval: u32, callback: *const void, param: *const void
    ): i32
    @lib("SDL2") fn SDL_RemoveTimer(id: i32): i32

    // ─── event queue ─────────────────────────────────────────────
    /// `Event` is a union spanning every event type. The actual
    /// size varies by SDL version (56 B in 2.0.x, up to ~88 B in
    /// newer releases). We pad to 88 bytes so SDL never writes past
    /// the end. Every sub-event lays its `Uint32 type` at offset 0,
    /// so reading `eventType` is enough for "what kind of event".
    struct Event {
        /// Discriminant — matches an `EventType` variant.
        eventType: u32
        _pad: u8[84]
    }

    @lib("SDL2") fn SDL_PumpEvents()
    @lib("SDL2") fn SDL_PollEvent(event: *Event): i32
    @lib("SDL2") fn SDL_WaitEvent(event: *Event): i32
    @lib("SDL2") fn SDL_WaitEventTimeout(event: *Event, timeout: i32): i32
    @lib("SDL2") fn SDL_PeepEvents(
        events: *Event, numevents: i32, action: i32,
        minType: u32, maxType: u32
    ): i32
    @lib("SDL2") fn SDL_RegisterEvents(numevents: i32): u32
    @lib("SDL2") fn SDL_PushEvent(event: *Event): i32
    @lib("SDL2") fn SDL_HasEvent(eventType: u32): i32
    @lib("SDL2") fn SDL_HasEvents(min_type: u32, max_type: u32): i32
    @lib("SDL2") fn SDL_FlushEvent(eventType: u32)
    @lib("SDL2") fn SDL_FlushEvents(min_type: u32, max_type: u32)
    @lib("SDL2") fn SDL_EventState(eventType: u32, state: i32): u8
    // SDL_GetEventState is a macro in SDL_events.h
    // (`#define SDL_GetEventState(t) SDL_EventState(t, SDL_QUERY)`),
    // not a real symbol — wrap via SDL_EventState below.

    // ─── friendly wrappers ───────────────────────────────────────

    /// Block the current thread for `ms` milliseconds. Granularity
    /// depends on the OS scheduler (typically 1 ms).
    ///
    /// Args:
    ///   ms — wait duration in milliseconds.
    fn delay(ms: u32) {
        SDL_Delay(ms)
    }

    /// Milliseconds elapsed since `SDL_Init`.
    ///
    /// Returns:
    ///   ticks since init. Wraps after ~49 days — use `getTicks64()`
    ///   for monotonic playtime longer than that.
    fn getTicks(): u32 {
        SDL_GetTicks()
    }

    /// 64-bit version of `getTicks` — never wraps in any sane
    /// timescale.
    ///
    /// Returns:
    ///   ms since init.
    fn getTicks64(): u64 {
        SDL_GetTicks64()
    }

    /// High-resolution counter — usually backed by RDTSC / mach
    /// time / clock_gettime.
    ///
    /// Returns:
    ///   monotonic counter value. Combine with
    ///   `getPerformanceFrequency()` to convert deltas into seconds:
    ///   `(b - a) as f64 / getPerformanceFrequency() as f64`.
    fn getPerformanceCounter(): u64 {
        SDL_GetPerformanceCounter()
    }

    /// Returns:
    ///   ticks per second of the high-resolution counter.
    fn getPerformanceFrequency(): u64 {
        SDL_GetPerformanceFrequency()
    }

    /// Schedule a callback to fire on SDL's timer thread.
    ///
    /// The callback signature on the C side is
    ///   `Uint32 (*)(Uint32 interval, void *param)`
    /// — return the next interval in ms, or `0` to cancel. Caller
    /// obtains the fn pointer via `fnAddr(myFn)` (must be invoked
    /// inside the caller's own `@extern(C){}` block).
    ///
    /// Args:
    ///   interval — first-shot delay in ms.
    ///   callback — fn pointer (use `fnAddr(myFn) as i64`).
    ///   param    — raw `void*` shipped through to the callback;
    ///              pass `0` when unused.
    ///
    /// Returns:
    ///   timer id (`> 0` on success, `0` on failure). Pass to
    ///   `removeTimer` to cancel.
    fn addTimer(interval: u32, callback: i64, param: i64): i32 {
        SDL_AddTimer(
            interval,
            callback as *const void,
            param as *const void
        )
    }

    /// Cancel a scheduled timer.
    ///
    /// Args:
    ///   id — value returned by `addTimer`.
    ///
    /// Returns:
    ///   `1` if the timer was found and removed, `0` otherwise.
    fn removeTimer(id: i32): i32 {
        SDL_RemoveTimer(id)
    }

    /// `SDL_TICKS_PASSED(a, b)` — true when `a` is at or past `b`,
    /// accounting for u32 wrap-around. Use with `getTicks()` for
    /// "has the deadline passed?" checks.
    ///
    /// Args:
    ///   a — current tick count (typically `getTicks()`).
    ///   b — deadline tick count.
    ///
    /// Returns:
    ///   true if `a >= b` modulo wrap.
    fn ticksPassed(a: u32, b: u32): bool {
        // Signed comparison after subtraction handles wrap.
        ((b as i32) - (a as i32)) <= 0
    }

    /// Drain the OS event queue into SDL's internal queue. Required
    /// on macOS Cocoa for window composition; harmless elsewhere.
    /// Call once per frame before reading events.
    fn pumpEvents() {
        SDL_PumpEvents()
    }

    /// Args:
    ///   eventType — variant to look for.
    ///
    /// Returns:
    ///   true iff at least one event of that type is pending.
    ///   Doesn't remove it from the queue.
    fn hasEvent(eventType: EventType): bool {
        SDL_HasEvent(eventType as u32) != 0
    }

    /// Range version of `hasEvent`.
    ///
    /// Args:
    ///   minType — lower bound (inclusive).
    ///   maxType — upper bound (inclusive).
    fn hasEvents(minType: EventType, maxType: EventType): bool {
        SDL_HasEvents(minType as u32, maxType as u32) != 0
    }

    /// Discard every queued event of the given type.
    fn flushEvent(eventType: EventType) {
        SDL_FlushEvent(eventType as u32)
    }

    /// Range version of `flushEvent`.
    fn flushEvents(minType: EventType, maxType: EventType) {
        SDL_FlushEvents(minType as u32, maxType as u32)
    }

    /// Pull one event off the queue.
    ///
    /// Args:
    ///   event — buffer that SDL fills in.
    ///
    /// Returns:
    ///   true if `event` was populated, false if the queue was
    ///   empty. Caller looks at `event.eventType` to dispatch.
    fn pollEvent(event: Event): bool {
        SDL_PollEvent(event) != 0
    }

    /// Push a synthetic event onto the queue.
    ///
    /// Args:
    ///   event — fully-populated event (set `event.eventType`).
    ///
    /// Returns:
    ///   `1` on success, `0` if the event was filtered, `< 0` on
    ///   error.
    fn pushEvent(event: Event): i32 {
        SDL_PushEvent(event)
    }

    /// Toggle delivery / query of an event class.
    ///
    /// Args:
    ///   eventType — class to control.
    ///   state      — `enable`, `ignore`, or `query`.
    ///
    /// Returns:
    ///   previous state for that class.
    fn eventState(eventType: EventType, state: EventState): u8 {
        SDL_EventState(eventType as u32, state as i32)
    }

    /// Read the current delivery state for an event class.
    ///
    /// Args:
    ///   eventType — class to query.
    ///
    /// Returns:
    ///   current state (`0` = ignored, `1` = enabled).
    fn getEventState(eventType: EventType): u8 {
        SDL_EventState(eventType as u32, EventState.query as i32)
    }

    /// True if the user clicked the window close button (or asked
    /// the OS to terminate the process). Internally pumps events
    /// and peeks for `SDL_QUIT`. Caller normally responds by
    /// exiting their main loop.
    fn quitRequested(): bool {
        SDL_PumpEvents()
        if SDL_HasEvent(EventType.quit as u32) != 0 {
            SDL_FlushEvent(EventType.quit as u32)
            return true
        }
        false
    }

    /// Block until an event is available, then return it.
    ///
    /// Args:
    ///   event — buffer that SDL fills in.
    ///
    /// Returns:
    ///   true on success, false on internal SDL error.
    fn waitEvent(event: Event): bool {
        SDL_WaitEvent(event) != 0
    }

    /// Block up to `timeoutMs` for an event.
    ///
    /// Args:
    ///   event     — buffer to fill.
    ///   timeoutMs — max wait in milliseconds. `0` returns
    ///               immediately; negative is treated as block.
    ///
    /// Returns:
    ///   true if `event` was populated, false on timeout / error.
    fn waitEventTimeout(event: Event, timeoutMs: i32): bool {
        SDL_WaitEventTimeout(event, timeoutMs) != 0
    }

    /// Peek / get / add events in bulk.
    ///
    /// Args:
    ///   events    — caller-allocated array; SDL fills the first
    ///               `numEvents` entries on `peek` / `get`, and
    ///               reads them on `add`.
    ///   numEvents — how many slots in `events` are available.
    ///   action    — `peek`, `get`, or `add`.
    ///   minType   — lower event-type bound (inclusive).
    ///   maxType   — upper event-type bound (inclusive).
    ///
    /// Returns:
    ///   number of events actually copied / added, `< 0` on error.
    fn peepEvents(
        events: Event[], numEvents: i32, action: PeepAction,
        minType: EventType, maxType: EventType
    ): i32 {
        let p: *Event = events
        SDL_PeepEvents(
            p, numEvents, action as i32,
            minType as u32, maxType as u32
        )
    }

    /// Allocate a contiguous range of user event types. Apps that
    /// post their own events via `pushEvent` should hold one such
    /// id rather than picking arbitrary numbers in `userEvent..`.
    ///
    /// Args:
    ///   numEvents — how many distinct ids to reserve.
    ///
    /// Returns:
    ///   the first id of the range, or `0xFFFFFFFF` if SDL has run
    ///   out of user event slots.
    fn registerEvents(numEvents: i32): u32 {
        SDL_RegisterEvents(numEvents)
    }
}
