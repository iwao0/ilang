//! Built-in `fs` module — sync filesystem helpers backing the
//! `stdlib/fs.il` wrappers. Each exported symbol has a matching
//! `fn` declaration in `fs.il`; the user-facing API there wraps
//! these in `Result<T, FsError>` after consulting the thread-local
//! last-error slot this module maintains.
//!
//! Error contract: every fallible helper clears the last-error slot
//! on entry, sets `(code, message)` on failure (and returns a
//! sentinel — empty string / `0` / `false`), or leaves the slot
//! empty on success. The ilang wrapper queries the slot via the
//! `fs.__hasError` / `fs.__errorCode` / `fs.__errorMessage` trio.

use std::cell::RefCell;
use std::fs;
use std::io;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::arrays::{build_i64_array, __c_array_to_array};
use crate::kind::{KIND_NONE, KIND_STR};
use crate::strings::{cstr_bytes, leak_cstring};

thread_local! {
    static LAST_FS_ERROR: RefCell<Option<(String, String)>> = const { RefCell::new(None) };
}

fn clear_error() {
    LAST_FS_ERROR.with(|e| *e.borrow_mut() = None);
}

/// Map a `std::io::Error` to an `(code, message)` pair where `code`
/// is a stable string identifier — preference goes to the platform
/// errno name (`ENOENT`, `EACCES`, …) so users can branch on it the
/// way Node.js code does. Falls back to a generic name when the
/// errno is unavailable.
fn record_error(e: io::Error) {
    let code = match e.raw_os_error() {
        Some(2) => "ENOENT",
        Some(13) => "EACCES",
        Some(17) => "EEXIST",
        Some(20) => "ENOTDIR",
        Some(21) => "EISDIR",
        Some(22) => "EINVAL",
        Some(28) => "ENOSPC",
        Some(30) => "EROFS",
        Some(_) => match e.kind() {
            io::ErrorKind::NotFound => "ENOENT",
            io::ErrorKind::PermissionDenied => "EACCES",
            io::ErrorKind::AlreadyExists => "EEXIST",
            io::ErrorKind::InvalidInput => "EINVAL",
            _ => "EIO",
        },
        None => match e.kind() {
            io::ErrorKind::NotFound => "ENOENT",
            io::ErrorKind::PermissionDenied => "EACCES",
            io::ErrorKind::AlreadyExists => "EEXIST",
            io::ErrorKind::InvalidInput => "EINVAL",
            _ => "EIO",
        },
    };
    let msg = e.to_string();
    LAST_FS_ERROR.with(|slot| *slot.borrow_mut() = Some((code.to_string(), msg)));
}

fn read_path(name_ptr: i64) -> Option<String> {
    if name_ptr == 0 {
        record_error(io::Error::new(io::ErrorKind::InvalidInput, "null path"));
        return None;
    }
    let bytes = unsafe { cstr_bytes(name_ptr) };
    match std::str::from_utf8(bytes) {
        Ok(s) => Some(s.to_string()),
        Err(_) => {
            record_error(io::Error::new(io::ErrorKind::InvalidInput, "path is not valid UTF-8"));
            None
        }
    }
}

// ---------------------------------------------------------------
// Error-slot accessors
// ---------------------------------------------------------------

#[unsafe(export_name = "$fs.hasError")]
pub extern "C" fn fs_has_error() -> i64 {
    LAST_FS_ERROR.with(|e| if e.borrow().is_some() { 1 } else { 0 })
}

#[unsafe(export_name = "$fs.errorCode")]
pub extern "C" fn fs_error_code() -> i64 {
    LAST_FS_ERROR.with(|e| match e.borrow().as_ref() {
        Some((code, _)) => leak_cstring(code.clone()),
        None => leak_cstring(String::new()),
    })
}

#[unsafe(export_name = "$fs.errorMessage")]
pub extern "C" fn fs_error_message() -> i64 {
    LAST_FS_ERROR.with(|e| match e.borrow().as_ref() {
        Some((_, msg)) => leak_cstring(msg.clone()),
        None => leak_cstring(String::new()),
    })
}

// ---------------------------------------------------------------
// readFile / readFileBytes
// ---------------------------------------------------------------

#[unsafe(export_name = "$fs.readFile")]
pub extern "C" fn fs_read_file(path: i64) -> i64 {
    clear_error();
    let Some(p) = read_path(path) else {
        return leak_cstring(String::new());
    };
    match fs::read_to_string(&p) {
        Ok(s) => leak_cstring(s),
        Err(e) => {
            record_error(e);
            leak_cstring(String::new())
        }
    }
}

