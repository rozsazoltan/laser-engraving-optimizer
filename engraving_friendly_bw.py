"""Prepare grayscale images for engraving."""
import cv2 as cv
import numpy as np
from decolorize import decolorize


def convert_photo_to_engraving_friendly_bw(image: np.ndarray, calibration_data=None,
                                           clahe_clip: float = 4.0,
                                           color_method: str = 'decolorize') -> np.ndarray:
    if image is None:
        raise ValueError("Input photo could not be loaded")
    if color_method == 'decolorize':
        if image.ndim != 3:
            raise ValueError("decolorize requires 3-channel input")
        gray = decolorize(image)[:, :, 0]
        low, high = np.min(gray), np.max(gray)
        normalized = (gray - low) / (high - low) if high > low else np.zeros_like(gray)
        gray = np.clip(np.floor(normalized * 256), 0, 255).astype(np.uint8)
    elif color_method == 'luma':
        gray = cv.cvtColor(image, cv.COLOR_BGR2GRAY) if image.ndim == 3 else image.copy()
    else:
        raise ValueError("color_method must be 'luma' or 'decolorize'")
    if clahe_clip < 0:
        raise ValueError("clahe_clip must be >= 0")
    if clahe_clip == 0:
        return gray
    return cv.createCLAHE(clipLimit=clahe_clip, tileGridSize=(16, 16)).apply(gray)
