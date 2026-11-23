use crate::calibration::{Calibration, MAX_DIMENSION, MAX_PIXELS};
use image::{GrayImage, ImageReader, RgbImage};
use rand::{rngs::StdRng, Rng, SeedableRng};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ColorMethod {
    Decolorize,
    Luma,
}
impl Default for ColorMethod {
    fn default() -> Self {
        Self::Decolorize
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ProcessingSettings {
    pub clahe_clip: f64,
    pub color_method: ColorMethod,
    pub gamma: f64,
}
impl Default for ProcessingSettings {
    fn default() -> Self {
        Self {
            clahe_clip: 4.0,
            color_method: ColorMethod::Decolorize,
            gamma: 1.0,
        }
    }
}
impl ProcessingSettings {
    pub fn validate(&self) -> Result<(), String> {
        if !self.clahe_clip.is_finite() || self.clahe_clip < 0.0 {
            return Err("clahe_clip must be finite and >= 0".into());
        }
        if !self.gamma.is_finite() || self.gamma <= 0.0 {
            return Err("gamma must be finite and > 0".into());
        }
        Ok(())
    }
}
pub struct ProcessedImages {
    pub grayscale: GrayImage,
    pub engraving: GrayImage,
    pub simulation: GrayImage,
}

pub fn load_rgb(path: impl AsRef<Path>) -> Result<RgbImage, String> {
    let mut reader = ImageReader::open(path)
        .map_err(|e| format!("Input photo could not be loaded: {e}"))?
        .with_guessed_format()
        .map_err(|e| format!("Unsupported image format: {e}"))?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_DIMENSION);
    limits.max_image_height = Some(MAX_DIMENSION);
    limits.max_alloc = Some(MAX_PIXELS * 4);
    reader.limits(limits);
    let decoded = reader
        .decode()
        .map_err(|e| format!("Input photo could not be decoded: {e}"))?;
    let rgb = decoded.to_rgb8();
    validate_size(rgb.width(), rgb.height())?;
    Ok(rgb)
}
fn validate_size(w: u32, h: u32) -> Result<(), String> {
    if w == 0
        || h == 0
        || w > MAX_DIMENSION
        || h > MAX_DIMENSION
        || w as u64 * h as u64 > MAX_PIXELS
    {
        Err(format!("Image dimensions {w}x{h} exceed safe limits"))
    } else {
        Ok(())
    }
}
pub(crate) fn luma(rgb: &RgbImage) -> GrayImage {
    GrayImage::from_fn(rgb.width(), rgb.height(), |x, y| {
        let p = rgb.get_pixel(x, y);
        image::Luma([
            ((1868u32 * p[2] as u32 + 9617 * p[1] as u32 + 4899 * p[0] as u32 + 8192) >> 14) as u8,
        ])
    })
}

// Based on Decolorize by Mark Grundland and Neil A. Dodgson (2007), adapted
// from decolorize.py / damondpham/decolorize-RGB2grayscale. Sampling is seeded.
fn decolorize(rgb: &RgbImage) -> GrayImage {
    let (w, h) = rgb.dimensions();
    let n = (w * h) as usize;
    let mut rng = StdRng::seed_from_u64(0xD3C010A);
    let indices = (0..n)
        .map(|i| {
            let x = (i as u32 % w) as f64;
            let y = (i as u32 / w) as f64;
            let scale = (2.0 * (w.min(h) as f64)).sqrt() * (2.0 / std::f64::consts::PI).sqrt();
            let sx = reflect((x + normal(&mut rng) * scale).round_ties_even() as i64, w);
            let sy = reflect((y + normal(&mut rng) * scale).round_ties_even() as i64, h);
            (sy * w + sx) as usize
        })
        .collect::<Vec<_>>();
    decolorize_with_indices(rgb, &indices)
}
pub(crate) fn decolorize_with_indices(rgb: &RgbImage, indices: &[usize]) -> GrayImage {
    let (w, h) = rgb.dimensions();
    let n = (w * h) as usize;
    if n == 0 {
        return GrayImage::new(w, h);
    }
    let mut yv = Vec::with_capacity(n);
    let mut pv = Vec::with_capacity(n);
    let mut qv = Vec::with_capacity(n);
    for p in rgb.pixels() {
        let (r, g, b) = (p[2] as f64 / 255., p[1] as f64 / 255., p[0] as f64 / 255.);
        yv.push(0.2989360212937754 * r + 0.5870430744511213 * g + 0.1140209042551033 * b);
        pv.push(0.5 * r + 0.5 * g - b);
        qv.push(r - g);
    }
    assert_eq!(indices.len(), n);
    let mut ax = 0.0;
    let mut ay = 0.0;
    let mut projections = vec![0.0; n];
    let mut chroma = vec![0.0; n];
    for i in 0..n {
        let j = indices[i];
        let dy = yv[i] - yv[j];
        let dp = pv[i] - pv[j];
        let dq = qv[i] - qv[j];
        let a = rgb.as_raw();
        let ci = i * 3;
        let cj = j * 3;
        let cd = (1.0
            + (0..3)
                .map(|k| ((a[ci + k] as f64 - a[cj + k] as f64) / 255.0).powi(2))
                .sum::<f64>())
        .sqrt()
            + f64::EPSILON;
        let weight = if cd < 100.0 * f64::EPSILON {
            0.0
        } else {
            1.0 - (dy.abs() / 0.6685679342408883) / cd
        };
        let sign = dy.signum() * weight;
        ax += dp * sign;
        ay += dq * sign;
        chroma[i] = (pv[i] * pv[i] + qv[i] * qv[i]).sqrt();
    }
    for i in 0..n {
        projections[i] = pv[i] * ax + qv[i] * ay;
    }
    let mut proj = projections.iter().map(|v| v.abs()).collect::<Vec<_>>();
    proj.sort_by(f64::total_cmp);
    let quantile = |sorted: &[f64], q: f64| {
        let i = q * (sorted.len() - 1) as f64;
        let a = i.floor() as usize;
        let b = i.ceil() as usize;
        sorted[a] + (i - a as f64) * (sorted[b] - sorted[a])
    };
    let tol = 100.0 * f64::EPSILON;
    let denom = quantile(&proj, 0.999) + tol;
    for p in &mut projections {
        *p /= denom;
    }
    let mut g = Vec::with_capacity(n);
    for i in 0..n {
        g.push(yv[i] + 0.5 * projections[i]);
    }
    let mut sorted = g.clone();
    sorted.sort_by(f64::total_cmp);
    let low = quantile(&sorted, 0.001);
    let high = quantile(&sorted, 0.999);
    let mut lum = yv.clone();
    lum.sort_by(f64::total_cmp);
    let target_low = 0.5 * quantile(&lum, 0.001);
    let target_high = 0.5 + 0.5 * quantile(&lum, 0.999);
    for i in 0..n {
        let normalized = (g[i] - low) / (high - low + f64::EPSILON);
        g[i] = ((target_low + normalized * (target_high - target_low + tol)).clamp(
            yv[i] - 0.5 / 1.118033988749895 * chroma[i],
            yv[i] + 0.5 / 1.118033988749895 * chroma[i],
        ))
        .clamp(0.0, 1.0);
    }
    let low = g.iter().copied().fold(f64::INFINITY, f64::min);
    let high = g.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    GrayImage::from_fn(w, h, |x, y| {
        let normalized = if high > low {
            (g[(y * w + x) as usize] - low) / (high - low)
        } else {
            0.0
        };
        image::Luma([(normalized * 256.0).floor().clamp(0.0, 255.0) as u8])
    })
}
fn normal(r: &mut StdRng) -> f64 {
    let u1 = r.gen::<f64>().max(f64::MIN_POSITIVE);
    let u2 = r.gen::<f64>();
    (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()
}
fn reflect(v: i64, n: u32) -> u32 {
    let n = n as i64;
    (if v < 0 {
        v.unsigned_abs() as i64 % n
    } else if v >= n {
        n - (v % n) - 1
    } else {
        v
    }) as u32
}

pub(crate) fn clahe(input: &GrayImage, clip: f64) -> GrayImage {
    let (w, h) = input.dimensions();
    if clip == 0.0 || w == 0 || h == 0 {
        return input.clone();
    }
    let tiles = 16u32;
    let pad_x = if w % tiles == 0 { 0 } else { tiles - w % tiles };
    let pad_y = if h % tiles == 0 { 0 } else { tiles - h % tiles };
    let pw = w + pad_x;
    let ph = h + pad_y;
    let padded = GrayImage::from_fn(pw, ph, |x, y| {
        let sx = reflect101(x as i64, w);
        let sy = reflect101(y as i64, h);
        *input.get_pixel(sx, sy)
    });
    let tile_w = pw / tiles;
    let tile_h = ph / tiles;
    let mut output = GrayImage::new(w, h);
    let mut maps = vec![[0u8; 256]; (tiles * tiles) as usize];
    for ty in 0..tiles {
        for tx in 0..tiles {
            let x0 = tx * tile_w;
            let x1 = x0 + tile_w;
            let y0 = ty * tile_h;
            let y1 = y0 + tile_h;
            let area = tile_w * tile_h;
            let limit = ((clip * area as f64 / 256.0) as u32).max(1);
            let mut hist = [0u32; 256];
            for y in y0..y1 {
                for x in x0..x1 {
                    hist[padded.get_pixel(x, y)[0] as usize] += 1
                }
            }
            let mut excess = 0;
            for v in &mut hist {
                if *v > limit {
                    excess += *v - limit;
                    *v = limit
                }
            }
            let batch = excess / 256;
            let mut residual = excess - batch * 256;
            for v in &mut hist {
                *v += batch
            }
            if residual > 0 {
                let residual_step = (256 / residual as usize).max(1);
                let mut at = 0usize;
                while residual > 0 && at < 256 {
                    hist[at] += 1;
                    residual -= 1;
                    at += residual_step;
                }
            }
            let scale = 255.0 / area as f64;
            let mut sum = 0u32;
            for i in 0..256 {
                sum += hist[i];
                maps[(ty * tiles + tx) as usize][i] =
                    (sum as f64 * scale).floor().clamp(0.0, 255.0) as u8;
            }
        }
    }
    for y in 0..h {
        for x in 0..w {
            let fx = x as f64 / tile_w as f64 - 0.5;
            let fy = y as f64 / tile_h as f64 - 0.5;
            let ax = fx.floor().clamp(0.0, 15.0) as u32;
            let ay = fy.floor().clamp(0.0, 15.0) as u32;
            let bx = (ax + 1).min(15);
            let by = (ay + 1).min(15);
            let dx = (fx - ax as f64).clamp(0.0, 1.0);
            let dy = (fy - ay as f64).clamp(0.0, 1.0);
            let v = input.get_pixel(x, y)[0] as usize;
            let a = maps[(ay * tiles + ax) as usize][v] as f64;
            let b = maps[(ay * tiles + bx) as usize][v] as f64;
            let c = maps[(by * tiles + ax) as usize][v] as f64;
            let d = maps[(by * tiles + bx) as usize][v] as f64;
            let top = a + (b - a) * dx;
            let bottom = c + (d - c) * dx;
            output.put_pixel(
                x,
                y,
                image::Luma([(top + (bottom - top) * dy).round().clamp(0.0, 255.0) as u8]),
            );
        }
    }
    output
}
fn reflect101(pos: i64, size: u32) -> u32 {
    if size <= 1 {
        return 0;
    }
    let mut p = pos;
    let n = size as i64;
    while p < 0 || p >= n {
        if p < 0 {
            p = -p
        } else {
            p = 2 * n - p - 2
        }
    }
    p as u32
}
pub fn process_photo(
    rgb: &RgbImage,
    calibration: &Calibration,
    settings: &ProcessingSettings,
) -> Result<ProcessedImages, String> {
    settings.validate()?;
    validate_size(rgb.width(), rgb.height())?;
    let gray = match settings.color_method {
        ColorMethod::Luma => luma(rgb),
        ColorMethod::Decolorize => decolorize(rgb),
    };
    let gray = clahe(&gray, settings.clahe_clip);
    let engraving = GrayImage::from_fn(gray.width(), gray.height(), |x, y| {
        let tone = gray.get_pixel(x, y)[0] as f32 / 255.0;
        let d = (1.0 - tone).powf(settings.gamma as f32) * calibration.maximum_darkness as f32;
        image::Luma([calibration.invert_desired_darkness(d)])
    });
    let simulation = calibration.simulate(&engraving);
    Ok(ProcessedImages {
        grayscale: gray,
        engraving,
        simulation,
    })
}
pub fn process_photo_file(
    path: impl AsRef<Path>,
    calibration: &Calibration,
    settings: &ProcessingSettings,
) -> Result<ProcessedImages, String> {
    let rgb = load_rgb(path)?;
    process_photo(&rgb, calibration, settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decolorize_is_deterministic_and_gray_luma_preserves_gray() {
        let rgb = RgbImage::from_fn(24, 18, |x, y| {
            image::Rgb([x as u8 * 7, y as u8 * 9, (x + y) as u8 * 4])
        });
        assert_eq!(decolorize(&rgb), decolorize(&rgb));
        let gray = RgbImage::from_pixel(3, 2, image::Rgb([127, 127, 127]));
        assert_eq!(luma(&gray), GrayImage::from_pixel(3, 2, image::Luma([127])));
    }

    #[test]
    fn clahe_handles_small_and_odd_dimensions() {
        let image = GrayImage::from_fn(7, 5, |x, y| image::Luma([(x * 31 + y * 17) as u8]));
        let output = clahe(&image, 4.0);
        assert_eq!(output.dimensions(), image.dimensions());
        assert_eq!(clahe(&image, 0.0), image);
    }
}
