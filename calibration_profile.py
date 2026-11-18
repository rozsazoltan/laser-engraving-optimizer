from typing import Generator
from dataclasses import dataclass
import math
import numpy as np
import cv2 as cv


class Image(np.ndarray): pass


class CalibrationArea:

    def __init__(self, top_left_corner, bottom_right_corner, svg_hls_color):
        self.top_left_corner = top_left_corner
        self.bottom_right_corner = bottom_right_corner
        self.svg_hls_color = svg_hls_color


class AbstractCalibrationImageSpecification:

    def get_width(self) -> int:
        pass

    def get_height(self) -> int:
        pass

    def get_whitespace_areas(self) -> Generator[CalibrationArea, None, None]:
        pass

    def get_colored_areas(self) -> Generator[CalibrationArea, None, None]:
        pass


class ARUCOCalibrationSpecification(AbstractCalibrationImageSpecification):

    def __init__(self, block_size: int, marker_size: int, number_of_rows: int, number_of_columns: int):
        self.block_size = block_size
        self.marker_size = marker_size
        self.number_of_rows = number_of_rows
        self.number_of_columns = number_of_columns

        self.calibration_image_width = math.ceil(
            self.block_size + number_of_columns * self.block_size + 3 * (self.block_size / 2))
        self.calibration_image_height = math.ceil(number_of_rows * self.block_size + 2 * (self.block_size / 2))

    def get_width(self) -> int:
        return self.calibration_image_width

    def get_height(self) -> int:
        return self.calibration_image_height

    def get_whitespace_areas(self) -> Generator[CalibrationArea, None, None]:
        x = (self.block_size / 2)
        x_end = (self.block_size / 2) + self.marker_size
        y = self.marker_size + 2 * (self.block_size / 2)
        y_end = self.calibration_image_height - (self.block_size / 2)

        white_hls = np.uint8([[[0, 255, 0]]])

        yield CalibrationArea((x, y), (x_end, y_end), white_hls)

    def get_colored_areas(self) -> Generator[CalibrationArea, None, None]:
        spacing = self.block_size * 0.15
        number_of_blocks = self.number_of_rows * self.number_of_columns
        block_count = 0
        for row_number in range(self.number_of_rows):
            for column_number in range(self.number_of_columns):
                x = math.ceil(
                    self.marker_size + column_number * self.block_size + 2 * (self.block_size / 2) + spacing)
                y = math.ceil(row_number * self.block_size + (self.block_size / 2) + spacing)

                x_end = math.ceil(x + self.block_size - spacing)
                y_end = math.ceil(y + self.block_size - spacing)

                # we suspect that the engraving darkness follows a root function, therefore we try to get the most
                # helpful data using an exponential function
                lightness = 255 - math.pow((1 + block_count) / number_of_blocks, 1.5) * 255
                hls_color = np.uint8([[[0, lightness, 0]]])

                block_count += 1
                yield CalibrationArea((x, y), (x_end, y_end), hls_color)


