import random
import string
import sys

import RPi.GPIO as gpio
from mfrc522 import SimpleMFRC522


def write_rfid():
    r = SimpleMFRC522()

    def id_generator(size=5, chars=string.ascii_uppercase + string.digits):
        return "".join(random.choice(chars) for _ in range(size))

    try:
        data = id_generator()
        print("Data for rfid is: ", data)
        r.write(data)
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

    if len(sys.argv) > 1 and sys.argv[1] == "1":
        print("Enrolling new user data")
        write_rfid()
    else:
        print("Reading Token...")
        read_rfid()
