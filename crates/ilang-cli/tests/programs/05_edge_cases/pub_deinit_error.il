// `deinit` is the destructor — invoked by the ARC runtime when
// the rc reaches zero, never by user code. Marking it `pub`
// implies external callability that doesn't exist; the type
// checker rejects it so callers don't write misleading code.
//
// expect-error: `deinit` cannot be marked `pub`

class Foo {
    pub deinit() {}
}

let _ = new Foo()