#[unsafe(export_name = "$fs.readFileBytes")]
pub extern "C" fn fs_read_file_bytes(path: i64) -> i64 {
    clear_error();
    let Some(p) = read_path(path) else {
        return build_i64_array(&[], KIND_NONE);
    };
    match fs::read(&p) {
        Ok(bytes) => {
            // Build a u8[] by copying the raw buffer into an ilang
            // dynamic array. stride = 1, kind_tag = KIND_NONE since
            // u8 is a primitive (no cascade).
            __c_array_to_array(
                bytes.as_ptr() as i64,
                bytes.len() as i64,
                1,
                KIND_NONE,
            )
        }
        Err(e) => {
            record_error(e);
            build_i64_array(&[], KIND_NONE)
        }
    }
}

// ---------------------------------------------------------------
// writeFile / writeFileBytes / appendFile
// ---------------------------------------------------------------

#[unsafe(export_name = "$fs.writeFile")]
pub extern "C" fn fs_write_file(path: i64, contents: i64) {
    clear_error();
    let Some(p) = read_path(path) else { return };
    let bytes = if contents == 0 {
        Vec::new()
    } else {
        unsafe { cstr_bytes(contents) }.to_vec()
    };
    if let Err(e) = fs::write(&p, &bytes) {
        record_error(e);
    }
}

#[unsafe(export_name = "$fs.writeFileBytes")]
pub extern "C" fn fs_write_file_bytes(path: i64, arr: i64) {
    clear_error();
    let Some(p) = read_path(path) else { return };
    // u8[] layout: [len | cap | data_ptr | rc | kind]
    let bytes: Vec<u8> = if arr == 0 {
        Vec::new()
    } else {
        unsafe {
            let len = *(arr as *const i64) as usize;
            let data_ptr = *((arr + 16) as *const i64) as *const u8;
            if len == 0 || data_ptr.is_null() {
                Vec::new()
            } else {
                std::slice::from_raw_parts(data_ptr, len).to_vec()
            }
        }
    };
    if let Err(e) = fs::write(&p, &bytes) {
        record_error(e);
    }
}

#[unsafe(export_name = "$fs.appendFile")]
pub extern "C" fn fs_append_file(path: i64, contents: i64) {
    clear_error();
    let Some(p) = read_path(path) else { return };
    let bytes = if contents == 0 {
        Vec::new()
    } else {
        unsafe { cstr_bytes(contents) }.to_vec()
    };
    use std::io::Write;
    let f = fs::OpenOptions::new().create(true).append(true).open(&p);
    match f {
        Ok(mut h) => {
            if let Err(e) = h.write_all(&bytes) {
                record_error(e);
            }
        }
        Err(e) => record_error(e),
    }
}

// ---------------------------------------------------------------
// exists / isFile / isDir
// ---------------------------------------------------------------

#[unsafe(export_name = "$fs.exists")]
pub extern "C" fn fs_exists(path: i64) -> i64 {
    let Some(p) = read_path(path) else { return 0 };
    if Path::new(&p).exists() { 1 } else { 0 }
}

#[unsafe(export_name = "$fs.isFile")]
pub extern "C" fn fs_is_file(path: i64) -> i64 {
    let Some(p) = read_path(path) else { return 0 };
    if Path::new(&p).is_file() { 1 } else { 0 }
}

#[unsafe(export_name = "$fs.isDir")]
pub extern "C" fn fs_is_dir(path: i64) -> i64 {
    let Some(p) = read_path(path) else { return 0 };
    if Path::new(&p).is_dir() { 1 } else { 0 }
}

// ---------------------------------------------------------------
// mkdir / rm / rmdir / rename
// ---------------------------------------------------------------

#[unsafe(export_name = "$fs.mkdir")]
pub extern "C" fn fs_mkdir(path: i64, recursive: i64) {
    clear_error();
    let Some(p) = read_path(path) else { return };
    let r = if recursive != 0 {
        fs::create_dir_all(&p)
    } else {
        fs::create_dir(&p)
    };
    if let Err(e) = r {
        record_error(e);
    }
}

#[unsafe(export_name = "$fs.rm")]
pub extern "C" fn fs_rm(path: i64) {
    clear_error();
    let Some(p) = read_path(path) else { return };
    if let Err(e) = fs::remove_file(&p) {
        record_error(e);
    }
}

#[unsafe(export_name = "$fs.rmdir")]
pub extern "C" fn fs_rmdir(path: i64, recursive: i64) {
    clear_error();
    let Some(p) = read_path(path) else { return };
    let r = if recursive != 0 {
        fs::remove_dir_all(&p)
    } else {
        fs::remove_dir(&p)
    };
    if let Err(e) = r {
        record_error(e);
    }
}

