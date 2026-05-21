/// C standard `<setjmp.h>` — non-local control transfer.
///
/// `jmp_buf` is implementation-defined and its size varies wildly
/// across platforms (glibc x86_64: 200 bytes; macOS: 192; ucrt:
/// 256). We expose it as an opaque 512-byte cookie — big enough on
/// every supported target. Pass the buffer by pointer.
///
/// Usage pattern:
///
///   let mut env: jmp_buf = jmp_buf{}
///   if setjmp(&env) == 0 {
///       // setup path
///       longjmp(&env, 1)   // never returns here
///   } else {
///       // returned from longjmp; the value passed becomes the
///       // setjmp return value (a 0 second argument is promoted
///       // to 1 by spec).
///   }
///
/// `setjmp` is technically a macro in C; the underlying symbols are
/// commonly `setjmp` / `_setjmp` (glibc keeps both). On Windows the
/// symbol is `_setjmp` but ucrt also forwards `setjmp`.

@extern(C, "c") {
    /// 512-byte opaque buffer; big enough for every supported target.
    pub struct jmp_buf {
        _opaque: u8[512]
    }

    /// Record the calling environment in `env`. Returns 0 on the
    /// direct call, the second argument to `longjmp` on the resumed
    /// call.
    @lib @optional pub fn setjmp(env: *jmp_buf): i32

    /// Jump back to the matching `setjmp(env)`. Never returns. `val`
    /// becomes the resumed `setjmp` return value (0 is promoted to 1).
    @lib @optional pub fn longjmp(env: *jmp_buf, val: i32)
}
