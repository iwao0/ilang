// `let w: T.weak = <expr>` where <expr> is a fresh-classified source —
// an if/else or match join, or a bare `new T()`. The join retains its
// strong branch value and a `new` owns its alloc +1; the StrongToWeak
// coerce then drops that strong +1, so the weak must take its own weak
// share (BEFORE the strong drop, or the box frees first). Without it the
// box dies under the live weak — a use-after-free.
//
// Deterministic in normal mode via slot reuse: a recycled slot makes a
// dangling `w.get()` upgrade to a bogus `some`.
//
// expect: ifelse none
// expect: match none
// expect: fresh none

class Tag {
    v: i64
    init(x: i64) { this.v = x }
}
enum E { x, y }

fn fromIf(c: bool): Tag.weak {
    let a = new Tag(1)
    let b = new Tag(2)
    let w: Tag.weak = if c { a } else { b }
    w
}
fn fromMatch(e: E): Tag.weak {
    let a = new Tag(3)
    let w: Tag.weak = match e {
        x { a }
        y { a }
    }
    w
}
fn fromFresh(): Tag.weak {
    let w: Tag.weak = new Tag(4)
    w
}

fn check(label: string, w: Tag.weak) {
    let filler: Tag[] = []
    let i = 0
    while i < 8 {
        filler.push(new Tag(999))
        i = i + 1
    }
    match w.get() {
        some(t) { console.log(`${label} BAD some v=${t.v}`) }
        none { console.log(`${label} none`) }
    }
}

check("ifelse", fromIf(true))
check("match", fromMatch(E.x))
check("fresh", fromFresh())
