import random
import string
import sys

import RPi.GPIO as gpio
from mfrc522 import SimpleMFRC522


def write_rfid(id: int):
    r = SimpleMFRC522()

    try:
        print("Data for rfid is: ", id)
        r.write(id)
        print("Write complete")
        return True
    except Exception as e:
        print(f"error: {e}")
        return False
    finally:
        print("Cleaning up...")
        gpio.cleanup()


def read_rfid():
    r = SimpleMFRC522()

    try:
        print("Place RFID tag near reader")
        id, data = r.read()
        print(f"RFID Tag ID: {id}")
        print(f"Data for this Tag: {data}")
        return data
    except Exception as e:
        print(f"Error: {e}")
        return None
    finally:
        print("Cleaning up...")
        gpio.cleanup()


if __name__ == "__main__":

    if len(sys.argv) > 2 and sys.argv[1] == "1":
        print("Enrolling new user data")
        write_rfid(int(sys.argv[2]))
    else:
        print("Reading Token...")
        read_rfid()