#[unsafe(export_name = "$fs.rename")]
pub extern "C" fn fs_rename(from: i64, to: i64) {
    clear_error();
    let Some(src) = read_path(from) else { return };
    let Some(dst) = read_path(to) else { return };
    if let Err(e) = fs::rename(&src, &dst) {
        record_error(e);
    }
}

// ---------------------------------------------------------------
// readDir / size
// ---------------------------------------------------------------

#[unsafe(export_name = "$fs.readDir")]
pub extern "C" fn fs_read_dir(path: i64) -> i64 {
    clear_error();
    let Some(p) = read_path(path) else {
        return build_i64_array(&[], KIND_STR);
    };
    let iter = match fs::read_dir(&p) {
        Ok(i) => i,
        Err(e) => {
            record_error(e);
            return build_i64_array(&[], KIND_STR);
        }
    };
    let mut entries: Vec<i64> = Vec::new();
    for entry in iter {
        match entry {
            Ok(de) => {
                let name = de.file_name().to_string_lossy().into_owned();
                entries.push(leak_cstring(name));
            }
            Err(e) => {
                record_error(e);
                return build_i64_array(&[], KIND_STR);
            }
        }
    }
    build_i64_array(&entries, KIND_STR)
}

#[unsafe(export_name = "$fs.size")]
pub extern "C" fn fs_size(path: i64) -> i64 {
    clear_error();
    let Some(p) = read_path(path) else { return -1 };
    match fs::metadata(&p) {
        Ok(m) => m.len() as i64,
        Err(e) => {
            record_error(e);
            -1
        }
    }
}

// ---------------------------------------------------------------
// stat / lstat — returns [size, mode, mtimeMs, atimeMs, ctimeMs,
// birthtimeMs, fileType] (length-7 i64[]) on success, empty array
// on error. `follow_links != 0` chooses `metadata` (stat); 0 uses
// `symlink_metadata` (lstat). The ilang wrapper unpacks the slots
// into a `Stats` object.
// ---------------------------------------------------------------

/// Convert an optional `SystemTime` into Unix epoch milliseconds.
/// Returns 0 for missing / unsupported / pre-epoch values — the
/// host platform may not support every timestamp (e.g. `created()`
/// on Linux before 4.11), so callers must treat 0 as "unknown".
fn system_time_to_ms(t: io::Result<SystemTime>) -> i64 {
    t.ok()
        .and_then(|st| st.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn stat_inner(path: i64, follow_links: bool) -> i64 {
    clear_error();
    let Some(p) = read_path(path) else {
        return build_i64_array(&[], KIND_NONE);
    };
    let meta = if follow_links {
        fs::metadata(&p)
    } else {
        fs::symlink_metadata(&p)
    };
    match meta {
        Ok(m) => {
            let size = m.len() as i64;
            // POSIX mode bits when available; on Windows fall back to
            // a synthetic mode derived from the read-only attribute.
            #[cfg(unix)]
            let mode = {
                use std::os::unix::fs::MetadataExt;
                m.mode() as i64
            };
            #[cfg(not(unix))]
            let mode: i64 = if m.permissions().readonly() {
                0o444
            } else {
                0o644
            };
            let mtime_ms = system_time_to_ms(m.modified());
            let atime_ms = system_time_to_ms(m.accessed());
            // POSIX `ctime` is inode-change time (matches Node's
            // `ctimeMs`); Windows has no equivalent so we mirror
            // `birthtime` there.
            #[cfg(unix)]
            let ctime_ms = {
                use std::os::unix::fs::MetadataExt;
                m.ctime() * 1000 + (m.ctime_nsec() / 1_000_000)
            };
            #[cfg(not(unix))]
            let ctime_ms = system_time_to_ms(m.created());
            let birthtime_ms = system_time_to_ms(m.created());
            let ft = m.file_type();
            let file_type: i64 = if ft.is_symlink() {
                3
            } else if ft.is_dir() {
                2
            } else if ft.is_file() {
                1
            } else {
                0
            };
            build_i64_array(
                &[
                    size,
                    mode,
                    mtime_ms,
                    atime_ms,
                    ctime_ms,
                    birthtime_ms,
                    file_type,
                ],
                KIND_NONE,
            )
        }
        Err(e) => {
            record_error(e);
            build_i64_array(&[], KIND_NONE)
        }
    }
}

#[unsafe(export_name = "$fs.stat")]
pub extern "C" fn fs_stat(path: i64) -> i64 {
    stat_inner(path, true)
}

#[unsafe(export_name = "$fs.lstat")]
pub extern "C" fn fs_lstat(path: i64) -> i64 {
    stat_inner(path, false)
}

// ---------------------------------------------------------------
// access — POSIX permission check. On Unix delegates to the libc
// `access(2)` syscall through a raw FFI declaration (no `libc`
// dep). On Windows we emulate F_OK via `exists` and W_OK via the
// read-only attribute; R_OK / X_OK always succeed there because
// Windows ACLs don't map cleanly to POSIX bits.
// ---------------------------------------------------------------

#[unsafe(export_name = "$fs.access")]
pub extern "C" fn fs_access(path: i64, mode: i64) {
    clear_error();
    let Some(p) = read_path(path) else { return };
    #[cfg(unix)]
    {
        use std::ffi::CString;
        unsafe extern "C" {
            fn access(path: *const std::os::raw::c_char, mode: i32) -> i32;
        }
        let cs = match CString::new(p.as_bytes()) {
            Ok(c) => c,
            Err(_) => {
                record_error(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "path contains interior NUL",
                ));
                return;
            }
        };
        let r = unsafe { access(cs.as_ptr(), mode as i32) };
        if r != 0 {
            record_error(io::Error::last_os_error());
        }
    }
    #[cfg(not(unix))]
    {
        let _ = mode;
        match fs::metadata(&p) {
            Ok(m) => {
                let m_i = mode as i32;
                // W_OK = 2: fail if readonly.
                if (m_i & 2) != 0 && m.permissions().readonly() {
                    record_error(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "read-only",
                    ));
                }
            }
            Err(e) => record_error(e),
        }
    }
}

