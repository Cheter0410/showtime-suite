//! The stage look of the window: a banner with a lighting rig over a stage (the colours of the apps
//! as beams through haze, pools of light on the floor, the audience in front) and the soft glow that
//! lights each app tile from above. Both are drawn here once, as pictures.

use egui::{Color32, ColorImage};

/// The lamps on the truss: where they hang, where they point on the floor (0..1 of the width) and
/// their colour. Show Master green, Beamweave pink, Lantern amber, and cool white.
const LAMPS: [(f32, f32, [f32; 3], f32); 7] = [
    (0.36, 0.52, [0.13, 0.77, 0.37], 0.75),
    (0.46, 0.33, [0.91, 0.31, 0.54], 0.85),
    (0.55, 0.70, [1.00, 0.77, 0.36], 0.80),
    (0.64, 0.50, [0.78, 0.84, 1.00], 0.55),
    (0.73, 0.90, [0.13, 0.77, 0.37], 0.70),
    (0.82, 0.62, [0.91, 0.31, 0.54], 0.80),
    (0.92, 0.78, [1.00, 0.77, 0.36], 0.70),
];

const TRUSS: f32 = 0.13;
const FLOOR: f32 = 0.80;

fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn add(c: &mut [f32; 3], col: [f32; 3], k: f32) {
    for (c, col) in c.iter_mut().zip(col) {
        *c += col * k;
    }
}

/// A number 0..1 that looks random but is always the same for `n`.
fn hash(n: u32) -> f32 {
    let mut x = n.wrapping_mul(0x9E37_79B9) ^ 0x85EB_CA6B;
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    (x & 0xFFFF) as f32 / 65535.0
}

/// The stage banner, `w`×`h` pixels. The left part stays dark for the title.
pub fn banner_image(w: usize, h: usize) -> ColorImage {
    let (wf, hf) = (w as f32, h as f32);
    let aspect = wf / hf;
    // The audience: one head every few pixels along the bottom edge.
    let heads: Vec<(f32, f32, f32)> = (0..)
        .map(|i| i as f32 * 0.034 * hf)
        .take_while(|x| *x < wf + 40.0)
        .enumerate()
        .map(|(i, x)| {
            let r = hf * (0.045 + 0.012 * hash(i as u32 * 7 + 1));
            let jx = (hash(i as u32 * 13 + 5) - 0.5) * hf * 0.02;
            let y = hf * (0.955 + 0.03 * hash(i as u32 * 31 + 3));
            (x + jx, y, r)
        })
        .collect();
    let mut px = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        let v = (y as f32 + 0.5) / hf;
        for x in 0..w {
            let u = (x as f32 + 0.5) / wf;
            // Room: almost black, a little lighter towards the floor.
            let mut c = if v < FLOOR {
                let t = v / FLOOR;
                [0.030 + 0.020 * t, 0.033 + 0.020 * t, 0.048 + 0.028 * t]
            } else {
                let t = (v - FLOOR) / (1.0 - FLOOR);
                [0.060 - 0.030 * t, 0.058 - 0.030 * t, 0.075 - 0.038 * t]
            };
            // The title side stays darker.
            let side = 0.25 + 0.75 * smooth(0.18, 0.62, u);
            // Haze: slow waves, so the beams are not perfectly even.
            let haze = 0.82 + 0.18 * ((u * 9.0 + v * 4.0).sin() * (u * 4.0 - v * 7.0 + 1.3).sin());
            for (from, to, col, power) in LAMPS {
                // Beams: from the lamp on the truss to its spot on the floor, wider further down.
                if v > TRUSS && v < FLOOR + 0.06 {
                    let t = ((v - TRUSS) / (FLOOR - TRUSS)).min(1.0);
                    let centre = from + (to - from) * t;
                    let half = (0.006 + 0.075 * t) / aspect * 2.2;
                    let d = (u - centre).abs() / half;
                    if d < 1.0 {
                        let edge = (1.0 - d * d).powi(2);
                        let fade = (0.30 + 0.70 * (1.0 - t)) * if v > FLOOR { 1.0 - (v - FLOOR) / 0.06 } else { 1.0 };
                        let k = 0.46 * edge * fade * haze * power * side;
                        add(&mut c, col, k);
                    }
                }
                // Pools of light on the floor.
                if v > FLOOR - 0.02 {
                    let dx = (u - to) * aspect / 0.55;
                    let dy = (v - (FLOOR + 0.07)) / 0.07;
                    let d2 = dx * dx + dy * dy;
                    if d2 < 1.0 {
                        let k = 0.42 * (1.0 - d2).powi(2) * power * side;
                        add(&mut c, col, k);
                    }
                }
                // The lamp itself: a small bright lens under the truss.
                let dx = (u - from) * aspect;
                let dy = v - (TRUSS + 0.03);
                let d = (dx * dx + dy * dy).sqrt() / 0.028;
                if d < 1.6 {
                    let k = (1.0 - d / 1.6).max(0.0).powi(2) * 1.4 * power.max(0.6);
                    add(&mut c, [col[0] * 0.6 + 0.4, col[1] * 0.6 + 0.4, col[2] * 0.6 + 0.4], k);
                }
            }
            // The truss: a bar across the top with its lattice.
            let tv = (v - TRUSS) * hf;
            if (-3.0..3.0).contains(&tv) && u > 0.30 {
                let lattice = if ((u * wf) as i32 / 6) % 2 == 0 { 0.10 } else { 0.07 };
                c = [lattice, lattice, lattice + 0.015];
            }
            // The audience in front: dark heads and shoulders.
            let (pxx, pyy) = (x as f32, y as f32);
            let crowd = v > 0.97
                || heads.iter().any(|(hx, hy, r)| {
                    let (dx, dy) = (pxx - hx, pyy - hy);
                    dx * dx + dy * dy < r * r
                });
            if crowd {
                c = [0.012, 0.012, 0.018];
            }
            for ch in c {
                px.push((ch.clamp(0.0, 1.0).powf(1.0 / 1.25) * 255.0) as u8);
            }
            px.push(255);
        }
    }
    ColorImage::from_rgba_unmultiplied([w, h], &px)
}

