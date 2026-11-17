"""Map requested photo tones through locally calibrated, monotone laser response."""
import cv2 as cv
import numpy as np
from calibration_profile import CalibrationData


def prepare_for_engraving(image: np.ndarray, calibration_data: CalibrationData,
                          gamma: float = 1.0) -> np.ndarray:
    """Return 8-bit grayscale BGR for LightBurn Grayscale mode.

    Gamma > 1 favors lighter midtones; gamma < 1 favors deeper midtones.
    Output 255 means laser off; 0 corresponds to maximum programmed power.
    """
    if gamma <= 0:
        raise ValueError("gamma must be > 0")
    if image.ndim == 3:
        image = cv.cvtColor(image, cv.COLOR_BGR2GRAY)
    tonal_darkness = 1.0 - image.astype(np.float32) / 255.0
    wanted = (tonal_darkness ** gamma) * calibration_data.maximum_darkness
    output = calibration_data.invert_desired_darkness(wanted)
    return cv.cvtColor(output, cv.COLOR_GRAY2BGR)
