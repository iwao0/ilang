// 範囲の端は整数型のみ。
// expect-error: expected
for i in 1.0..5.0 {
    let _ = i
}
