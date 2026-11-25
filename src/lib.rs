//! Bounded, pure-Rust image processing core for laser engraving.
pub mod calibration;
pub mod host;
pub mod presets;
pub mod processing;

pub use calibration::{Calibration, CalibrationSample, GridCalibrationSpecification};
pub use processing::{process_photo, ColorMethod, ProcessedImages, ProcessingSettings};

#[cfg(test)]
mod reference_tests {
    use crate::{
        calibration::{Calibration, GridCalibrationSpecification},
        processing::{
            clahe, decolorize_with_indices, luma, process_photo, ColorMethod, ProcessingSettings,
        },
    };
    use image::{GrayImage, RgbImage};
    use serde_json::Value;

    fn fixture() -> Value {
        serde_json::from_str(include_str!("fixtures/python_reference.json")).unwrap()
    }
    fn gray(value: &Value) -> GrayImage {
        let rows = value.as_array().unwrap();
        let h = rows.len() as u32;
        let w = rows[0].as_array().unwrap().len() as u32;
        GrayImage::from_fn(w, h, |x, y| {
            let v = &rows[y as usize][x as usize];
            image::Luma([v
                .as_u64()
                .map(|n| n as u8)
                .unwrap_or_else(|| v.as_f64().unwrap().round() as u8)])
        })
    }
    fn gray_row(value: &Value) -> GrayImage {
        let rows = value.as_array().unwrap();
        let row = if rows[0].is_array() {
            rows[0].as_array().unwrap()
        } else {
            rows
        };
        GrayImage::from_fn(row.len() as u32, 1, |x, _| {
            image::Luma([row[x as usize].as_u64().unwrap() as u8])
        })
    }
    fn gray_bgr_row(value: &Value) -> GrayImage {
        let row = value.as_array().unwrap()[0].as_array().unwrap();
        GrayImage::from_fn(row.len() as u32, 1, |x, _| {
            image::Luma([row[x as usize][0].as_u64().unwrap() as u8])
        })
    }
    #[test]
    fn cv2_clahe_matches_reference_bytes() {
        let f = fixture();
        for key in ["clahe_7x5", "clahe_17x19", "clahe_32x32"] {
            let input = gray(&f[key]["input"]);
            let expected = gray(&f[key]["out"]);
            let actual = clahe(&input, 4.0);
            let max = actual
                .as_raw()
                .iter()
                .zip(expected.as_raw())
                .map(|(a, b)| a.abs_diff(*b))
                .max()
                .unwrap_or(0);
            eprintln!("{key} max byte error={max}");
            assert!(max <= 2, "{key}: maximum pixel error {max}");
        }
    }
    #[test]
    fn calibration_and_gauge_match_python_reference() {
        let f = fixture();
        let c = &f["calibration"];
        let s = GridCalibrationSpecification {
            block_size: 33,
            rows: 3,
            columns: 3,
            exponent: 1.8,
        };
        let actual = s.generate().unwrap();
        let expected_gauge = gray(&c["gauge"]);
        let gauge_error = actual
            .as_raw()
            .iter()
            .zip(expected_gauge.as_raw())
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap_or(0);
        eprintln!("gauge max byte error={gauge_error}");
        assert!(gauge_error <= 1, "maximum gauge pixel error {gauge_error}");
        let (w, h) = s.dimensions().unwrap();
        let scan_rows = c["scan"].as_array().unwrap();
        let scan = RgbImage::from_fn(w, h, |x, y| {
            let p = &scan_rows[y as usize][x as usize];
            image::Rgb([
                p[0].as_u64().unwrap() as u8,
                p[1].as_u64().unwrap() as u8,
                p[2].as_u64().unwrap() as u8,
            ])
        });
        let calibration = Calibration::from_scan(&s, &scan).unwrap();
        assert_eq!(
            calibration.maximum_darkness,
            c["maximum_darkness"].as_f64().unwrap()
        );
        assert_eq!(
            calibration.median_whiteness,
            c["median_whiteness"].as_f64().unwrap()
        );
        let csv = calibration.report_csv();
        let mut csv_lines = csv.lines();
        assert_eq!(
            csv_lines.next(),
            Some("input_gray,local_wood_gray,engraved_gray,observed_darkness,fitted_darkness")
        );
        assert_eq!(csv_lines.count(), calibration.samples.len());
        for (sample, expected) in calibration
            .samples
            .iter()
            .zip(c["samples"].as_array().unwrap())
        {
            assert_eq!(
                sample.input_lightness,
                expected["input_gray"].as_u64().unwrap() as u8
            );
            assert!(
                (sample.background_lightness - expected["local_wood_gray"].as_f64().unwrap()).abs()
                    < 0.01
            );
            assert!(
                (sample.engraved_lightness - expected["engraved_gray"].as_f64().unwrap()).abs()
                    < 0.01
            );
            assert!(
                (sample.measured_darkness - expected["observed_darkness"].as_f64().unwrap()).abs()
                    < 0.01
            );
            assert!(
                (sample.fitted_darkness - expected["fitted_darkness"].as_f64().unwrap()).abs()
                    < 0.01
            );
        }
        for (actual, expected) in calibration
            .forward_darkness
            .iter()
            .zip(c["forward"].as_array().unwrap())
        {
            assert!((actual - expected.as_f64().unwrap()).abs() < 1e-9);
        }
        for (actual, expected) in calibration
            .inverse_darkness
            .iter()
            .zip(c["inverse"].as_array().unwrap())
        {
            assert!((actual - expected.as_f64().unwrap()).abs() < 1e-9);
        }
        let forward_max = calibration
            .forward_darkness
            .iter()
            .zip(c["forward"].as_array().unwrap())
            .map(|(a, b)| (a - b.as_f64().unwrap()).abs())
            .fold(0.0_f64, f64::max);
        let inverse_max = calibration
            .inverse_darkness
            .iter()
            .zip(c["inverse"].as_array().unwrap())
            .map(|(a, b)| (a - b.as_f64().unwrap()).abs())
            .fold(0.0_f64, f64::max);
        eprintln!("calibration forward LUT max error={forward_max:.3e}; inverse LUT max error={inverse_max:.3e}");
        assert_eq!(calibration.inverse_darkness.first(), Some(&0.0));
        assert_eq!(calibration.inverse_darkness.last(), Some(&255.0));
        assert!(calibration
            .samples
            .windows(2)
            .any(|pair| pair[0].measured_darkness > pair[1].measured_darkness
                && pair[0].fitted_darkness == pair[1].fitted_darkness));
    }
    #[test]
    fn luma_uses_image_crate_rgb_order_and_decolorize_accepts_python_neighbors() {
        let red = RgbImage::from_pixel(1, 1, image::Rgb([255, 0, 0]));
        let blue = RgbImage::from_pixel(1, 1, image::Rgb([0, 0, 255]));
        assert_eq!(luma(&red).get_pixel(0, 0)[0], 76);
        assert_eq!(luma(&blue).get_pixel(0, 0)[0], 29);
        let f = fixture();
        let rows = f["photo"]["rgb"].as_array().unwrap();
        let rgb = RgbImage::from_fn(11, 9, |x, y| {
            let p = &rows[y as usize][x as usize];
            image::Rgb([
                p[0].as_u64().unwrap() as u8,
                p[1].as_u64().unwrap() as u8,
                p[2].as_u64().unwrap() as u8,
            ])
        });
        let python_indices = f["photo"]["indices"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as usize)
            .collect::<Vec<_>>();
        let indices = (0..(11 * 9))
            .map(|i| {
                let py_i = (i % 11) * 9 + i / 11;
                let py_j = python_indices[py_i];
                let y = py_j % 9;
                let x = py_j / 9;
                y * 11 + x
            })
            .collect::<Vec<_>>();
        let expected = gray(&f["photo"]["gray"]);
        let actual = decolorize_with_indices(&rgb, &indices);
        let max = actual
            .as_raw()
            .iter()
            .zip(expected.as_raw())
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap_or(0);
        eprintln!("decolorize max byte error={max}");
        assert!(max <= 1, "decolorize max byte error {max}");
    }

