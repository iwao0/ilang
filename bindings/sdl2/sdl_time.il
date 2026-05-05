// SDL2 timing + event-queue helpers. Mirrors SDL_timer.h plus
// the event-queue subset of SDL_events.h (SDL_PumpEvents,
// SDL_PollEvent, SDL_HasEvent / SDL_FlushEvent and friends).
// Timer-callback APIs (SDL_AddTimer / SDL_RemoveTimer) need a C
// function pointer and aren't exposed.

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
    // SDL_Event is a union spanning every event type. The actual
    // size varies by SDL version (56 B in 2.0.x, up to ~88 B in
    // newer releases). We pad to 88 bytes so SDL never writes past
    // the end. Every sub-event lays its `Uint32 type` at offset 0,
    // so reading `event_type` is enough for "what kind of event".
    struct SDL_Event {
        event_type: u32
        _pad: u8[84]
    }

    @lib("SDL2") fn SDL_PumpEvents()
    @lib("SDL2") fn SDL_PollEvent(event: *SDL_Event): i32
    @lib("SDL2") fn SDL_PushEvent(event: *SDL_Event): i32
    @lib("SDL2") fn SDL_HasEvent(event_type: u32): i32
    @lib("SDL2") fn SDL_HasEvents(min_type: u32, max_type: u32): i32
    @lib("SDL2") fn SDL_FlushEvent(event_type: u32)
    @lib("SDL2") fn SDL_FlushEvents(min_type: u32, max_type: u32)
    @lib("SDL2") fn SDL_EventState(event_type: u32, state: i32): u8
    // SDL_GetEventState is a macro in SDL_events.h
    // (`#define SDL_GetEventState(t) SDL_EventState(t, SDL_QUERY)`),
    // not a real symbol — wrap via SDL_EventState below.

    // ─── friendly wrappers ───────────────────────────────────────

    fn delay(ms: u32) {
        SDL_Delay(ms)
    }

    // Milliseconds since SDL_Init. Wraps after ~49 days; use
    // `getTicks64()` for monotonic playtime tracking longer than
    // that.
    fn getTicks(): u32 {
        SDL_GetTicks()
    }
    fn getTicks64(): u64 {
        SDL_GetTicks64()
    }

    // High-resolution counter + its frequency (ticks per second).
    // For elapsed measurements: `(b - a) / getPerformanceFrequency()
    // * 1e6` µs.
    fn getPerformanceCounter(): u64 {
        SDL_GetPerformanceCounter()
    }
    fn getPerformanceFrequency(): u64 {
        SDL_GetPerformanceFrequency()
    }

    // Timer callback registration.
    //
    // The callback signature on the C side is
    //   `Uint32 (*)(Uint32 interval, void *param)`
    // — return the next interval in ms, or 0 to cancel. Caller
    // obtains the fn pointer via `fnAddr(myFn)` (must be invoked
    // inside the caller's own `@extern(C){}` block). `param` is a
    // raw `void*` shipped through to the callback; pass 0 when
    // unused.
    fn addTimer(interval: u32, callback: i64, param: i64): i32 {
        SDL_AddTimer(
            interval,
            callback as *const void,
            param as *const void
        )
    }
    fn removeTimer(id: i32): i32 {
        SDL_RemoveTimer(id)
    }

    // SDL_TICKS_PASSED(A, B) macro: true if `a` is at or past `b`,
    // accounting for u32 wrap-around. Use with `getTicks()`
    // ("has the deadline passed?").
    fn ticksPassed(a: u32, b: u32): bool {
        // Signed comparison after subtraction handles wrap.
        ((b as i32) - (a as i32)) <= 0
    }

    // Drain the OS event queue into SDL's internal queue. Required
    // on macOS Cocoa for window composition; harmless elsewhere.
    fn pumpEvents() {
        SDL_PumpEvents()
    }

    // True iff at least one event of `event_type` is pending.
    // Doesn't remove it from the queue.
    fn hasEvent(event_type: u32): bool {
        SDL_HasEvent(event_type) != 0
    }
    fn hasEvents(min_type: u32, max_type: u32): bool {
        SDL_HasEvents(min_type, max_type) != 0
    }

    fn flushEvent(event_type: u32) {
        SDL_FlushEvent(event_type)
    }
    fn flushEvents(min_type: u32, max_type: u32) {
        SDL_FlushEvents(min_type, max_type)
    }

    // Pull one event off the queue. Returns true if `event` was
    // populated, false if the queue was empty. Caller looks at
    // `event.event_type` to decide what to do.
    fn pollEvent(event: SDL_Event): bool {
        SDL_PollEvent(event) != 0
    }

    // Push a synthetic event onto the queue. Returns 1 on
    // success, 0 if the event was filtered, < 0 on error.
    fn pushEvent(event: SDL_Event): i32 {
        SDL_PushEvent(event)
    }

    // Toggle delivery / query of an event class. `state`:
    //   QUERY = -1, IGNORE = 0, ENABLE = 1.
    // Returns the previous state.
    fn eventState(event_type: u32, state: i32): u8 {
        SDL_EventState(event_type, state)
    }
    fn getEventState(event_type: u32): u8 {
        SDL_EventState(event_type, EVENT_QUERY)
    }

    // True if the user clicked the window close button (or asked
    // the OS to terminate the process). Internally pumps events
    // and peeks for `SDL_QUIT`. Caller normally responds by
    // exiting their main loop.
    fn quitRequested(): bool {
        SDL_PumpEvents()
        if SDL_HasEvent(EVENT_QUIT) != 0 {
            SDL_FlushEvent(EVENT_QUIT)
            return true
        }
        false
    }
}

