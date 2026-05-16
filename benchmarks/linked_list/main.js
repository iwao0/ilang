const N = 10_000_000;

class Node {
    constructor(value, next) {
        this.value = value;
        this.next = next;
    }
}

let head = null;
for (let i = 0; i < N; i++) {
    head = new Node(i, head);
}
let sum = 0;
for (let p = head; p; p = p.next) sum += p.value;
console.log(sum);
