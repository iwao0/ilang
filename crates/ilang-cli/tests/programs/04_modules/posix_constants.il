// POSIX constant values exposed through the `os` module. Verifies
// the values agree with the syscall layer on this platform —
// values that differ across Linux/macOS are intentionally omitted
// from the stdlib.
use os
use test

// Standard fds.
test.expect(os.STDIN_FILENO as i64, 0)
test.expect(os.STDOUT_FILENO as i64, 1)
test.expect(os.STDERR_FILENO as i64, 2)

// Exit codes.
test.expect(os.EXIT_SUCCESS as i64, 0)
test.expect(os.EXIT_FAILURE as i64, 1)

// lseek whence.
test.expect(os.SEEK_SET as i64, 0)
test.expect(os.SEEK_CUR as i64, 1)
test.expect(os.SEEK_END as i64, 2)

// File access modes.
test.expect(os.O_RDONLY as i64, 0)
test.expect(os.O_WRONLY as i64, 1)
test.expect(os.O_RDWR as i64, 2)

// File mode bits (POSIX).
test.expect(os.S_IRUSR as i64, 256)
test.expect(os.S_IWUSR as i64, 128)
test.expect(os.S_IXUSR as i64, 64)
test.expect(os.S_IROTH as i64, 4)

// Socket types.
test.expect(os.SOCK_STREAM as i64, 1)
test.expect(os.SOCK_DGRAM as i64, 2)

// Common signals.
test.expect(os.SIGINT as i64, 2)
test.expect(os.SIGKILL as i64, 9)
test.expect(os.SIGTERM as i64, 15)
