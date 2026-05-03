// Sibling module exporting an enum.

enum Color { red, green, blue }

fn brightness(c: Color): i64 {
    match c {
        red { 1 }
        green { 2 }
        blue { 3 }
    }
}
