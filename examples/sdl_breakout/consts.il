// All compile-time tunables for the breakout, in one place.
// Names are camelCase so they read like local variables at the
// call site (`consts.winW` etc.). The loader inlines every const
// during merge, so these never carry a runtime cost.

// Window.
const winW: i32 = 900
const winH: i32 = 700

// Brick grid.
const cols: i32 = 9
const rows: i32 = 5
const brickW: i32 = 80
const brickH: i32 = 26
const brickGap: i32 = 8
const brickTop: i32 = 80

// Paddle / ball.
const paddleW: i32 = 110
const paddleH: i32 = 14
const paddleSpeed: f64 = 9.0
const ballSize: i32 = 14
// Length of each ball's trail ring buffer (`Ball.trail`).
const trailLen: i32 = 12

// Frame pacing.
const frameMs: u32 = 16
// Mono PCM sample rate for the SFX / BGM `AudioDevice`s.
const sampleRate: i32 = 44100

// Brick drop-in intro tuning. With stagger=4 and cols=9 the last
// column starts at frame 32 and finishes 24 frames later → ~0.93s
// total, ≈0.5s "perceived" since most bricks are already settled
// by then.
const introStagger: i32 = 4
const introDuration: i32 = 24
const introDropPx: f64 = 280.0

// Procedural-sprite atlas sizes.
const starTexSize: i32 = 32
const glowTexSize: i32 = 64

// Bloom downsample target. The scene is drawn shrunk into a
// `bloomW×bloomH` texture (= winW/4 × winH/4); thanks to linear
// filtering each pixel averages a 4×4 block. Drawing that
// upscaled back over the window blurs the highlights into a
// neon halo.
const bloomW: i32 = 225
const bloomH: i32 = 175
