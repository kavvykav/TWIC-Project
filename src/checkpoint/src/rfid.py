import time
import random
import string
import sys

import RPi.GPIO as gpio
from mfrc522 import SimpleMFRC522

def write_rfid(id: int):
    r = SimpleMFRC522()

    try:
        data = str(int)
        print("Data for rfid is: ", data)
        r.write(data)
        print("Write complete")
    except Exception as e:
        print(f"error: {e}")
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
    except Exception as e:
        print(f"Error: {e}")
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
