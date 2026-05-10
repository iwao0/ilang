// Sibling module exporting a class.

pub class Counter {
    pub n: i64
    pub init(start: i64) { this.n = start }
    pub bump() { this.n = this.n + 1 }
    pub add(k: i64) { this.n = this.n + k }
    pub get(): i64 { this.n }
}
