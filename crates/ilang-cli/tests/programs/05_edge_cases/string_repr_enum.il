// `: string`-repr enums — each unit variant carries an explicit
// string discriminant (`= "literal"`). The value is reachable
// via `enumValue as string`; `match` still dispatches on the
// auto-assigned variant index. Useful for grouping SDL hint
// names / driver names / other string-keyed constant sets.
use std.test

pub enum Hint: string {
    audio = "SDL_AUDIO_RESAMPLING_MODE"
    video = "SDL_RENDER_DRIVER"
}

// Cast extracts the discriminant string.
test.expectStr(Hint.audio as string, "SDL_AUDIO_RESAMPLING_MODE")
test.expectStr(Hint.video as string, "SDL_RENDER_DRIVER")

// Match still works — dispatched by variant index, not by
// content compare.
let h = Hint.audio
let n = match h {
    audio { 1 }
    video { 2 }
}
test.expect(n, 1)

// Equality between two same-variant values evaluates true.
test.expectTrue(Hint.audio == Hint.audio)
test.expectTrue(Hint.video != Hint.audio)
