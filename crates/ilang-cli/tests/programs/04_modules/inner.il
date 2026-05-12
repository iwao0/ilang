pub fn greeting(): string { "hello from inner" }
pub const MAGIC: i64 = 42

pub class Box {
    pub value: i64
    pub init(v: i64) { this.value = v }
}
