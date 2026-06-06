// Set/Map with object keys must consult the user's `equals` when two
// distinct keys land in the same hash bucket. A coarse `hashCode`
// forces collisions on purpose. The runtime called `equals` through a
// function pointer typed `-> i64` while the method returns `bool` (low
// byte only); on SysV x86_64 the upper bits of the return register are
// undefined, so the truthiness test read garbage and collapsed (or
// dropped) distinct keys. Fixed by masking the result to the low byte.
// expect: 3
// expect: true
// expect: false
// expect: 3
// expect: some(20)

class Key {
    pub n: i64
    pub init(n: i64) { this.n = n }
    pub hashCode(): i64 { 0 }                       // every key collides
    pub equals(other: Key): bool { this.n == other.n }
}

let s = new Set<Key>()
s.add(new Key(1))
s.add(new Key(2))
s.add(new Key(3))
s.add(new Key(2))            // structural dup → ignored
console.log(s.size())        // 3
console.log(s.has(new Key(2)))   // true
console.log(s.has(new Key(9)))   // false

let m = new Map<Key, i64>()
m.set(new Key(1), 10)
m.set(new Key(2), 20)
m.set(new Key(3), 30)
console.log(m.size())        // 3 (distinct keys, same bucket)
console.log(m.get(new Key(2)))   // 20