class GridCalibrationSpecification(AbstractCalibrationImageSpecification):

    def __init__(self, block_size: int, number_of_rows: int, number_of_columns: int, non_linearity_value: float = 1.8):
        if block_size < 32 or number_of_rows < 2 or number_of_columns < 2 or non_linearity_value <= 0:
            raise ValueError("block_size >= 32, rows/columns >= 2 and curve exponent > 0 required")
        self.block_size = block_size
        self.number_of_rows = number_of_rows
        self.number_of_columns = number_of_columns
        self.non_linearity_value = non_linearity_value

        self.calibration_image_width = math.ceil(number_of_columns * self.block_size)
        self.calibration_image_height = math.ceil(number_of_rows * self.block_size)

    def get_width(self) -> int:
        return self.calibration_image_width

    def get_height(self) -> int:
        return self.calibration_image_height

    def get_whitespace_areas(self) -> Generator[CalibrationArea, None, None]:
        spacing = int(self.block_size * 0.1)
        x, y = spacing, spacing
        x_end = y_end = self.block_size - spacing

        white_hls = np.uint8([[[0, 255, 0]]])

        yield CalibrationArea((x, y), (x_end, y_end), white_hls)

    def get_colored_areas(self) -> Generator[CalibrationArea, None, None]:
        spacing = self.block_size * 0.1
        # vertical safety to give room to smoke
        vertical_safety = (self.block_size - spacing) * 0.3
        # vertical_safety = 0
        number_of_blocks = (self.number_of_rows * self.number_of_columns) - 1
        block_count = -1
        for row_number in range(self.number_of_rows):
            for column_number in range(self.number_of_columns):
                # first block is whitespace
                if block_count == -1:
                    block_count += 1
                    continue

                lightness_factor = math.pow((1 + block_count) / number_of_blocks, self.non_linearity_value)
                local_vertical_safety = vertical_safety * lightness_factor

                x = math.ceil((column_number * self.block_size) + spacing)
                y = math.ceil((row_number * self.block_size) + spacing + local_vertical_safety)

                x_end = math.ceil((column_number + 1) * self.block_size - spacing)
                y_end = math.ceil((row_number + 1) * self.block_size - spacing)

                # we suspect that the engraving darkness follows a root function, therefore we try to get the most
                # helpful data using an exponential function
                lightness = 255 - lightness_factor * 255
                hls_color = np.uint8([[[0, lightness, 0]]])

                block_count += 1
                yield CalibrationArea((x, y), (x_end, y_end), hls_color)


@dataclass(frozen=True)
class CalibrationSample:
    input_lightness: int
    background_lightness: float
    engraved_lightness: float
    measured_darkness: float
    fitted_darkness: float = 0.0


def isotonic_increasing(values: np.ndarray, weights: np.ndarray) -> np.ndarray:
    """Weighted pool-adjacent-violators; preserves nondecreasing measured response."""
    blocks = []
    for index, (value, weight) in enumerate(zip(values, weights)):
        blocks.append([index, index + 1, float(value) * float(weight), float(weight)])
        while len(blocks) >= 2 and blocks[-2][2] / blocks[-2][3] > blocks[-1][2] / blocks[-1][3]:
            last = blocks.pop()
            blocks[-1][1] = last[1]
            blocks[-1][2] += last[2]
            blocks[-1][3] += last[3]
    fitted = np.empty(len(values), dtype=np.float64)
    for start, end, total, weight in blocks:
        fitted[start:end] = total / weight
    return fitted