    #[test]
    fn luma_gamma_engraving_and_simulation_match_python() {
        let f = fixture();
        let c = &f["calibration"];
        let spec = GridCalibrationSpecification {
            block_size: 33,
            rows: 3,
            columns: 3,
            exponent: 1.8,
        };
        let scan_rows = c["scan"].as_array().unwrap();
        let (w, h) = spec.dimensions().unwrap();
        let scan = RgbImage::from_fn(w, h, |x, y| {
            let p = &scan_rows[y as usize][x as usize];
            image::Rgb([
                p[0].as_u64().unwrap() as u8,
                p[1].as_u64().unwrap() as u8,
                p[2].as_u64().unwrap() as u8,
            ])
        });
        let calibration = Calibration::from_scan(&spec, &scan).unwrap();
        for (gamma, key) in [(0.7, "pipeline_0_7"), (1.8, "pipeline_1_8")] {
            let p = &c[key];
            let rows = p["input"].as_array().unwrap();
            let photo = RgbImage::from_fn(16, 1, |x, y| {
                let v = &rows[y as usize][x as usize];
                image::Rgb([
                    v[0].as_u64().unwrap() as u8,
                    v[1].as_u64().unwrap() as u8,
                    v[2].as_u64().unwrap() as u8,
                ])
            });
            let result = process_photo(
                &photo,
                &calibration,
                &ProcessingSettings {
                    clahe_clip: 0.0,
                    color_method: ColorMethod::Luma,
                    gamma,
                },
            )
            .unwrap();
            for (actual, expected) in result
                .grayscale
                .as_raw()
                .iter()
                .zip(gray_row(&p["gray"]).as_raw())
            {
                assert_eq!(actual, expected);
            }
            for (actual, expected) in result
                .engraving
                .as_raw()
                .iter()
                .zip(gray_row(&p["engraving"]).as_raw())
            {
                assert_eq!(actual, expected);
            }
            for (actual, expected) in result
                .simulation
                .as_raw()
                .iter()
                .zip(gray_bgr_row(&p["simulation"]).as_raw())
            {
                assert_eq!(actual, expected);
            }
        }
    }
}
