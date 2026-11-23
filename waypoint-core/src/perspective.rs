//! Rectilinear camera views from equirectangular Street View panoramas.
use image::{Rgb, RgbImage};

fn sample(pano: &RgbImage, x: f64, y: f64) -> Rgb<u8> {
    let w = pano.width() as i64;
    let h = pano.height() as i64;
    let xx = x.rem_euclid(w as f64);
    let yy = y.clamp(0.0, (h - 1) as f64);
    let x0 = xx.floor() as i64;
    let y0 = yy.floor() as i64;
    let x1 = (x0 + 1).rem_euclid(w);
    let y1 = (y0 + 1).min(h - 1);
    let (dx, dy) = (xx - x0 as f64, yy - y0 as f64);
    let a = pano.get_pixel(x0 as u32, y0 as u32);
    let b = pano.get_pixel(x1 as u32, y0 as u32);
    let c = pano.get_pixel(x0 as u32, y1 as u32);
    let d = pano.get_pixel(x1 as u32, y1 as u32);
    let mix = |k: usize| {
        let top = a[k] as f64 * (1.0 - dx) + b[k] as f64 * dx;
        let bot = c[k] as f64 * (1.0 - dx) + d[k] as f64 * dx;
        (top * (1.0 - dy) + bot * dy).round().clamp(0.0, 255.0) as u8
    };
    Rgb([mix(0), mix(1), mix(2)])
}
/// heading=0 faces the equirectangular horizontal origin.
pub fn render(pano: &RgbImage, heading_deg: f64, fov_deg: f64, width: u32, height: u32) -> RgbImage {
    assert!(pano.width() > 0 && pano.height() > 0 && width > 0 && height > 0);
    let mut out = RgbImage::new(width, height);
    let half_width = (0.5 * fov_deg.to_radians()).tan();
    let half_height = half_width * height as f64 / width as f64;
    let yaw = heading_deg.to_radians();
    let w = pano.width() as f64;
    let h = pano.height() as f64;
    for y in 0..height {
        for x in 0..width {
            let vx = ((x as f64 + 0.5) / width as f64 * 2.0 - 1.0) * half_width;
            let vy = (1.0 - (y as f64 + 0.5) / height as f64 * 2.0) * half_height;
            let longitude = (yaw + vx.atan2(1.0)).rem_euclid(std::f64::consts::TAU);
            let latitude = vy.atan2((vx * vx + 1.0).sqrt());
            let pano_x = longitude / std::f64::consts::TAU * w;
            let pano_y = (0.5 - latitude / std::f64::consts::PI) * h;
            out.put_pixel(x, y, sample(pano, pano_x, pano_y));
        }
    }
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn center_matches_equator() {
        let mut pano = RgbImage::new(360, 180);
        for y in 0..180 { for x in 0..360 { pano.put_pixel(x, y, Rgb([x as u8, y as u8, 0])); } }
        let c = render(&pano, 90.0, 60.0, 3, 3);
        assert!((c.get_pixel(1, 1)[0] as i32 - 90).abs() <= 1);
        assert!((c.get_pixel(1, 1)[1] as i32 - 90).abs() <= 1);
    }
    #[test]
    fn longitude_wrap() {
        let mut pano = RgbImage::new(360, 180);
        for y in 0..180 { for x in 0..360 { pano.put_pixel(x, y, Rgb([x as u8, 0, 0])); } }
        assert_eq!(render(&pano, 359.0, 70.0, 5, 5), render(&pano, -1.0, 70.0, 5, 5));
    }
}