/// winnt.h — Win32 base type system: the universal kernel object
/// HANDLE and the access-mask / security primitives shared across
/// every API surface.
///
/// In the Windows SDK this header is `#include`d transitively by
/// every other Win32 header; in ilang we mirror it as its own file
/// so that "where does HANDLE live?" has a single answer.

@extern(C) {
    /// Generic kernel-object handle. Returned by CreateFile,
    /// CreateEvent, OpenProcess, OpenThread, CreateMutex,
    /// CreateSemaphore, etc. Closed via `kernel32.CloseHandle`.
    ///
    /// Special values:
    ///   INVALID_HANDLE_VALUE = (HANDLE)-1   — file-API failure sentinel
    ///   NULL                                 — most other APIs' failure sentinel
    @handle pub struct HANDLE {}
}
