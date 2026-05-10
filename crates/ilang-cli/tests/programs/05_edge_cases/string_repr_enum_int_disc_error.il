// On a `: string`-repr enum every variant must declare its
// discriminant as a string literal. Mixing in an integer
// discriminant is a lower-time error.
//
// jit: skip
// expect-error: integer discriminant used on a `: string` repr enum

pub enum H: string {
    audio = "SDL_AUDIO"
    video = 0x10
}
