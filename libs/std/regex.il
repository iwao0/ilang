/// Built-in `regex` module — regular-expression matching via the
/// Rust `regex` crate. Compile a pattern once with `new Regex(...)`
/// and reuse the resulting object; the wrapped engine drops when
/// the object's ARC refcount reaches zero.
///
/// Backrefs (`\1`) and lookaround (`(?=...)`) aren't supported —
/// the wrapped engine is the linear-time `regex` crate, not PCRE.
///
/// Flags: each character in the `flags` string toggles one option.
///   i  case-insensitive
///   m  `^` / `$` match line boundaries (multi-line)
///   s  `.` matches newlines (dot-all)
///   x  ignore whitespace + `#` comments in the pattern

@intrinsic("regex.compile")
fn regex_compile(pattern: string, flags: string): i64
@intrinsic("regex.destroy")
fn regex_destroy(handle: i64)
@intrinsic("regex.test")
fn regex_test(handle: i64, s: string): i32
@intrinsic("regex.has_match")
fn regex_has_match(handle: i64, s: string): i32
@intrinsic("regex.first_match")
fn regex_first_match(handle: i64, s: string): string
@intrinsic("regex.replace_all")
fn regex_replace_all(handle: i64, s: string, with: string): string
@intrinsic("regex.find_all")
fn regex_find_all(handle: i64, s: string): string[]
@intrinsic("regex.split")
fn regex_split(handle: i64, s: string): string[]

/// Compiled regular expression. Construct with a pattern and a
/// flags string (see module doc for the recognised characters);
/// reuse the instance across as many match calls as needed.
pub class Regex {
    handle: i64

    /// Compile `pattern` with the given `flags`. Panics if the
    /// pattern fails to compile — validate untrusted input before
    /// passing it in. Pass `""` for `flags` to use defaults.
    pub init(pattern: string, flags: string) {
        this.handle = regex_compile(pattern, flags)
    }

    /// `true` when `s` contains at least one match.
    pub test(s: string): bool {
        regex_test(this.handle, s) != 0
    }

    /// First match as a substring, or `none` when nothing matches.
    pub find(s: string): string? {
        if regex_has_match(this.handle, s) == 0 {
            return none
        }
        some(regex_first_match(this.handle, s))
    }

    /// Every match in `s`, in left-to-right non-overlapping order.
    pub findAll(s: string): string[] {
        regex_find_all(this.handle, s)
    }

    /// Replace every non-overlapping match with `replacement`.
    /// `$1`, `$2`, ... in `replacement` substitute capture groups.
    pub replace(s: string, replacement: string): string {
        regex_replace_all(this.handle, s, replacement)
    }

    /// Split `s` on every match. The substrings between matches
    /// (including empty leading / trailing chunks) are returned.
    pub split(s: string): string[] {
        regex_split(this.handle, s)
    }

    /// Releases the host-side compiled pattern. Runs automatically
    /// when the ARC refcount reaches zero.
    deinit() {
        regex_destroy(this.handle)
    }
}
