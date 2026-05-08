// Parent-child structure where the child holds a `Parent.weak` to
// avoid a strong cycle. Walking from a child through `weak.get()`
// must yield the live parent (Some); after the parent's only
// strong owner drops, future `get()` calls return None instead.
// `weak_basic.il` exercises the single-class case; this fixture
// adds the realistic multi-child + reverse-walk shape.
use test

class Parent {
    children: Child[]
    name: string
    init(n: string) {
        this.children = []
        this.name = n
    }
    add(c: Child) {
        this.children.push(c)
        c.parent = this
    }
}

class Child {
    parent: Parent.weak
    label: string
    init(l: string) { this.label = l }
}

let p = new Parent("root")
let c1 = new Child("c1")
let c2 = new Child("c2")
p.add(c1)
p.add(c2)

test.expect(p.children.length, 2)

// Walk back through weak ref.
test.expectBool(c1.parent.get().isSome, true)
test.expectStr(c1.parent.get().unwrap().name, "root")
test.expectStr(c2.parent.get().unwrap().name, "root")

// Multiple children share the same upgraded parent.
let p1 = c1.parent.get().unwrap()
let p2 = c2.parent.get().unwrap()
test.expectBool(p1 == p2, true)

// Iterate via parent → children → label.
let labels = ""
for c in p.children {
    labels = labels + c.label + ","
}
test.expectStr(labels, "c1,c2,")
