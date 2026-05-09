// Heap-typed field reassignment must release the previous occupant
// before storing the new value, otherwise the old value (and any
// resources it owns) leaks until program exit. Covers Object,
// Array<T>, and Optional<T> field types — patterns lifted from
// `examples/sdl_breakout`'s per-frame
// `this.particles = aliveP` / `this.balls = surviving` swaps,
// which previously kept every prior frame's particle data alive.
//
// Pre-fix the JIT's AssignField path didn't snapshot+release the
// old field, so deinits piled up at program exit instead of firing
// at the swap point.
//
// jit: skip
//
// expect: --- before swap ---
// expect: deinit obj-1
// expect: deinit arr-1a
// expect: deinit arr-1b
// expect: deinit opt-1
// expect: --- after swap ---
// expect: deinit obj-2
// expect: deinit arr-2a
// expect: deinit opt-2

class T {
    name: string
    init(n: string) { this.name = n }
    deinit() { console.log("deinit " + this.name) }
}

class Box {
    obj: T
    arr: T[]
    opt: T?
    init() {
        this.obj = new T("obj-1")
        this.arr = [new T("arr-1a"), new T("arr-1b")]
        this.opt = some(new T("opt-1"))
    }
    swap() {
        // Each assignment must drop the prior value's deinit
        // immediately, NOT defer it to the surrounding object's
        // own drop at program exit.
        this.obj = new T("obj-2")
        this.arr = [new T("arr-2a")]
        this.opt = some(new T("opt-2"))
    }
}

let b = new Box()
console.log("--- before swap ---")
b.swap()
console.log("--- after swap ---")
// Program exits here — `b` releases, cascading deinit on
// the post-swap occupants (obj-2 / arr-2a / opt-2).