/// A soft white light falling from above the top middle; tinted with an app's colour it lights the
/// top of its tile.
pub fn glow_image(n: usize) -> ColorImage {
    let mut px = Vec::with_capacity(n * n * 4);
    for y in 0..n {
        for x in 0..n {
            let u = (x as f32 + 0.5) / n as f32 - 0.5;
            let v = (y as f32 + 0.5) / n as f32 + 0.15;
            let d = (u * u * 1.4 + v * v).sqrt() / 0.85;
            let a = (1.0 - d).max(0.0).powi(2);
            px.extend_from_slice(&[255, 255, 255, (a * 255.0) as u8]);
        }
    }
    ColorImage::from_rgba_unmultiplied([n, n], &px)
}

/// `c` with `alpha` (0..1), for overlays.
pub fn alpha(c: Color32, alpha: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (alpha.clamp(0.0, 1.0) * 255.0) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_banner_is_dark_on_the_title_side_and_lit_on_the_stage() {
        let img = banner_image(400, 120);
        assert_eq!(img.size, [400, 120]);
        let lum = |x: usize, y: usize| {
            let c = img.pixels[y * 400 + x];
            c.r() as u32 + c.g() as u32 + c.b() as u32
        };
        let left: u32 = (10..60).map(|y| lum(20, y)).sum();
        let right: u32 = (10..90).map(|y| (240..380).map(|x| lum(x, y)).max().unwrap_or(0)).sum();
        assert!(right > left * 3, "beams on the right ({right}) outshine the title side ({left})");
    }

    #[test]
    fn the_glow_fades_to_nothing_at_the_bottom() {
        let img = glow_image(64);
        assert!(img.pixels[2 * 64 + 32].a() > 100);
        assert_eq!(img.pixels[63 * 64 + 32].a(), 0);
    }
}
