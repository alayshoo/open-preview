//! Procedurally generated window/taskbar icon.

use egui::IconData;

pub fn app_icon() -> IconData {
    const N: usize = 64;
    let mut rgba = vec![0u8; N * N * 4];
    let (a, b) = ([124.0, 108.0, 255.0], [72.0, 196.0, 255.0]);
    for y in 0..N {
        for x in 0..N {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            // Rounded-square coverage via a signed distance field.
            let (half, radius) = (30.0, 15.0);
            let qx = (fx - 32.0).abs() - (half - radius);
            let qy = (fy - 32.0).abs() - (half - radius);
            let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - radius;
            let cover = (0.5 - outside).clamp(0.0, 1.0);
            if cover <= 0.0 {
                continue;
            }
            let t = (fx + fy) / (2.0 * N as f32);
            let mut c = [0.0; 3];
            for i in 0..3 {
                c[i] = a[i] + (b[i] - a[i]) * t;
            }
            // Play triangle: (24,18) (46,32) (24,46).
            let tri = edge(fx, fy, 24.0, 18.0, 46.0, 32.0).min(edge(fx, fy, 46.0, 32.0, 24.0, 46.0)).min(edge(fx, fy, 24.0, 46.0, 24.0, 18.0));
            let white = (tri + 0.5).clamp(0.0, 1.0);
            let o = (y * N + x) * 4;
            for i in 0..3 {
                rgba[o + i] = (c[i] + (255.0 - c[i]) * white) as u8;
            }
            rgba[o + 3] = (cover * 255.0) as u8;
        }
    }
    IconData { rgba, width: N as u32, height: N as u32 }
}

/// Signed distance to the left of the directed edge (positive = inside for clockwise winding).
fn edge(px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let (dx, dy) = (bx - ax, by - ay);
    -(((px - ax) * dy - (py - ay) * dx) / (dx * dx + dy * dy).sqrt())
}