class CalibrationData:
    """Estimate locally-background-corrected burn response from calibration patches.

    LightBurn grayscale: pixel 255 = no laser, pixel 0 = maximum configured power.
    Scan brightness itself is not comparable across wood patches; subtract each
    colored patch from unengraved margins in the same grid cell.
    """

    def __init__(self, calibration_specification: AbstractCalibrationImageSpecification,
                 calibration_image: Image, noise_floor: float = 2.0):
        if calibration_image is None:
            raise ValueError("Calibration scan could not be loaded")
        if calibration_image.ndim != 3 or calibration_image.shape[2] != 3:
            raise ValueError("Calibration scan must be a BGR color image")
        required = (calibration_specification.get_height(), calibration_specification.get_width())
        if calibration_image.shape[:2] != required:
            raise ValueError(f"Calibration scan shape {calibration_image.shape[:2]} != {required}; "
                             "crop, align and resize scan to calibration gauge dimensions")
        if noise_floor < 0:
            raise ValueError("noise_floor must be nonnegative")

        self.specification = calibration_specification
        self.noise_floor = noise_floor
        self._gray = cv.cvtColor(calibration_image, cv.COLOR_BGR2GRAY)
        self.samples = self._extract_samples()
        self.median_whiteness = float(np.median([s.background_lightness for s in self.samples]))
        self._build_monotonic_curve()

    @staticmethod
    def _median(image: np.ndarray, x0: int, y0: int, x1: int, y1: int) -> float:
        block = image[y0:y1, x0:x1]
        if block.size == 0:
            raise ValueError("Calibration patch too small to sample")
        return float(np.median(block))

    def _extract_samples(self) -> list[CalibrationSample]:
        gray = self._gray
        samples = []
        # Sample each patch's surrounding unburned margins, independent of layout.
        for area in self.specification.get_colored_areas():
            x0, y0 = map(int, area.top_left_corner)
            x1, y1 = map(int, area.bottom_right_corner)
            width = x1 - x0
            inset = max(1, round(width * 0.015))
            margin_end = max(inset + 1, round(width * 0.075))
            py0 = y0 + round((y1 - y0) * 0.20)
            py1 = y1 - round((y1 - y0) * 0.20)
            px0 = x0 + round(width * 0.15)
            px1 = x1 - round(width * 0.15)
            engraved = self._median(gray, px0, py0, px1, py1)
            left = self._median(gray, x0 - margin_end, py0, x0 - inset, py1)
            right = self._median(gray, x1 + inset, py0, x1 + margin_end, py1)
            background = (left + right) / 2.0
            level = int(area.svg_hls_color[0, 0, 1])
            samples.append(CalibrationSample(level, background, engraved,
                                             max(0.0, background - engraved)))
        if not samples:
            raise ValueError("Empty calibration gauge")
        return samples

    def _build_monotonic_curve(self) -> None:
        # Increasing input darkness must never make inferred burn *lighter*.
        # Pure white (255) is an unburned anchor with strong weight.
        raw = [(0, 0.0, 20.0)]
        raw += [(255 - s.input_lightness, s.measured_darkness, 1.0) for s in self.samples]
        raw.sort(key=lambda item: item[0])
        x, d, weights = [], [], []
        for input_darkness, actual_darkness, weight in raw:
            if x and x[-1] == input_darkness:
                d[-1] = (d[-1] * weights[-1] + actual_darkness * weight) / (weights[-1] + weight)
                weights[-1] += weight
            else:
                x.append(input_darkness)
                d.append(actual_darkness)
                weights.append(weight)

        x = np.asarray(x, dtype=np.float64)
        y = isotonic_increasing(np.asarray(d, dtype=np.float64), np.asarray(weights))
        y = np.maximum(0.0, y - self.noise_floor)
        y[0] = 0.0
        if y[-1] <= 1.0:
            raise ValueError("No measurable engraving contrast; check scan, image orientation and laser settings")

        self.maximum_darkness = float(y[-1])
        self.forward_darkness = np.interp(np.arange(256), x, y)
        # First (least burning) input attaining each darkness avoids overburn on plateaus.
        unique_y, first = np.unique(y, return_index=True)
        self.inverse_darkness = np.interp(
            np.linspace(0, self.maximum_darkness, 4096), unique_y, x[first]
        )
        self.svg_lightness_to_engraved_lightness = {
            s.input_lightness: float(self.median_whiteness - self.forward_darkness[255 - s.input_lightness])
            for s in self.samples
        }
        self.fitted_samples = [CalibrationSample(
            s.input_lightness, s.background_lightness, s.engraved_lightness,
            s.measured_darkness, float(self.forward_darkness[255 - s.input_lightness]))
            for s in self.samples]

    def get_dark_light_range(self) -> float:
        return self.maximum_darkness / 255.0

    def invert_desired_darkness(self, desired: np.ndarray) -> np.ndarray:
        levels = np.clip(np.asarray(desired, dtype=np.float32), 0, self.maximum_darkness)
        # The dense inverse LUT is smooth, monotonic and fast on megapixel photos.
        darkness_amount = np.interp(levels, np.linspace(0, self.maximum_darkness, 4096),
                                    self.inverse_darkness)
        return np.clip(255.0 - darkness_amount, 0, 255).astype(np.uint8)

    def simulate(self, input_image: np.ndarray) -> np.ndarray:
        if input_image.ndim == 3:
            input_image = cv.cvtColor(input_image, cv.COLOR_BGR2GRAY)
        measured = self.forward_darkness[255 - input_image.astype(np.uint8)]
        return np.uint8(np.clip(np.rint(self.median_whiteness - measured), 0, 255))

    def report(self) -> list[dict]:
        return [{"input_gray": s.input_lightness,
                 "local_wood_gray": round(s.background_lightness, 2),
                 "engraved_gray": round(s.engraved_lightness, 2),
                 "observed_darkness": round(s.measured_darkness, 2),
                 "fitted_darkness": round(s.fitted_darkness, 2)}
                for s in self.fitted_samples]
