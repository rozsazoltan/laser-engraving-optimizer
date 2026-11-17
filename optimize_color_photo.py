import argparse
import cv2 as cv
from calibration_profile import GridCalibrationSpecification, CalibrationData
from calibration_report import save_calibration_report
from engraving_simulator import simulate_engraving
from bw_to_engraving import prepare_for_engraving
from engraving_friendly_bw import convert_photo_to_engraving_friendly_bw

parser = argparse.ArgumentParser(description='Optimize a photo for calibrated grayscale laser engraving.')
parser.add_argument('block_size', type=int)
parser.add_argument('row_num', type=int)
parser.add_argument('column_num', type=int)
parser.add_argument('scanned_gauge_path', type=str)
parser.add_argument('photo_path', type=str)
parser.add_argument('--curve-exponent', type=float, default=1.8,
                    help='Must match calibration gauge. Original: 1.8; new linear: 1.0.')
parser.add_argument('--gamma', type=float, default=1.0,
                    help='Photo contrast tone: >1 lighter midtones, <1 darker midtones.')
parser.add_argument('--clahe', type=float, default=2.0,
                    help='Local contrast, default 2.0; 0 disables CLAHE.')
parser.add_argument('--color-method', choices=['luma', 'decolorize'], default='luma')
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
print(f'Local wood brightness: {calibration.median_whiteness:.1f}; '
      f'measurable burn range: {calibration.maximum_darkness:.1f} / 255')

photo = cv.imread(args.photo_path)
if photo is None:
    parser.error(f'Cannot read photo: {args.photo_path}')
gray = convert_photo_to_engraving_friendly_bw(photo, calibration, args.clahe,
                                               args.color_method)
cv.imwrite('greyscale.png', gray)
optimized = prepare_for_engraving(gray, calibration, args.gamma)
cv.imwrite('greyscale_for_engraving.png', optimized)
cv.imwrite('engraving_simulation_result.png', simulate_engraving(optimized, calibration))
print('Saved greyscale.png, greyscale_for_engraving.png, engraving_simulation_result.png')
print('Saved calibration_report.csv and calibration_curve.png')
