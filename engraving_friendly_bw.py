"""Deterministic, photographic grayscale preparation."""
import cv2 as cv
import numpy as np
from decolorize import decolorize


def convert_photo_to_engraving_friendly_bw(image: np.ndarray, calibration_data=None,
                                           clahe_clip: float = 2.0,
                                           color_method: str = 'luma') -> np.ndarray:
    if image is None:
        raise ValueError("Input photo could not be loaded")
    if color_method == 'luma':
        gray = cv.cvtColor(image, cv.COLOR_BGR2GRAY) if image.ndim == 3 else image.copy()
    elif color_method == 'decolorize':
        if image.ndim != 3:
            raise ValueError("decolorize requires 3-channel input")
        # Legacy algorithm's random sampling may vary between runs.
        legacy = decolorize(image)
        gray = np.uint8(np.clip(legacy[:, :, 0], 0, 1) * 255)
    else:
        raise ValueError("color_method must be 'luma' or 'decolorize'")
    if clahe_clip < 0:
        raise ValueError("clahe_clip must be >= 0")
    if clahe_clip == 0:
        return gray
    return cv.createCLAHE(clipLimit=clahe_clip, tileGridSize=(8, 8)).apply(gray)