// ---------------------------------------------------------------
// copyFile / cp / realpath
// ---------------------------------------------------------------

#[unsafe(export_name = "$fs.copyFile")]
pub extern "C" fn fs_copy_file(src: i64, dst: i64, overwrite: i64) {
    clear_error();
    let Some(s) = read_path(src) else { return };
    let Some(d) = read_path(dst) else { return };
    if overwrite == 0 && Path::new(&d).exists() {
        record_error(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "destination exists",
        ));
        return;
    }
    if let Err(e) = fs::copy(&s, &d) {
        record_error(e);
    }
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ft = entry.file_type()?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if ft.is_symlink() {
            // Copy symlinks as symlinks (Node's `cp` default).
            let target = fs::read_link(&from)?;
            #[cfg(unix)]
            {
                std::os::unix::fs::symlink(&target, &to)?;
            }
            #[cfg(windows)]
            {
                if Path::new(&target).is_dir() {
                    std::os::windows::fs::symlink_dir(&target, &to)?;
                } else {
                    std::os::windows::fs::symlink_file(&target, &to)?;
                }
            }
        } else if ft.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

#[unsafe(export_name = "$fs.cp")]
pub extern "C" fn fs_cp(src: i64, dst: i64, recursive: i64) {
    clear_error();
    let Some(s) = read_path(src) else { return };
    let Some(d) = read_path(dst) else { return };
    let r = if recursive != 0 {
        copy_dir_recursive(Path::new(&s), Path::new(&d))
    } else {
        fs::copy(&s, &d).map(|_| ())
    };
    if let Err(e) = r {
        record_error(e);
    }
}

#[unsafe(export_name = "$fs.realpath")]
pub extern "C" fn fs_realpath(path: i64) -> i64 {
    clear_error();
    let Some(p) = read_path(path) else {
        return leak_cstring(String::new());
    };
    match fs::canonicalize(&p) {
        Ok(buf) => leak_cstring(buf.to_string_lossy().into_owned()),
        Err(e) => {
            record_error(e);
            leak_cstring(String::new())
        }
    }
}

// ---------------------------------------------------------------
// chmod / truncate / utimes
// ---------------------------------------------------------------

#[unsafe(export_name = "$fs.chmod")]
pub extern "C" fn fs_chmod(path: i64, mode: i64) {
    clear_error();
    let Some(p) = read_path(path) else { return };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(mode as u32);
        if let Err(e) = fs::set_permissions(&p, perms) {
            record_error(e);
        }
    }
    #[cfg(not(unix))]
    {
        match fs::metadata(&p) {
            Ok(m) => {
                let mut perms = m.permissions();
                // S_IWUSR = 0o200 — clearing it on Windows flips the
                // read-only attribute.
                perms.set_readonly((mode as u32 & 0o200) == 0);
                if let Err(e) = fs::set_permissions(&p, perms) {
                    record_error(e);
                }
            }
            Err(e) => record_error(e),
        }
    }
}

