// A generic class whose method names a STRICTLY DEEPER instantiation of
// itself (`Wrap<T>.doubled(): Wrap<Wrap<T>>`) can never finish
// monomorphizing — each level is a distinct concrete type, so the
// worklist grows forever. The substitution is eager (every method of an
// instantiated class is scanned even if never called), so merely
// constructing `new Wrap<i64>(7)` triggers it; the compiler used to hang.
// It now bails with a clear "monomorphization limit exceeded" error
// instead of spinning.

// expect-error: monomorphization limit exceeded

class Wrap<T> {
    pub v: T
    init(v: T) { this.v = v }
    doubled(): Wrap<Wrap<T>> { new Wrap<Wrap<T>>(new Wrap<T>(this.v)) }
}

let w = new Wrap<i64>(7)
console.log(w.v.toString())
