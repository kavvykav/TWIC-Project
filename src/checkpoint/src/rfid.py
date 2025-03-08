import random
import string
import sys
from datetime import datetime

import RPi.GPIO as gpio
from mfrc522 import SimpleMFRC522


def write_rfid(id: int):
    r = SimpleMFRC522()

    try:
        ct_us = datetime.now().timestamp() * 1_000_000
        print("Data for rfid is: ", ct_us)
        r.write(str(ct_us))
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
        rfid_conf = read_rfid()
        print(rfid_conf)
