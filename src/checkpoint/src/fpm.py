import random
import sys
import time

import adafruit_fingerprint as adafp
import serial

uart = serial.Serial("/dev/serial0", baudrate=57600, timeout=1)
fpm = adafp.Adafruit_Fingerprint(uart)


def get_fp():

    while fpm.get_image() != adafp.OK:
        pass

    if fpm.image_2_tz(1) != adafp.OK:
        return
    # TODO add some return value other than NULL (I THINK)

    if fpm.finger_search() == adafp.OK:
        fp_id = fpm.finger_id
        print(
            f"{fpm.finger_id}"
        )
        return fp_id


def enroll_fp(fp_id, checkpoint_id):

    for i in range(1, 3):
        while fpm.get_image() != adafp.OK:
            pass

        if fpm.image_2_tz(i) != adafp.OK:
            return
        if i == 1:
            time.sleep(2)

    if fpm.create_model() != adafp.OK:
        return

    if fpm.store_model(fp_id) == adafp.OK:
        print("f{fp_id}")
        return fp_id


if __name__ == "__main__":

    if sys.argv[1] == "1":
        get_fp()
    elif sys.argv[1] == "2":
        if len(sys.argv) < 3:
            sys.exit(1)

        try:
            fp_id = int(sys.argv[2])  # Get fingerprint ID from main.rs
            checkpoint_id = int(sys.argv[3])  # Get checkpoint ID from main.rs
        except ValueError:
            sys.exit(1)

        enroll_fp(fp_id, checkpoint_id)

