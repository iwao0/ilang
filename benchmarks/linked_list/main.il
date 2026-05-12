const N: i64 = 1000000

class Node {
    pub value: i64
    pub next: Node?
    pub init(v: i64, nx: Node?) {
        this.value = v
        this.next = nx
    }
}

let mut_head: Node? = none
for i in 0..N {
    mut_head = some(new Node(i, mut_head))
}

let mut_sum: i64 = 0
let mut_p: Node? = mut_head
while mut_p.isSome {
    let n: Node = mut_p.unwrap()
    mut_sum = mut_sum + n.value
    mut_p = n.next
}
console.log(mut_sum)
