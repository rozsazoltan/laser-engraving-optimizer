use image::{GrayImage, RgbImage};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

pub(crate) const MAX_DIMENSION: u32 = 16_384;
pub(crate) const MAX_PIXELS: u64 = 100_000_000;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct GridCalibrationSpecification {
    pub block_size: u32,
    pub rows: u32,
    pub columns: u32,
    pub exponent: f64,
}

impl Default for GridCalibrationSpecification {
    fn default() -> Self {
        Self {
            block_size: 400,
            rows: 6,
            columns: 6,
            exponent: 1.8,
        }
    }
}

impl GridCalibrationSpecification {
    pub fn validate(&self) -> Result<(), String> {
        if self.block_size < 32
            || self.rows < 2
            || self.columns < 2
            || !self.exponent.is_finite()
            || self.exponent <= 0.0
        {
            return Err(
                "block_size >= 32, rows/columns >= 2 and curve exponent > 0 required".into(),
            );
        }
        let (w, h) = self.dimensions()?;
        validate_dimensions(w, h)
    }
    pub fn dimensions(&self) -> Result<(u32, u32), String> {
        Ok((
            self.block_size
                .checked_mul(self.columns)
                .ok_or("Gauge dimensions overflow")?,
            self.block_size
                .checked_mul(self.rows)
                .ok_or("Gauge dimensions overflow")?,
        ))
    }
    pub fn generate(&self) -> Result<GrayImage, String> {
        self.validate()?;
        let (w, h) = self.dimensions()?;
        let mut image = GrayImage::from_pixel(w, h, image::Luma([255]));
        let spacing = self.block_size as f64 * 0.1;
        let vertical_safety = (self.block_size as f64 - spacing) * 0.3;
        let count = (self.rows * self.columns - 1) as f64;
        let mut n = 0u32;
        for row in 0..self.rows {
            for col in 0..self.columns {
                if n == 0 {
                    n += 1;
                    continue;
                }
                let factor = (n as f64 / count).powf(self.exponent);
                let x0 = ((col * self.block_size) as f64 + spacing).ceil() as u32;
                let y0 = ((row * self.block_size) as f64 + spacing + vertical_safety * factor)
                    .ceil() as u32;
                let x1 = (((col + 1) * self.block_size) as f64 - spacing).ceil() as u32;
                let y1 = (((row + 1) * self.block_size) as f64 - spacing).ceil() as u32;
                let level = (255.0 - factor * 255.0) as u8;
                for y in y0..=y1.min(h - 1) {
                    for x in x0..=x1.min(w - 1) {
                        image.put_pixel(x, y, image::Luma([level]));
                    }
                }
                n += 1;
            }
        }
        Ok(image)
    }
    pub(crate) fn colored_areas(&self) -> Result<Vec<Area>, String> {
        self.validate()?;
        let mut areas = Vec::new();
        let spacing = self.block_size as f64 * 0.1;
        let safety = (self.block_size as f64 - spacing) * 0.3;
        let count = (self.rows * self.columns - 1) as f64;
        let mut n = 0u32;
        for row in 0..self.rows {
            for col in 0..self.columns {
                if n == 0 {
                    n += 1;
                    continue;
                }
                let factor = (n as f64 / count).powf(self.exponent);
                let x0 = ((col * self.block_size) as f64 + spacing).ceil() as u32;
                let y0 = ((row * self.block_size) as f64 + spacing + safety * factor).ceil() as u32;
                let x1 = (((col + 1) * self.block_size) as f64 - spacing).ceil() as u32;
                let y1 = (((row + 1) * self.block_size) as f64 - spacing).ceil() as u32;
                areas.push(Area {
                    x0,
                    y0,
                    x1,
                    y1,
                    level: (255.0 - factor * 255.0) as u8,
                });
                n += 1;
            }
        }
        Ok(areas)
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Area {
    pub x0: u32,
    pub y0: u32,
    pub x1: u32,
    pub y1: u32,
    pub level: u8,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CalibrationSample {
    pub input_lightness: u8,
    pub background_lightness: f64,
    pub engraved_lightness: f64,
    pub measured_darkness: f64,
    pub fitted_darkness: f64,
}

#[derive(Clone, Debug)]
pub struct Calibration {
    pub samples: Vec<CalibrationSample>,
    pub median_whiteness: f64,
    pub maximum_darkness: f64,
    pub forward_darkness: Vec<f64>,
    pub(crate) inverse_darkness: Vec<f64>,
}

fn validate_dimensions(w: u32, h: u32) -> Result<(), String> {
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

fn median(mut values: Vec<f64>) -> Result<f64, String> {
    if values.is_empty() {
        return Err("Calibration patch too small to sample".into());
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    let m = values.len() / 2;
    Ok(if values.len() % 2 == 0 {
        (values[m - 1] + values[m]) / 2.0
    } else {
        values[m]
    })
}
fn pava(values: &[f64], weights: &[f64]) -> Vec<f64> {
    let mut b: Vec<(usize, usize, f64, f64)> = Vec::new();
    for (i, (&v, &w)) in values.iter().zip(weights).enumerate() {
        b.push((i, i + 1, v * w, w));
        while b.len() > 1 {
            let n = b.len();
            if b[n - 2].2 / b[n - 2].3 <= b[n - 1].2 / b[n - 1].3 {
                break;
            }
            let r = b.pop().unwrap();
            let l = b.last_mut().unwrap();
            l.1 = r.1;
            l.2 += r.2;
            l.3 += r.3;
        }
    }
    let mut out = vec![0.0; values.len()];
    for (s, e, t, w) in b {
        out[s..e].fill(t / w);
    }
    out
}
fn interp(x: f64, xs: &[f64], ys: &[f64]) -> f64 {
    if x <= xs[0] {
        return ys[0];
    }
    if x >= xs[xs.len() - 1] {
        return ys[ys.len() - 1];
    }
    let hi = xs.partition_point(|v| *v < x);
    let lo = hi - 1;
    ys[lo] + (x - xs[lo]) / (xs[hi] - xs[lo]) * (ys[hi] - ys[lo])
}

impl Calibration {
    pub fn from_scan(spec: &GridCalibrationSpecification, scan: &RgbImage) -> Result<Self, String> {
        Self::from_scan_with_noise(spec, scan, 2.0)
    }
    pub fn from_scan_with_noise(
        spec: &GridCalibrationSpecification,
        scan: &RgbImage,
        noise_floor: f64,
    ) -> Result<Self, String> {
        spec.validate()?;
        let (w, h) = spec.dimensions()?;
        if scan.dimensions() != (w, h) {
            return Err(format!("Calibration scan dimensions {}x{} != {w}x{h}; crop, align and resize scan to calibration gauge dimensions",scan.width(),scan.height()));
        }
        if !noise_floor.is_finite() || noise_floor < 0.0 {
            return Err("noise_floor must be finite and nonnegative".into());
        }
        let gray = GrayImage::from_fn(w, h, |x, y| {
            let p = scan.get_pixel(x, y);
            image::Luma([
                ((1868u32 * p[2] as u32 + 9617 * p[1] as u32 + 4899 * p[0] as u32 + 8192) >> 14)
                    as u8,
            ])
        });
        let mut samples = Vec::new();
        for a in spec.colored_areas()? {
            let width = a.x1 - a.x0;
            let height = a.y1 - a.y0;
            let inset = ((width as f64 * 0.015).round_ties_even() as u32).max(1);
            let end = ((width as f64 * 0.075).round_ties_even() as u32).max(inset + 1);
            let py0 = a.y0 + (height as f64 * 0.20).round_ties_even() as u32;
            let py1 = a.y1 - (height as f64 * 0.20).round_ties_even() as u32;
            let px0 = a.x0 + (width as f64 * 0.15).round_ties_even() as u32;
            let px1 = a.x1 - (width as f64 * 0.15).round_ties_even() as u32;
            let region = |x0: u32, y0: u32, x1: u32, y1: u32| -> Result<f64, String> {
                let mut v = Vec::new();
                for y in y0..y1 {
                    for x in x0..x1 {
                        v.push(gray.get_pixel(x, y)[0] as f64)
                    }
                }
                median(v)
            };
            let engr = region(px0, py0, px1, py1)?;
            let left = region(a.x0 - end, py0, a.x0 - inset, py1)?;
            let right = region(a.x1 + inset, py0, a.x1 + end, py1)?;
            let bg = (left + right) / 2.0;
            samples.push(CalibrationSample {
                input_lightness: a.level,
                background_lightness: bg,
                engraved_lightness: engr,
                measured_darkness: (bg - engr).max(0.0),
                fitted_darkness: 0.0,
            });
        }
        if samples.is_empty() {
            return Err("Empty calibration gauge".into());
        }
        let mut backgrounds: Vec<f64> = samples.iter().map(|s| s.background_lightness).collect();
        backgrounds.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let median_whiteness = median(backgrounds)?;
        let mut raw = vec![(0.0, 0.0, 20.0)];
        raw.extend(
            samples
                .iter()
                .map(|s| ((255 - s.input_lightness) as f64, s.measured_darkness, 1.0)),
        );
        raw.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        let mut xs: Vec<f64> = Vec::new();
        let mut ds: Vec<f64> = Vec::new();
        let mut ws: Vec<f64> = Vec::new();
        for (x, d, w) in raw {
            if xs.last() == Some(&x) {
                let i = xs.len() - 1;
                ds[i] = (ds[i] * ws[i] + d * w) / (ws[i] + w);
                ws[i] += w;
            } else {
                xs.push(x);
                ds.push(d);
                ws.push(w);
            }
        }
        let mut fit = pava(&ds, &ws);
        for y in &mut fit {
            *y = (*y - noise_floor).max(0.0)
        }
        fit[0] = 0.0;
        if *fit.last().unwrap_or(&0.0) <= 1.0 {
            return Err("No measurable engraving contrast; check scan, image orientation and laser settings".into());
        }
        let maximum_darkness = *fit.last().unwrap();
        let forward_darkness = (0..256)
            .map(|x| interp(x as f64, &xs, &fit))
            .collect::<Vec<_>>();
        let mut ux = Vec::new();
        let mut uy = Vec::new();
        for (i, &v) in fit.iter().enumerate() {
            if ux.last() != Some(&v) {
                ux.push(v);
                uy.push(xs[i]);
            }
        }
        let inverse_darkness = (0..4096)
            .map(|i| interp(maximum_darkness * i as f64 / 4095.0, &ux, &uy))
            .collect();
        for s in &mut samples {
            s.fitted_darkness = forward_darkness[(255 - s.input_lightness) as usize];
        }
        Ok(Self {
            samples,
            median_whiteness,
            maximum_darkness,
            forward_darkness,
            inverse_darkness,
        })
    }
    pub fn invert_desired_darkness(&self, desired: f32) -> u8 {
        let d = (desired as f64).clamp(0.0, self.maximum_darkness);
        let index = d / self.maximum_darkness * 4095.0;
        let lo = index.floor() as usize;
        let hi = index.ceil() as usize;
        (255.0
            - (self.inverse_darkness[lo]
                + (index - lo as f64) * (self.inverse_darkness[hi] - self.inverse_darkness[lo])))
            .clamp(0.0, 255.0) as u8
    }
    pub fn simulate(&self, input: &GrayImage) -> GrayImage {
        GrayImage::from_fn(input.width(), input.height(), |x, y| {
            let d = self.forward_darkness[(255 - input.get_pixel(x, y)[0]) as usize];
            image::Luma([(self.median_whiteness - d)
                .round_ties_even()
                .clamp(0.0, 255.0) as u8])
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_gauge_has_expected_size_and_ordered_tones() {
        let spec = GridCalibrationSpecification::default();
        let gauge = spec.generate().unwrap();
        assert_eq!(gauge.dimensions(), (2400, 2400));
        let areas = spec.colored_areas().unwrap();
        assert_eq!(areas.len(), 35);
        assert!(areas.windows(2).all(|pair| pair[0].level >= pair[1].level));
    }

    #[test]
    fn rejects_wrong_scan_size_and_no_contrast() {
        let spec = GridCalibrationSpecification {
            block_size: 32,
            rows: 2,
            columns: 2,
            exponent: 1.8,
        };
        assert!(Calibration::from_scan(&spec, &RgbImage::new(1, 1))
            .unwrap_err()
            .contains("dimensions"));
        let blank = RgbImage::from_pixel(64, 64, image::Rgb([255, 255, 255]));
        assert!(Calibration::from_scan(&spec, &blank)
            .unwrap_err()
            .contains("contrast"));
    }
}
