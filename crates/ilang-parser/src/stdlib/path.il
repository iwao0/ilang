// Built-in `path` module — Node.js-inspired path manipulation.
//
// All operations treat `/` as the only separator and never touch
// the filesystem. The shape mirrors Node.js's `path` (POSIX), so
// `basename` / `dirname` / `extname` / `join` / `isAbsolute` /
// `normalize` / `relative` / `parse` / `format` behave the same
// way Node does on POSIX-style paths.
//
// Cross-platform note: even on Windows the separator stays `/`.
// Pass `\`-style paths through `replace("\\", "/")` first if you
// have them. (Native Windows backslash handling isn't supported —
// pick one path style and stick to it.)
//
// Pure ilang implementation, no FFI; safe to call from anywhere.

/// Path separator. Always `"/"` (cross-platform; see module note).
pub const sep: string = "/"

/// PATH-list separator. Always `":"` — matches Node.js's POSIX
/// `path.delimiter`. Use this when splitting `$PATH`-style env vars.
pub const delimiter: string = ":"

/// `true` when `p` begins with `/`. Empty string returns `false`.
pub fn isAbsolute(p: string): bool {
    p.length > 0 && p.startsWith("/")
}

/// Last path segment. Trailing separators are ignored, so
/// `basename("/a/b/")` returns `"b"`. Empty string returns `""`.
pub fn basename(p: string): string {
    let parts = p.split("/")
    let n = parts.length
    if n == 0 { return "" }
    // Walk back to the last non-empty segment to skip trailing /'s.
    let i = n - 1
    while i >= 0 && parts[i] == "" { i = i - 1 }
    if i < 0 { "" } else {
        // Bind to a local before returning — returning `parts[i]`
        // directly hands back a pointer into the array, which the
        // caller may read after `parts` is dropped (and its
        // contained strings released along with it).
        let s = parts[i]
        s
    }
}

/// `basename(p)` with `ext` stripped from the result if it appears
/// at the end. Mirrors Node's `path.basename(p, ext)`.
pub fn basename(p: string, ext: string): string {
    let b = basename(p)
    if ext.length > 0 && b.endsWith(ext) {
        b.slice(0 as i64, b.length - ext.length)
    } else {
        b
    }
}

/// Path up to but not including the last segment. `dirname("/a")`
/// returns `"/"`, `dirname("a")` returns `"."`, `dirname("/")`
/// returns `"/"`. Trailing separators are ignored.
pub fn dirname(p: string): string {
    if p.length == 0 { return "." }
    let parts = p.split("/")
    let n = parts.length
    // Trim trailing empty segments so `"/a/b/"` → `["", "a", "b"]`.
    let last = n - 1
    while last > 0 && parts[last] == "" { last = last - 1 }
    if last == 0 {
        // "/foo" → ["", "foo"], last=1 → wait that's last>0 check.
        // "" already returned above. "/" → ["", ""], last=0.
        if parts[0] == "" { "/" } else { "." }
    } else {
        // Join parts[0..last-1] inclusive with "/" — that's
        // everything except the final segment.
        let upto = last - 1
        // "/foo" → ["", "foo"], last=1, upto=0 → empty join → "/"
        if upto == 0 && parts[0] == "" { "/" }
        else { joinFirst(parts, upto + 1) }
    }
}

// Join the first `count` segments of `parts` with "/".
fn joinFirst(parts: string[], count: i64): string {
    let acc = ""
    for i in 0..count {
        if i > 0 { acc = acc + "/" }
        acc = acc + parts[i]
    }
    acc
}

/// File extension including the leading dot. `extname("a.tar.gz")`
/// returns `".gz"`, `extname("a")` returns `""`. A leading dot
/// without a name (e.g. `".bashrc"`) is treated as the file name,
/// not an extension — matches Node.
pub fn extname(p: string): string {
    let b = basename(p)
    if b.length == 0 { return "" }
    // Walk backward looking for the last `.`. If we hit the start
    // before finding one — or the only `.` is at index 0 — the
    // file has no extension.
    let i = b.length - 1
    while i > 0 {
        if b.charAt(i) == "." {
            return b.slice(i, b.length)
        }
        i = i - 1
    }
    ""
}

/// Join multiple segments with `/`. Empty segments are skipped.
/// The result is normalised (collapses `..`, `.`, repeated `/`).
pub fn join(parts: string[]): string {
    if parts.length == 0 { return "." }
    let acc = ""
    let n = parts.length
    for i in 0..n {
        if parts[i].length == 0 { continue }
        if acc.length == 0 { acc = parts[i] }
        else { acc = acc + "/" + parts[i] }
    }
    if acc.length == 0 { "." } else { normalize(acc) }
}

