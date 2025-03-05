import time
import random
import string
import sys

import RPi.GPIO as gpio
from mfrc522 import SimpleMFRC522

def write_rfid(id: int):
    r = SimpleMFRC522()

    try:
        print("Data for rfid is: ", id)
        r.write(str(id))
        print("Write complete")
    except Exception as e:
        print(f"error: {e}")
    finally:
        print("Cleaning up...")
        gpio.cleanup()


def read_rfid():
    r = SimpleMFRC522()

    try:
        id, data = r.read()
        print(f"{data}")
        return data
    except Exception as e:
        print(f"Error: {e}")
        return None
    finally:
        gpio.cleanup()


if __name__ == "__main__":

    if len(sys.argv) > 2 and sys.argv[1] == "1":
        write_rfid(int(sys.argv[2]))
    else:
        read_rfid()
