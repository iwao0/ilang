// A self-referential enum payload — `node` carries `Tree` recursively,
// so the payload is a heap-allocated boxed value. Match destructuring
// must keep the bindings alive across the arm body and the recursion
// has to terminate cleanly at `leaf`.
use std.test as test
enum Tree {
    leaf
    node: (Tree, i64, Tree)
}

fn sum(t: Tree): i64 {
    match t {
        leaf { 0 }
        node(l, v, r) { sum(l) + v + sum(r) }
    }
}

fn depth(t: Tree): i64 {
    match t {
        leaf { 0 }
        node(l, _v, r) {
            let dl = depth(l)
            let dr = depth(r)
            (if dl > dr { dl } else { dr }) + 1
        }
    }
}

fn count(t: Tree): i64 {
    match t {
        leaf { 0 }
        node(l, _v, r) { count(l) + count(r) + 1 }
    }
}

// Build:        2
//              / \
//             1   3
//                / \
//               leaf 4
let inner = Tree.node(Tree.leaf, 4, Tree.leaf)
let right = Tree.node(Tree.leaf, 3, inner)
let tree = Tree.node(
    Tree.node(Tree.leaf, 1, Tree.leaf),
    2,
    right
)

test.expect(sum(tree), 10)             // 1 + 2 + 3 + 4
test.expect(count(tree), 4)
test.expect(depth(tree), 3)

// Single leaf.
test.expect(sum(Tree.leaf), 0)
test.expect(depth(Tree.leaf), 0)
test.expect(count(Tree.leaf), 0)
