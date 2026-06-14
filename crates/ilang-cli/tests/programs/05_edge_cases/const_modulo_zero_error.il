// A statically ill-formed const — division / modulo by zero, an
// out-of-range shift — is a COMPILE error, not a value to demote to the
// runtime init path (which would panic at module init). The const folder
// previously returned a plain error string that the caller couldn't tell
// apart from "not a constant expression", so it fell through to runtime;
// the error type now distinguishes the two. The "in const expression"
// suffix is unique to the compile-time diagnostic (the runtime panic is
// "panic: modulo by zero / division by zero"), so matching it pins that
// this is caught at compile time.

// expect-error: modulo by zero in const expression

const X: i64 = 100 % (4 - 4)
console.log("unreachable")
