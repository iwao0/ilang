// An array passed to a function with a typed `xs: T[]` parameter must
// alias the caller's binding — mutations through the parameter (index
// assign, push, pop) have to be visible to the caller. Pre-fix the
// interpreter's `cast_value` always allocated a fresh `Rc` for typed
// array parameters, silently breaking this contract while the JIT
// kept aliasing intact.
use test

fn mutate(xs: i64[]) {
    xs[0] = 999
    xs.push(42)
}

let arr: i64[] = [1, 2, 3]
mutate(arr)
test.expect(arr[0], 999)
test.expect(arr.length, 4)
test.expect(arr[3], 42)

// Same shape with a class-element array.
class Counter {
    n: i64
    init() { this.n = 0 }
    bump() { this.n = this.n + 1 }
}

fn bump_first(cs: Counter[]) {
    cs[0].bump()
    cs[0].bump()
}

let cs: Counter[] = [new Counter(), new Counter()]
bump_first(cs)
test.expect(cs[0].n, 2)
test.expect(cs[1].n, 0)

// And: a class field of type `T[]` initialized from a constructor
// parameter must keep aliasing with the caller's array — otherwise
// `deinit` callbacks that push into a shared log array would land
// in a private copy and never be observed.
class T {
    log: i64[]
    init(l: i64[]) { this.log = l }
    deinit() { this.log.push(99) }
}

let log: i64[] = []
{
    let _x = new T(log)
}
test.expect(log.length, 1)
test.expect(log[0], 99)
