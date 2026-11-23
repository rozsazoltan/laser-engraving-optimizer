"""Regenerate small OpenCV/Python goldens from the checked-in reference modules."""
import json
import numpy as np
import cv2 as cv

from calibration_image_generator import create_calibration_image
from calibration_profile import CalibrationData, GridCalibrationSpecification
from decolorize import decolorize
from bw_to_engraving import prepare_for_engraving
from engraving_friendly_bw import convert_photo_to_engraving_friendly_bw
from engraving_simulator import simulate_engraving

OUTPUT = "src/fixtures/python_reference.json"

def main():
    data = {}
    for height, width in ((5, 7), (19, 17), (32, 32)):
        image = np.fromfunction(lambda y, x: (x * 31 + y * 17) % 256,
                                (height, width), dtype=int).astype(np.uint8)
        data[f"clahe_{width}x{height}"] = {
            "input": image.tolist(),
            "out": cv.createCLAHE(clipLimit=4, tileGridSize=(16, 16)).apply(image).tolist(),
        }

    spec = GridCalibrationSpecification(33, 3, 3, 1.8)
    gauge = create_calibration_image(spec)
    scan = gauge.copy()
    burns = (25, 80, 42, 150, 110, 205, 135, 230)
    for index, area in enumerate(spec.get_colored_areas()):
        x, y = area.top_left_corner
        x1, y1 = area.bottom_right_corner
        scan[y:y1, x - 4:x] = np.uint8(205 + 12 * (index % 3))
        scan[y:y1, x1 + 2:x1 + 6] = np.uint8(211)
        scan[y:y1, x + 5:x1 - 5] = np.uint8(
            np.clip(210 + 10 * (index % 3) - burns[index], 0, 255))
    calibration = CalibrationData(spec, scan)
    data["calibration"] = {
        "spec": {"block_size": 33, "rows": 3, "columns": 3, "exponent": 1.8},
        "gauge": gauge[:, :, 0].tolist(),
        "scan": scan.tolist(),
        "samples": calibration.report(),
        "median_whiteness": calibration.median_whiteness,
        "maximum_darkness": calibration.maximum_darkness,
        "forward": calibration.forward_darkness.tolist(),
        "inverse": calibration.inverse_darkness.tolist(),
    }

    photo = np.fromfunction(
        lambda y, x, c: (y * 31 + x * 17 + c * 67) % 256,
        (9, 11, 3), dtype=int).astype(np.uint8)
    seed = 123
    np.random.seed(seed)
    dims = photo.shape
    mesh = np.meshgrid(range(dims[0]), range(dims[1]))
    mesh = np.dstack(mesh).reshape(-1, 2)
    displacement = np.sqrt(2 * min(dims[:2])) * np.sqrt(2 / np.pi)
    look = np.round(mesh + displacement * np.random.normal(size=(dims[0] * dims[1], 2)))
    for axis, size in enumerate(dims[:2]):
        negative = look[:, axis] < 0
        look[negative, axis] = np.abs(look[negative, axis]) % size
        outside = look[:, axis] >= size
        look[outside, axis] = size - (look[outside, axis] % size) - 1
    indices = (look[:, 0] + dims[0] * look[:, 1]).astype(int)

    # Make the original implementation consume captured neighbors, without rewriting its algorithm.
    original_normal = np.random.normal
    def fixed_normal(size=None):
        if size == [dims[0] * dims[1], 2]:
            return (np.column_stack((indices % dims[0], indices // dims[0])) - mesh) / displacement
        return original_normal(size=size)
    np.random.normal = fixed_normal
    try:
        raw = decolorize(photo[:, :, ::-1].copy())[:, :, 0]
    finally:
        np.random.normal = original_normal
    low, high = raw.min(), raw.max()
    normalized = np.clip(np.floor((raw - low) / (high - low) * 256), 0, 255).astype(np.uint8)
    data["photo"] = {"rgb": photo.tolist(), "indices": indices.tolist(),
                     "gray": normalized.tolist(), "sampling_seed": seed}
    for gamma in (0.7, 1.8):
        ramp = np.linspace(0, 255, 16, dtype=np.uint8)
        source = np.stack((ramp, ramp, ramp), axis=1)[None, :, :]
        processed = convert_photo_to_engraving_friendly_bw(
            source, color_method="luma", clahe_clip=0)
        engraving = prepare_for_engraving(processed, calibration, gamma)[:, :, 0]
        simulation = simulate_engraving(engraving, calibration)
        data["calibration"][f"pipeline_{str(gamma).replace('.', '_')}"] = {
            "input": source.tolist(), "gray": processed.tolist(),
            "engraving": engraving.tolist(), "simulation": simulation.tolist(),
        }
    data["luma"] = {"red": 76, "blue": 29}
    with open(OUTPUT, "w", encoding="utf-8") as fixture:
        json.dump(data, fixture, separators=(",", ":"))

if __name__ == "__main__":
    main()
