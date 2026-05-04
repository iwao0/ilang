// SDL2 timing + event-pump helpers. Pumping events between sleeps
// is required on macOS so the OS run loop advances and the window
// actually paints.

@extern(C) {
    @lib("SDL2") fn SDL_Delay(ms: u32)
    @lib("SDL2") fn SDL_PumpEvents()

    fn delay(ms: u32) {
        SDL_Delay(ms)
    }

    // Drain the SDL event queue without consuming events. Required
    // on macOS Cocoa for window composition; harmless elsewhere.
    fn pumpEvents() {
        SDL_PumpEvents()
    }
}
