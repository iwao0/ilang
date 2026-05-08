// Self-recursive method on a class whose fields hold `Self?`. Walks
// the tree via `if let some(...)`. Combines: optional unwrap on
// instance fields, recursive method dispatch (the same vtable slot
// re-entered), and value-returning `if let` body.
use test

class Tree {
    v: i64
    left: Tree?
    right: Tree?
    init(v: i64, l: Tree?, r: Tree?) {
        this.v = v
        this.left = l
        this.right = r
    }
    sum(): i64 {
        let total = this.v
        if let some(l) = this.left { total = total + l.sum() }
        if let some(r) = this.right { total = total + r.sum() }
        total
    }
    depth(): i64 {
        let ld = if let some(l) = this.left { l.depth() } else { 0 }
        let rd = if let some(r) = this.right { r.depth() } else { 0 }
        let m = if ld > rd { ld } else { rd }
        m + 1
    }
}

// Build:
//        10
//       /  \
//      2    3
//            \
//             1
let leaf = new Tree(1, none, none)
let r = new Tree(3, none, leaf)
let l = new Tree(2, none, none)
let root = new Tree(10, l, r)

test.expect(root.sum(), 16)         // 10+2+3+1
test.expect(root.depth(), 3)
test.expect(l.depth(), 1)
test.expect(leaf.depth(), 1)

// Single-node tree.
let single = new Tree(42, none, none)
test.expect(single.sum(), 42)
test.expect(single.depth(), 1)