/// Collapse `.` segments, resolve `..` segments, fold repeated
/// separators. The leading `/` (absolute) and trailing `/` of the
/// input are preserved when present.
pub fn normalize(p: string): string {
    if p.length == 0 { return "." }
    let isAbs = isAbsolute(p)
    let trailing = p.length > 1 && p.endsWith("/")
    let parts = p.split("/")
    let stack: string[] = []
    let n = parts.length
    for i in 0..n {
        let s = parts[i]
        if s.length == 0 { continue }       // collapse `//`
        if s == "." { continue }
        if s == ".." {
            let top = stack.length
            if top > 0 && stack[top - 1] != ".." {
                let _ = stack.pop()
                continue
            }
            // Absolute path: `..` at the root is a no-op.
            if isAbs { continue }
            stack.push("..")
            continue
        }
        stack.push(s)
    }
    let body = joinFirst(stack, stack.length)
    let prefix = if isAbs { "/" } else { "" }
    let suffix = if trailing && body.length > 0 { "/" } else { "" }
    let combined = prefix + body + suffix
    if combined.length == 0 { "." } else { combined }
}

/// Relative path from `from` to `to`. Both paths are normalised
/// before comparing. The result, when joined with `from` and
/// normalised, yields `to`.
pub fn relative(from: string, to: string): string {
    let f = normalize(from)
    let t = normalize(to)
    if f == t { return "" }
    let fp = f.split("/")
    let tp = t.split("/")
    // Skip leading empty segments from absolute paths so the common
    // prefix walks the actual segments.
    let fi = 0
    let ti = 0
    if fp.length > 0 && fp[0] == "" { fi = 1 }
    if tp.length > 0 && tp[0] == "" { ti = 1 }
    // Walk common prefix.
    while fi < fp.length && ti < tp.length && fp[fi] == tp[ti] {
        fi = fi + 1
        ti = ti + 1
    }
    // Each remaining `from` segment becomes a `..`. Then append
    // the remaining `to` segments.
    let acc = ""
    let remFrom = fp.length - fi
    let remTo = tp.length - ti
    for _i in 0..remFrom {
        if acc.length > 0 { acc = acc + "/" }
        acc = acc + ".."
    }
    let i = ti
    while i < tp.length {
        if acc.length > 0 { acc = acc + "/" }
        acc = acc + tp[i]
        i = i + 1
    }
    acc
}

/// Decomposition of a path into its constituent parts. `format`
/// is the inverse: `format(parse(p))` reconstructs an equivalent
/// path (modulo input quirks like trailing `/` or `.` segments).
pub class PathParts {
    /// Directory portion (`dirname(p)`), without the trailing `/`.
    pub dir: string
    /// `"/"` for absolute paths, `""` for relative.
    pub root: string
    /// Last segment (`basename(p)`).
    pub base: string
    /// `base` minus its extension.
    pub name: string
    /// Extension including leading dot (`extname(p)`).
    pub ext: string

    pub init(dir: string, root: string, base: string, name: string, ext: string) {
        this.dir = dir
        this.root = root
        this.base = base
        this.name = name
        this.ext = ext
    }
}

/// Split a path into `{ dir, root, base, name, ext }`.
pub fn parse(p: string): PathParts {
    // The arguments below are passed *directly* to `new` (no
    // intermediate `let s = ...; new PathParts(s, ...)` shape) —
    // the compiler currently misses the retain in that pattern,
    // so the field would end up dangling. Threading the call
    // expressions straight in dodges the bug.
    new PathParts(dirname(p), parseRoot(p), basename(p), parseName(p), extname(p))
}

fn parseRoot(p: string): string {
    if isAbsolute(p) { "/" } else { "" }
}

fn parseName(p: string): string {
    let base = basename(p)
    let ext = extname(p)
    if ext.length > 0 {
        base.slice(0 as i64, base.length - ext.length)
    } else {
        base
    }
}

/// Re-assemble a `PathParts`. `base` (when non-empty) wins over
/// `name + ext`; matches Node.js's precedence.
pub fn format(parts: PathParts): string {
    let base = if parts.base.length > 0 {
        parts.base
    } else {
        parts.name + parts.ext
    }
    if parts.dir.length == 0 { return base }
    if parts.dir == "/" { return "/" + base }
    parts.dir + "/" + base
}
