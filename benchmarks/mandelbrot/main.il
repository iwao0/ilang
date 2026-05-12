const W: i64 = 1024
const H: i64 = 1024
const MAX_ITER: i64 = 1000

let mut_escaped: i64 = 0
for py in 0..H {
    let y0: f64 = (py as f64) / (H as f64) * 2.0 - 1.0
    for px in 0..W {
        let x0: f64 = (px as f64) / (W as f64) * 3.0 - 2.0
        let mut_x: f64 = 0.0
        let mut_y: f64 = 0.0
        let mut_it: i64 = 0
        while mut_x * mut_x + mut_y * mut_y <= 4.0 && mut_it < MAX_ITER {
            let xt: f64 = mut_x * mut_x - mut_y * mut_y + x0
            mut_y = 2.0 * mut_x * mut_y + y0
            mut_x = xt
            mut_it = mut_it + 1
        }
        if mut_it < MAX_ITER {
            mut_escaped = mut_escaped + 1
        }
    }
}
console.log(mut_escaped)
