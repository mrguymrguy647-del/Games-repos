//! Hex grid maths. The maps are pointy-topped hexes in rows ("odd-r": odd
//! rows sit half a hex to the right), one unit wide, written one letter
//! per hex in the map files.
//!
//! Two kinds of position:
//! - a cell, `(column, row)`, as in the files (row 0 at the top, north);
//! - the map plane, `(x, y)` in hex widths with y pointing south, which the
//!   3D world turns into `x` east and `z` north.

/// The distance between two rows' centres.
pub const ROW: f32 = 0.866_025_4;
/// From a hex's centre to a corner.
pub const RADIUS: f32 = 0.577_350_3;

/// Where a cell's centre is on the map plane.
pub fn center(c: usize, r: usize) -> (f32, f32) {
    (c as f32 + 0.5 * (r & 1) as f32, r as f32 * ROW)
}

/// The six neighbours of a cell that are on a `w` x `h` grid.
pub fn neighbours(c: usize, r: usize, w: usize, h: usize) -> impl Iterator<Item = (usize, usize)> {
    const EVEN: [(i32, i32); 6] = [(-1, 0), (1, 0), (-1, -1), (0, -1), (-1, 1), (0, 1)];
    const ODD: [(i32, i32); 6] = [(-1, 0), (1, 0), (0, -1), (1, -1), (0, 1), (1, 1)];
    let offsets = if r & 1 == 1 { ODD } else { EVEN };
    offsets.into_iter().filter_map(move |(dc, dr)| {
        let (cc, rr) = (c as i32 + dc, r as i32 + dr);
        (cc >= 0 && rr >= 0 && (cc as usize) < w && (rr as usize) < h).then_some((cc as usize, rr as usize))
    })
}

/// Corner `k` (0 to 5) of a hex centred at `(x, y)` on the map plane:
/// corner 0 is down-right, going clockwise (y points down).
pub fn corner(x: f32, y: f32, k: usize) -> (f32, f32) {
    let a = (30.0 + 60.0 * k as f32).to_radians();
    (x + RADIUS * a.cos(), y + RADIUS * a.sin())
}

/// The two corners of the edge a hex at `from` shares with its neighbour
/// at `to` (both map-plane centres), in clockwise order.
pub fn shared_edge(from: (f32, f32), to: (f32, f32)) -> (usize, usize) {
    let a = (to.1 - from.1).atan2(to.0 - from.0).to_degrees();
    let d = ((a / 60.0).round() as i32).rem_euclid(6) as usize;
    ((d + 5) % 6, d)
}

/// The cell whose hex contains the map-plane point `(x, y)`, if it's on a
/// `w` x `h` grid.
pub fn cell_at(x: f32, y: f32, w: usize, h: usize) -> Option<(usize, usize)> {
    let r0 = (y / ROW).round() as i32;
    let mut best: Option<((usize, usize), f32)> = None;
    for r in (r0 - 1)..=(r0 + 1) {
        if r < 0 || r as usize >= h {
            continue;
        }
        let shift = 0.5 * (r & 1) as f32;
        let c0 = (x - shift).round() as i32;
        for c in (c0 - 1)..=(c0 + 1) {
            if c < 0 || c as usize >= w {
                continue;
            }
            let (cx, cy) = center(c as usize, r as usize);
            let d = (cx - x).powi(2) + (cy - y).powi(2);
            if best.is_none_or(|(_, bd)| d < bd) {
                best = Some(((c as usize, r as usize), d));
            }
        }
    }
    best.filter(|(_, d)| *d <= RADIUS * RADIUS * 1.01).map(|(cell, _)| cell)
}

/// Steps between two cells.
pub fn distance(a: (usize, usize), b: (usize, usize)) -> usize {
    let cube = |(c, r): (usize, usize)| {
        let q = c as i32 - (r as i32 - (r as i32 & 1)) / 2;
        let rr = r as i32;
        (q, rr, -q - rr)
    };
    let (a, b) = (cube(a), cube(b));
    (((a.0 - b.0).abs() + (a.1 - b.1).abs() + (a.2 - b.2).abs()) / 2) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neighbours_are_one_step_away_and_share_an_edge() {
        for (c, r) in [(5, 4), (5, 5), (0, 0), (9, 7)] {
            let ns: Vec<_> = neighbours(c, r, 10, 8).collect();
            for &n in &ns {
                assert_eq!(distance((c, r), n), 1);
                let (a, b) = center(c, r);
                let (x, y) = center(n.0, n.1);
                assert!(((a - x).powi(2) + (b - y).powi(2) - 1.0).abs() < 1e-3);
                // The shared edge's corners are the same points seen from both sides.
                let (k0, k1) = shared_edge((a, b), (x, y));
                let (j0, j1) = shared_edge((x, y), (a, b));
                let p = |p: (f32, f32), q: (f32, f32)| (p.0 - q.0).abs() + (p.1 - q.1).abs() < 1e-3;
                assert!(p(corner(a, b, k0), corner(x, y, j1)) && p(corner(a, b, k1), corner(x, y, j0)));
            }
        }
        assert_eq!(neighbours(5, 4, 10, 8).count(), 6);
        assert_eq!(neighbours(0, 0, 10, 8).count(), 2);
    }

    #[test]
    fn a_point_finds_its_own_hex() {
        for r in 0..8 {
            for c in 0..10 {
                let (x, y) = center(c, r);
                assert_eq!(cell_at(x + 0.2, y - 0.15, 10, 8), Some((c, r)));
            }
        }
        assert_eq!(cell_at(-3.0, 2.0, 10, 8), None);
    }
}
