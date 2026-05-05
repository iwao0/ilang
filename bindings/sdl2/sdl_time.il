// SDL2 timing + event-pump helpers. Pumping events between sleeps
// is required on macOS so the OS run loop advances and the window
// actually paints.

@extern(C) {
    @lib("SDL2") fn SDL_Delay(ms: u32)
    @lib("SDL2") fn SDL_PumpEvents()
    @lib("SDL2") fn SDL_GetTicks(): u32
    @lib("SDL2") fn SDL_HasEvent(event_type: u32): i32
    @lib("SDL2") fn SDL_FlushEvent(event_type: u32)

    fn delay(ms: u32) {
        SDL_Delay(ms)
    }

    // Drain the SDL event queue without consuming events. Required
    // on macOS Cocoa for window composition; harmless elsewhere.
    fn pumpEvents() {
        SDL_PumpEvents()
    }

    // Milliseconds since SDL_Init. Wraps after ~49 days.
    fn getTicks(): u32 {
        SDL_GetTicks()
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

// Common SDL_EventType values (subset). The full list lives in
// SDL_events.h.
const EVENT_QUIT: u32       = 0x100
const EVENT_WINDOWEVENT: u32 = 0x200
const EVENT_KEYDOWN: u32    = 0x300
const EVENT_KEYUP: u32      = 0x301