#[unsafe(export_name = "$fs.truncate")]
pub extern "C" fn fs_truncate(path: i64, len: i64) {
    clear_error();
    let Some(p) = read_path(path) else { return };
    if len < 0 {
        record_error(io::Error::new(
            io::ErrorKind::InvalidInput,
            "truncate length must be non-negative",
        ));
        return;
    }
    match fs::OpenOptions::new().write(true).open(&p) {
        Ok(f) => {
            if let Err(e) = f.set_len(len as u64) {
                record_error(e);
            }
        }
        Err(e) => record_error(e),
    }
}

#[unsafe(export_name = "$fs.utimes")]
pub extern "C" fn fs_utimes(path: i64, atime_ms: i64, mtime_ms: i64) {
    clear_error();
    let Some(p) = read_path(path) else { return };
    let to_st = |ms: i64| -> SystemTime {
        if ms >= 0 {
            UNIX_EPOCH + std::time::Duration::from_millis(ms as u64)
        } else {
            UNIX_EPOCH - std::time::Duration::from_millis((-ms) as u64)
        }
    };
    let times = fs::FileTimes::new()
        .set_accessed(to_st(atime_ms))
        .set_modified(to_st(mtime_ms));
    match fs::OpenOptions::new().write(true).open(&p) {
        Ok(f) => {
            if let Err(e) = f.set_times(times) {
                record_error(e);
            }
        }
        Err(e) => record_error(e),
    }
}

// ---------------------------------------------------------------
// symlink / readlink / link
// ---------------------------------------------------------------

#[unsafe(export_name = "$fs.symlink")]
pub extern "C" fn fs_symlink(target: i64, link_path: i64) {
    clear_error();
    let Some(t) = read_path(target) else { return };
    let Some(l) = read_path(link_path) else { return };
    #[cfg(unix)]
    {
        if let Err(e) = std::os::unix::fs::symlink(&t, &l) {
            record_error(e);
        }
    }
    #[cfg(windows)]
    {
        let r = if Path::new(&t).is_dir() {
            std::os::windows::fs::symlink_dir(&t, &l)
        } else {
            std::os::windows::fs::symlink_file(&t, &l)
        };
        if let Err(e) = r {
            record_error(e);
        }
    }
}

#[unsafe(export_name = "$fs.readlink")]
pub extern "C" fn fs_readlink(path: i64) -> i64 {
    clear_error();
    let Some(p) = read_path(path) else {
        return leak_cstring(String::new());
    };
    match fs::read_link(&p) {
        Ok(buf) => leak_cstring(buf.to_string_lossy().into_owned()),
        Err(e) => {
            record_error(e);
            leak_cstring(String::new())
        }
    }
}

#[unsafe(export_name = "$fs.link")]
pub extern "C" fn fs_link(existing: i64, new_path: i64) {
    clear_error();
    let Some(e1) = read_path(existing) else { return };
    let Some(n) = read_path(new_path) else { return };
    if let Err(e) = fs::hard_link(&e1, &n) {
        record_error(e);
    }
}

// ---------------------------------------------------------------
// mkdtemp — create a uniquely-named temp directory. Suffix is 8
// characters from [a-zA-Z0-9_-], seeded from pid xor current nanos
// and stepped with an LCG. Retries up to 256 times on collision
// before giving up; that's enough for any realistic concurrent
// workload without pulling in `rand` / `tempfile`.
// ---------------------------------------------------------------

#[unsafe(export_name = "$fs.mkdtemp")]
pub extern "C" fn fs_mkdtemp(prefix: i64) -> i64 {
    clear_error();
    let Some(pre) = read_path(prefix) else {
        return leak_cstring(String::new());
    };
    let pid = std::process::id() as u64;
    let nanos: u64 = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let mut state = (pid << 32) ^ nanos;
    for _ in 0..256 {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let suffix: String = (0..8)
            .map(|i| {
                let v = ((state >> (i * 6)) & 0x3f) as u8;
                let b: u8 = match v {
                    0..=25 => b'a' + v,
                    26..=51 => b'A' + (v - 26),
                    52..=61 => b'0' + (v - 52),
                    62 => b'_',
                    _ => b'-',
                };
                b as char
            })
            .collect();
        let candidate = format!("{}{}", pre, suffix);
        match fs::create_dir(&candidate) {
            Ok(()) => return leak_cstring(candidate),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => {
                record_error(e);
                return leak_cstring(String::new());
            }
        }
    }
    record_error(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "mkdtemp: could not find unique temp name after 256 tries",
    ));
    leak_cstring(String::new())
}
