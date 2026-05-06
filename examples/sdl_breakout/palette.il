// One row → one neon hue. Five rows from top to bottom.

fn paletteR(row: i32): u8 {
    if row == 0 { return 255 as u8 }
    if row == 1 { return 255 as u8 }
    if row == 2 { return  80 as u8 }
    if row == 3 { return 120 as u8 }
    if row == 4 { return 255 as u8 }
    255 as u8
}

fn paletteG(row: i32): u8 {
    if row == 0 { return  80 as u8 }
    if row == 1 { return 200 as u8 }
    if row == 2 { return 255 as u8 }
    if row == 3 { return 120 as u8 }
    if row == 4 { return 255 as u8 }
    255 as u8
}

fn paletteB(row: i32): u8 {
    if row == 0 { return 200 as u8 }
    if row == 1 { return  80 as u8 }
    if row == 2 { return 220 as u8 }
    if row == 3 { return 255 as u8 }
    if row == 4 { return 120 as u8 }
    255 as u8
}
