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

#[unsafe(export_name = "fs.__hasError")]
pub extern "C" fn fs_has_error() -> i64 {
    LAST_FS_ERROR.with(|e| if e.borrow().is_some() { 1 } else { 0 })
}

#[unsafe(export_name = "fs.__errorCode")]
pub extern "C" fn fs_error_code() -> i64 {
    LAST_FS_ERROR.with(|e| match e.borrow().as_ref() {
        Some((code, _)) => leak_cstring(code.clone()),
        None => leak_cstring(String::new()),
    })
}

#[unsafe(export_name = "fs.__errorMessage")]
pub extern "C" fn fs_error_message() -> i64 {
    LAST_FS_ERROR.with(|e| match e.borrow().as_ref() {
        Some((_, msg)) => leak_cstring(msg.clone()),
        None => leak_cstring(String::new()),
    })
}

// ---------------------------------------------------------------
// readFile / readFileBytes
// ---------------------------------------------------------------

#[unsafe(export_name = "fs.__readFile")]
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

#[unsafe(export_name = "fs.__readFileBytes")]
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

#[unsafe(export_name = "fs.__writeFile")]
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

#[unsafe(export_name = "fs.__writeFileBytes")]
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

#[unsafe(export_name = "fs.__appendFile")]
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

#[unsafe(export_name = "fs.__exists")]
pub extern "C" fn fs_exists(path: i64) -> i64 {
    let Some(p) = read_path(path) else { return 0 };
    if Path::new(&p).exists() { 1 } else { 0 }
}

#[unsafe(export_name = "fs.__isFile")]
pub extern "C" fn fs_is_file(path: i64) -> i64 {
    let Some(p) = read_path(path) else { return 0 };
    if Path::new(&p).is_file() { 1 } else { 0 }
}

#[unsafe(export_name = "fs.__isDir")]
pub extern "C" fn fs_is_dir(path: i64) -> i64 {
    let Some(p) = read_path(path) else { return 0 };
    if Path::new(&p).is_dir() { 1 } else { 0 }
}

// ---------------------------------------------------------------
// mkdir / rm / rmdir / rename
// ---------------------------------------------------------------

#[unsafe(export_name = "fs.__mkdir")]
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

#[unsafe(export_name = "fs.__rm")]
pub extern "C" fn fs_rm(path: i64) {
    clear_error();
    let Some(p) = read_path(path) else { return };
    if let Err(e) = fs::remove_file(&p) {
        record_error(e);
    }
}

#[unsafe(export_name = "fs.__rmdir")]
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

#[unsafe(export_name = "fs.__rename")]
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

#[unsafe(export_name = "fs.__readDir")]
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

#[unsafe(export_name = "fs.__size")]
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
