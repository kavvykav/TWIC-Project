import sys
import time

import adafruit_fingerprint as adafp
import serial

uart = serial.Serial("/dev/serial0", baudrate=57600, timeout=1)
fpm = adafp.Adafruit_Fingerprint(uart)


def get_fp():
    print("Place finger on Sensor...")

    while fpm.get_image() != adafp.OK:
        pass

    if fpm.image_2_tz(1) != adafp.OK:
        print("Error coverting Image")
        return
    # TODO add some return value other than NULL (I THINK)

    if fpm.finger_search() == adafp.OK:
        fp_id = fpm.finger_id
        print(
            f"Fingerprint Match! ID: {fpm.finger_id} with {fpm.confidence} confidence"
        )
        return fp_id
    else:
        print("No Match Found")


def enroll_fp(fp_id):
    print(f"Enrolling fingerprint for ID {fp_id}. Place finger on sensor")

    for i in range(1, 3):
        while fpm.get_image() != adafp.OK:
            pass

        if fpm.image_2_tz(i) != adafp.OK:
            print("Error processing finger Image")
            return
        if i == 1:
            print("Remove finger and press again")
            time.sleep(2)

    if fpm.create_model() != adafp.OK:
        print("Template did not match")
        return

    if fpm.store_model(fp_id) == adafp.OK:
        print("Template registered with ID: {fp_id}")
        return fp_id
    else:
        print("Registration failed")


if __name__ == "__main__":
    print("Enter 1 for Scan, 2 for Registration...")

    if sys.argv[1] == "1":
        get_fp()
    elif sys.argv[1] == "2":
        fp_id = int(input("enter an ID (1-127): "))
        enroll_fp(fp_id)
    else:
        print("No such option")
