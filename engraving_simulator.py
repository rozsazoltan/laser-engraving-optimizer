"""Predict gray appearance from the same monotone response used for conversion."""
import cv2 as cv
from calibration_profile import CalibrationData


def simulate_engraving(input_image, calibration_data: CalibrationData):
    result = calibration_data.simulate(input_image)
    return cv.cvtColor(result, cv.COLOR_GRAY2BGR)
