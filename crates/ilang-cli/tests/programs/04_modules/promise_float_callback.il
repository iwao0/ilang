// A `Promise<f64>` (or any float-valued promise) hands its value to a
// `.then` callback through a float-register ABI, and a callback that
// returns a float propagates it correctly to the next `.then`. The old
// integer-only call path fed the callback garbage. A single sequential
// chain with one final `console.log` keeps the async output
// deterministic.
//
// expect: 5.5

let _ = Promise.resolve(2.5)
  .then(fn(v: f64): f64 { v * 2.0 })       // 5.0
  .then(fn(v: f64): i64 { v as i64 })       // 5
  .then(fn(n: i64): f64 { (n as f64) + 0.5 }) // 5.5
  .then(fn(v: f64) { console.log(v) })
