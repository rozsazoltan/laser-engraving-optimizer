"""Export raw and monotone-fitted laser calibration measurements."""
import csv
import cv2 as cv
import numpy as np
from calibration_profile import CalibrationData


def save_calibration_report(data: CalibrationData, csv_path='calibration_report.csv',
                            graph_path='calibration_curve.png') -> None:
    rows = data.report()
    with open(csv_path, 'w', newline='', encoding='utf-8') as handle:
        writer = csv.DictWriter(handle, fieldnames=list(rows[0]))
        writer.writeheader()
        writer.writerows(rows)

    image = np.full((520, 820, 3), 255, dtype=np.uint8)
    ox, oy, width, height = 75, 440, 695, 365
    for step in range(0, 101, 25):
        yy = round(oy - height * step / 100)
        cv.line(image, (ox, yy), (ox + width, yy), (222, 222, 222), 1)
        cv.putText(image, str(step) + '%', (18, yy + 5), cv.FONT_HERSHEY_SIMPLEX,
                   .5, (80, 80, 80), 1)
    cv.line(image, (ox, oy), (ox + width, oy), (60, 60, 60), 2)
    cv.line(image, (ox, oy), (ox, oy - height), (60, 60, 60), 2)
    for step in range(0, 256, 51):
        xx = round(ox + width * step / 255)
        cv.putText(image, str(step), (xx - 10, oy + 24), cv.FONT_HERSHEY_SIMPLEX,
                   .45, (80, 80, 80), 1)
    cv.putText(image, 'Input darkness: 0 = white, 255 = black', (ox + 120, 495),
               cv.FONT_HERSHEY_SIMPLEX, .6, (50, 50, 50), 1)
    cv.putText(image, 'Measured burn (red) vs monotone model (blue)', (ox, 46),
               cv.FONT_HERSHEY_SIMPLEX, .62, (50, 50, 50), 2)

    def coordinate(input_darkness, burn):
        return (round(ox + width * input_darkness / 255),
                round(oy - height * np.clip(burn / data.maximum_darkness, 0, 1)))

    for sample in data.fitted_samples:
        cv.circle(image, coordinate(255 - sample.input_lightness, sample.measured_darkness),
                  4, (50, 50, 210), -1)
    coords = np.array([coordinate(i, d) for i, d in enumerate(data.forward_darkness)],
                      dtype=np.int32).reshape(-1, 1, 2)
    cv.polylines(image, [coords], False, (205, 100, 40), 2)
    if not cv.imwrite(graph_path, image):
        raise OSError(f"Cannot write {graph_path}")
