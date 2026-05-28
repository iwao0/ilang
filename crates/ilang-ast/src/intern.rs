//! Process-wide string interner.
//!
//! `Symbol` replaces `String` for identifier-like data in the AST
//! (variable / fn / class / type names, etc.). Equality is a `u32`
//! compare and copies are free, which matters because the same name
//! gets referenced many times across an AST and gets cloned through
//! the pipeline (parser → checker → mangle → codegen).
//!
//! The interner is process-global with a `RwLock`. `Symbol::intern`
//! takes a single read-lock on the fast path (already-interned) and
//! upgrades to a write-lock only on first insertion, so contention
//! is minimal in steady state.
//!
//! Strings, once interned, live for the lifetime of the process —
//! `Symbol::as_str` returns `&'static str` because the underlying
//! storage is `Box::leak`-ed. Acceptable for a compiler that runs
//! once per invocation; would not be for a long-lived REPL session
//! (the REPL re-uses one interner across inputs, so duplicate
//! identifiers are deduped — only genuinely new names accumulate).

use std::collections::HashMap;
use std::fmt;
use std::sync::{OnceLock, RwLock};

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Symbol(u32);

struct Interner {
    /// `&'static str` keys point into the same leaked allocations
    /// stored in `strings`, so the map doesn't keep separate copies.
    map: HashMap<&'static str, Symbol>,
    strings: Vec<&'static str>,
}

static INTERNER: OnceLock<RwLock<Interner>> = OnceLock::new();

fn interner() -> &'static RwLock<Interner> {
    INTERNER.get_or_init(|| {
        RwLock::new(Interner {
            map: HashMap::new(),
            strings: Vec::new(),
        })
    })
}

impl Symbol {
    /// Intern `s` and return its `Symbol`. Equal strings always
    /// produce the same Symbol.
    pub fn intern(s: &str) -> Self {
        // Fast path under a read lock.
        {
            let r = interner().read().expect("interner poisoned");
            if let Some(&sym) = r.map.get(s) {
                return sym;
            }
        }
        // Slow path: write lock, double-check (another thread may
        // have just inserted), then leak and insert.
        let mut w = interner().write().expect("interner poisoned");
        if let Some(&sym) = w.map.get(s) {
            return sym;
        }
        let leaked: &'static str = Box::leak(s.to_owned().into_boxed_str());
        let sym = Symbol(w.strings.len() as u32);
        w.strings.push(leaked);
        w.map.insert(leaked, sym);
        sym
    }

    /// Returns the interned text. The reference is `'static` because
    /// interned strings are leaked once and live for the rest of the
    /// process.
    ///
    /// Hot path: served from a per-thread cache with no global lock.
    /// The interner only ever *appends* (a `Symbol`'s string never
    /// changes once assigned), so a cached entry is valid for the rest
    /// of the process. Only a cache miss — the first time this thread
    /// sees a given `Symbol` index — takes the interner read lock, and
    /// then just to copy the newly-seen `&'static str` pointers across.
    /// `as_str` is called pervasively (every `Display` / `Debug` /
    /// `== "lit"` comparison routes through it), and the LSP resolves
    /// symbols from many worker threads at once, so the old
    /// per-call read lock was a real contention point.
    pub fn as_str(self) -> &'static str {
        let idx = self.0 as usize;
        STR_CACHE.with(|cache| {
            if let Some(&s) = cache.borrow().get(idx) {
                return s;
            }
            // Miss: catch this thread's cache up to the global
            // interner under a single read lock, then resolve.
            let mut cache = cache.borrow_mut();
            let r = interner().read().expect("interner poisoned");
            let have = cache.len();
            if have < r.strings.len() {
                cache.extend_from_slice(&r.strings[have..]);
            }
            cache[idx]
        })
    }
}

thread_local! {
    /// Per-thread `Symbol`-index → `&'static str` cache backing
    /// `Symbol::as_str` (see there). Append-only, mirroring the global
    /// interner; never reset because interned strings are never freed.
    static STR_CACHE: std::cell::RefCell<Vec<&'static str>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

impl fmt::Display for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Debug for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Render like a string literal so debug output stays readable.
        write!(f, "{:?}", self.as_str())
    }
}

impl From<&str> for Symbol {
    fn from(s: &str) -> Self {
        Symbol::intern(s)
    }
}

impl From<String> for Symbol {
    fn from(s: String) -> Self {
        Symbol::intern(&s)
    }
}

impl From<&String> for Symbol {
    fn from(s: &String) -> Self {
        Symbol::intern(s)
    }
}

impl AsRef<str> for Symbol {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

// Convenience comparisons so `if name == "init"` keeps working at
// call sites without forcing every site to write `name.as_str()`.
impl PartialEq<str> for Symbol {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for Symbol {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl PartialEq<Symbol> for str {
    fn eq(&self, other: &Symbol) -> bool {
        self == other.as_str()
    }
}

impl PartialEq<Symbol> for &str {
    fn eq(&self, other: &Symbol) -> bool {
        *self == other.as_str()
    }
}

impl PartialEq<String> for Symbol {
    fn eq(&self, other: &String) -> bool {
        self.as_str() == other.as_str()
    }
}
