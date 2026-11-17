from calibration_profile import GridCalibrationSpecification
from calibration_image_generator import create_calibration_image
import cv2 as cv
import argparse

parser = argparse.ArgumentParser(description='Create a calibration gauge for engraving.')
parser.add_argument('block_size', type=int, help='Height and width of a block in pixels.')
parser.add_argument('row_num', type=int, help='Number of block rows.')
parser.add_argument('column_num', type=int, help='Number of block columns.')
parser.add_argument('--curve-exponent', type=float, default=1.8,
                    help='Input-tone spacing: 1.0 = linear, 1.8 = original (default). '
                         'Use SAME exponent to analyze the scan.')
args = parser.parse_args()
grid_spec = GridCalibrationSpecification(args.block_size, args.row_num, args.column_num,
                                         args.curve_exponent)
calibration_image = create_calibration_image(grid_spec)
if not cv.imwrite('calibration_gauge.png', calibration_image):
    parser.error('Could not write calibration_gauge.png')
print('Saved calibration_gauge.png; curve exponent:', args.curve_exponent)
