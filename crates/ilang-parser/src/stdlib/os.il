// Built-in `os` module — thin host-provided helpers for OS-level
// state that escapes the regular `@extern("lib")` ABI. For now just
// `errno()`; later additions (PID, env vars, signals) live here too.

// Returns the current value of the C `errno` variable (Windows:
// `GetLastError()`). Read after a failing libc call (e.g. `fopen`
// returned NULL) to see *why* it failed. Common values:
//   2  ENOENT (no such file)
//   13 EACCES (permission denied)
//   17 EEXIST (file exists)
// Resets to 0 on a fresh thread but is NOT auto-cleared between
// successful calls — convention is to read it only after a known
// failure indicator (NULL / -1 / etc.).
@extern fn errno(): i32
