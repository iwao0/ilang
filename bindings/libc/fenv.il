/// C99 `<fenv.h>` — floating-point environment.
///
/// Lets a program query and tweak the current rounding mode and
/// FP exception state. Used by numerics code that wants to detect
/// inexact / overflow / divide-by-zero without per-op branches.
///
/// Rounding modes (x86 / x86_64 / aarch64 — the values are
/// portable across these targets but spelled with FE_ macros in
/// C; the constants below are the conventional fenv.h values):
///   FE_TONEAREST   0x0000   (default IEEE-754 round-to-nearest-even)
///   FE_DOWNWARD    0x0400
///   FE_UPWARD      0x0800
///   FE_TOWARDZERO  0x0C00
///
/// Exception bits (OR-able into a single `excepts` argument):
///   FE_INVALID     0x01
///   FE_DIVBYZERO   0x04
///   FE_OVERFLOW    0x08
///   FE_UNDERFLOW   0x10
///   FE_INEXACT     0x20
///   FE_ALL_EXCEPT  0x3D
///
/// All entry points return 0 on success.

pub enum FeRound: i32 {
    toNearest  = 0x0000
    downward   = 0x0400
    upward     = 0x0800
    towardZero = 0x0C00
}

@flags
pub enum FeExcept: i32 {
    invalid   = 0x01
    divByZero = 0x04
    overflow  = 0x08
    underflow = 0x10
    inexact   = 0x20
    /// FE_INVALID | FE_DIVBYZERO | FE_OVERFLOW | FE_UNDERFLOW | FE_INEXACT.
    all       = 0x3D
}

@extern(C, "c") {
    /// Opaque fenv_t — exact layout is implementation-defined.
    /// 64 bytes is enough for every host we care about (glibc x86_64
    /// uses 32; ucrt uses 28).
    pub struct fenv_t {
        _opaque: u8[64]
    }

    /// Opaque fexcept_t — typically 2 or 4 bytes; we pad to 16 to
    /// stay future-proof and aligned.
    pub struct fexcept_t {
        _opaque: u8[16]
    }

    // ── Rounding mode ─────────────────────────────────────────

    /// Current rounding mode (FeRound.*). Returns the FE_* value.
    @lib @optional pub fn fegetround(): i32

    /// Install `round` as the rounding mode. Returns 0 on success.
    @lib @optional pub fn fesetround(round: i32): i32

    // ── Exception flags ───────────────────────────────────────

    /// Clear the bits named in `excepts`.
    @lib @optional pub fn feclearexcept(excepts: i32): i32

    /// Raise the bits named in `excepts` (sticky — they stay set
    /// until cleared).
    @lib @optional pub fn feraiseexcept(excepts: i32): i32

    /// Returns the subset of `excepts` that are currently set.
    @lib @optional pub fn fetestexcept(excepts: i32): i32

    @lib @optional pub fn fegetexceptflag(flagp: *fexcept_t, excepts: i32): i32
    @lib @optional pub fn fesetexceptflag(flagp: *const fexcept_t, excepts: i32): i32

    // ── Whole-environment save / restore ──────────────────────

    @lib @optional pub fn fegetenv(envp: *fenv_t): i32
    @lib @optional pub fn fesetenv(envp: *const fenv_t): i32

    /// Restore `envp` and re-raise any exceptions that were set
    /// in the calling environment before the save.
    @lib @optional pub fn feupdateenv(envp: *const fenv_t): i32

    /// Save the current environment to `envp` and clear all
    /// exception flags. Pair with `feupdateenv` to wrap a region
    /// that mustn't leak FP exceptions.
    @lib @optional pub fn feholdexcept(envp: *fenv_t): i32
}
