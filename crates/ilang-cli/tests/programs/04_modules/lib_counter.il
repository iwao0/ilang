// Sibling module exporting a class.

class Counter {
    n: i64
    init(start: i64) { this.n = start }
    bump() { this.n = this.n + 1 }
    add(k: i64) { this.n = this.n + k }
    get(): i64 { this.n }
}
