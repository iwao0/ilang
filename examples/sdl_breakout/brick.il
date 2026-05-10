// One coloured brick on the playfield. The death animation is held
// here too so the renderer can fade it out without separate state.
// `BrickGrid` owns the rectangular array, palette assignment, and
// the alive-count for end-of-stage detection.

use palette

pub class Brick {
    pub x: i32
    pub y: i32
    pub r: u8
    pub g: u8
    pub b: u8
    pub alive: bool
    // Death animation timer. 0 = either alive or fully gone. > 0
    // means we're inside the puff-and-fade animation: 1.0 starts,
    // counts down to 0.
    pub dying: f64
    pub init(x: i32, y: i32, r: u8, g: u8, b: u8) {
        this.x = x
        this.y = y
        this.r = r
        this.g = g
        this.b = b
        this.alive = true
        this.dying = 0.0
    }
}

pub class BrickGrid {
    pub bricks: Brick[]
    pub cols: i32
    pub rows: i32
    // Number of still-alive bricks. The game-clear check tests
    // this. `kill()` decrements it; `reset()` resets to full.
    pub aliveCount: i32

    /// Build the rectangular grid centred horizontally inside
    /// `winW`, with `gap` pixels between cells. Each row is tinted
    /// by `palette.paletteR/G/B(row)`.
    pub init(cols: i32, rows: i32, brickW: i32, brickH: i32, gap: i32,
         top: i32, winW: i32) {
        let arr: Brick[] = []
        let row: i32 = 0
        while row < rows {
            let col: i32 = 0
            while col < cols {
                let x = col * (brickW + gap) +
                        (winW - cols * (brickW + gap) + gap) / 2
                let y = top + row * (brickH + gap)
                arr.push(new Brick(
                    x, y,
                    palette.paletteR(row),
                    palette.paletteG(row),
                    palette.paletteB(row)
                ))
                col = col + 1
            }
            row = row + 1
        }
        this.bricks = arr
        this.cols = cols
        this.rows = rows
        this.aliveCount = cols * rows
    }

    /// Reset every brick to alive (used when the player restarts
    /// after game-over or clear).
    pub reset() {
        let i: i64 = 0
        let n: i64 = this.bricks.length
        while i < n {
            let b = this.bricks[i]
            b.alive = true
            b.dying = 0.0
            i = i + 1
        }
        this.aliveCount = this.cols * this.rows
    }
}
