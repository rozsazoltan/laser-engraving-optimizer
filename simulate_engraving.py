import argparse
import cv2 as cv
from calibration_profile import GridCalibrationSpecification, CalibrationData
from engraving_simulator import simulate_engraving
from calibration_report import save_calibration_report

parser = argparse.ArgumentParser(description='Simulate grayscale engraving using measured laser response.')
parser.add_argument('block_size', type=int)
parser.add_argument('row_num', type=int)
parser.add_argument('column_num', type=int)
parser.add_argument('scanned_gauge_path', type=str)
parser.add_argument('photo_path', type=str)
parser.add_argument('--curve-exponent', type=float, default=1.8,
                    help='Must match the calibration gauge; original default 1.8.')
args = parser.parse_args()
scan = cv.imread(args.scanned_gauge_path)
if scan is None:
    parser.error(f'Cannot read scan: {args.scanned_gauge_path}')
spec = GridCalibrationSpecification(args.block_size, args.row_num, args.column_num,
                                    args.curve_exponent)
try:
    calibration = CalibrationData(spec, scan)
    save_calibration_report(calibration)
except ValueError as exc:
    parser.error(str(exc))
photo = cv.imread(args.photo_path)
if photo is None:
    parser.error(f'Cannot read photo: {args.photo_path}')
cv.imwrite('simulated.png', simulate_engraving(photo, calibration))
print('Saved simulated.png, calibration_report.csv and calibration_curve.png')
