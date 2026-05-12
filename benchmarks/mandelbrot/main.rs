const W: i64 = 1024;
const H: i64 = 1024;
const MAX_ITER: i64 = 1000;

fn main() {
    let mut escaped: i64 = 0;
    for py in 0..H {
        let y0 = (py as f64) / (H as f64) * 2.0 - 1.0;
        for px in 0..W {
            let x0 = (px as f64) / (W as f64) * 3.0 - 2.0;
            let (mut x, mut y) = (0.0_f64, 0.0_f64);
            let mut it: i64 = 0;
            while x * x + y * y <= 4.0 && it < MAX_ITER {
                let xt = x * x - y * y + x0;
                y = 2.0 * x * y + y0;
                x = xt;
                it += 1;
            }
            if it < MAX_ITER {
                escaped += 1;
            }
        }
    }
    println!("{escaped}");
}