// ─── SDL_EventType (subset that covers the common cases) ──────────
const EVENT_FIRSTEVENT: u32              = 0
const EVENT_QUIT: u32                    = 0x100
const EVENT_APP_TERMINATING: u32         = 0x101
const EVENT_APP_LOWMEMORY: u32           = 0x102
const EVENT_APP_WILLENTERBACKGROUND: u32 = 0x103
const EVENT_APP_DIDENTERBACKGROUND: u32  = 0x104
const EVENT_APP_WILLENTERFOREGROUND: u32 = 0x105
const EVENT_APP_DIDENTERFOREGROUND: u32  = 0x106
const EVENT_LOCALECHANGED: u32           = 0x107
const EVENT_DISPLAYEVENT: u32            = 0x150
const EVENT_WINDOWEVENT: u32             = 0x200
const EVENT_SYSWMEVENT: u32              = 0x201
const EVENT_KEYDOWN: u32                 = 0x300
const EVENT_KEYUP: u32                   = 0x301
const EVENT_TEXTEDITING: u32             = 0x302
const EVENT_TEXTINPUT: u32               = 0x303
const EVENT_KEYMAPCHANGED: u32           = 0x304
const EVENT_MOUSEMOTION: u32             = 0x400
const EVENT_MOUSEBUTTONDOWN: u32         = 0x401
const EVENT_MOUSEBUTTONUP: u32           = 0x402
const EVENT_MOUSEWHEEL: u32              = 0x403
const EVENT_JOYAXISMOTION: u32           = 0x600
const EVENT_JOYBALLMOTION: u32           = 0x601
const EVENT_JOYHATMOTION: u32            = 0x602
const EVENT_JOYBUTTONDOWN: u32           = 0x603
const EVENT_JOYBUTTONUP: u32             = 0x604
const EVENT_JOYDEVICEADDED: u32          = 0x605
const EVENT_JOYDEVICEREMOVED: u32        = 0x606
const EVENT_CONTROLLERAXISMOTION: u32     = 0x650
const EVENT_CONTROLLERBUTTONDOWN: u32     = 0x651
const EVENT_CONTROLLERBUTTONUP: u32       = 0x652
const EVENT_CONTROLLERDEVICEADDED: u32    = 0x653
const EVENT_CONTROLLERDEVICEREMOVED: u32  = 0x654
const EVENT_CONTROLLERDEVICEREMAPPED: u32 = 0x655
const EVENT_CONTROLLERTOUCHPADDOWN: u32   = 0x656
const EVENT_CONTROLLERTOUCHPADMOTION: u32 = 0x657
const EVENT_CONTROLLERTOUCHPADUP: u32     = 0x658
const EVENT_CONTROLLERSENSORUPDATE: u32   = 0x659
const EVENT_FINGERDOWN: u32              = 0x700
const EVENT_FINGERUP: u32                = 0x701
const EVENT_FINGERMOTION: u32            = 0x702
const EVENT_DOLLARGESTURE: u32           = 0x800
const EVENT_DOLLARRECORD: u32            = 0x801
const EVENT_MULTIGESTURE: u32            = 0x802
const EVENT_CLIPBOARDUPDATE: u32         = 0x900
const EVENT_DROPFILE: u32                = 0x1000
const EVENT_DROPTEXT: u32                = 0x1001
const EVENT_DROPBEGIN: u32               = 0x1002
const EVENT_DROPCOMPLETE: u32            = 0x1003
const EVENT_AUDIODEVICEADDED: u32        = 0x1100
const EVENT_AUDIODEVICEREMOVED: u32      = 0x1101
const EVENT_SENSORUPDATE: u32            = 0x1200
const EVENT_RENDER_TARGETS_RESET: u32    = 0x2000
const EVENT_RENDER_DEVICE_RESET: u32     = 0x2001
const EVENT_USEREVENT: u32               = 0x8000
const EVENT_LASTEVENT: u32               = 0xFFFF

// ─── EventState states (passed to SDL_EventState) ─────────────────
const EVENT_QUERY: i32   = -1
const EVENT_IGNORE: i32  = 0
const EVENT_DISABLE: i32 = 0       // alias
const EVENT_ENABLE: i32  = 1
