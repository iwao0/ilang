// Map / Set iterate in INSERTION ORDER (JS semantics, user
// decision): entries()/keys()/values()/forEach and printing all
// follow first-insert position; overwriting a key keeps its slot,
// delete + re-insert moves it to the end. The backing HashMaps
// iterate in a random per-process order (SipHash seeding), which
// used to leak into programs as run-to-run nondeterminism.
//
// expect: b=20
// expect: c=3
// expect: d=4
// expect: a=10
// expect: {b: 20, c: 3, d: 4, a: 10}
// expect: keys=b,c,d,a
// expect: apple
// expect: cherry
// expect: banana
// expect: Set { apple, cherry, banana }
// expect: union=Set { apple, cherry, banana, durian }
// expect: inter=Set { cherry }
// expect: diff=Set { apple, banana }
// expect: obj=10,30
// expect: float=1.5,0.5

let m: Map<string, i64> = {"a": 1, "b": 2, "c": 3}
m.set("d", 4)
m.set("b", 20)     // overwrite keeps position
m.delete("a")
m.set("a", 10)     // re-insert goes to the end
for e in m.entries() {
    let (k, v) = e
    console.log(k + "=" + v.toString())
}
console.log(m)
let ks: string[] = m.keys()
console.log("keys=" + ks.join(","))

let s = new Set<string>()
s.add("banana")
s.add("apple")
s.add("cherry")
s.add("apple")
s.delete("banana")
s.add("banana")
for v in s.values() { console.log(v) }
console.log(s)
let t = new Set<string>()
t.add("cherry")
t.add("durian")
console.log("union=" + `${s.union(t)}`)
console.log("inter=" + `${s.intersection(t)}`)
console.log("diff=" + `${s.difference(t)}`)

// object elements keep insertion order too
@derive(Eq, Hash)
class Key {
    id: i64
    init(v: i64) { this.id = v }
}
let os = new Set<Key>()
os.add(new Key(10))
os.add(new Key(30))
os.add(new Key(10))
let ids: string[] = []
for k in os.values() { ids.push(k.id.toString()) }
console.log("obj=" + ids.join(","))

// float elements ride the Int store with bit-pattern identity
let fs = new Set<f64>()
fs.add(1.5)
fs.add(0.5)
fs.add(1.5)
let fvals: string[] = []
fs.forEach(fn(v: f64) { fvals.push(v.toString()) })
console.log("float=" + fvals.join(","))
