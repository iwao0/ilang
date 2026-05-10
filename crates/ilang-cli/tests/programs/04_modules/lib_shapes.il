// Sibling module exporting an enum.

pub enum Color { red, green, blue }

pub fn brightness(c: Color): i64 {
    match c {
        red { 1 }
        green { 2 }
        blue { 3 }
    }
}
