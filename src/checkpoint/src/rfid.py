import sys
from datetime import datetime

import RPi.GPIO as gpio
from mfrc522 import SimpleMFRC522


def write_rfid():
    r = SimpleMFRC522()

    try:
        ct_us = int(datetime.now().timestamp())
        r.write(str(ct_us))
        print(ct_us)
    except Exception as e:
        print(f"error: {e}")
    finally:
        print("Cleaning up...")
        gpio.cleanup()


def read_rfid():
    r = SimpleMFRC522()
    try:
        id, data = r.read()
        extr_data = int(data.strip())
        print(extr_data)
        return extr_data
    except Exception as e:
        print(f"Error: {e}")
        return None
    finally:
        gpio.cleanup()

def get_token_id():
    r = SimpleMFRC522()
    try:
        id, data = r.read()
        extr_id = int(id.strip())
        print(extr_id)
        return extr_id
    except Exception as e:
        print(f"Error: {e}")
        return None
    finally:
        gpio.cleanup()


if __name__ == "__main__":

    if len(sys.argv) != 1:
        print("Please enter 1 2 or 3 as a cmdline arg")
    elif sys.argv[1] == 1:
        write_rfid()
    elif sys.argv[1] == 2:
        read_rfid()
    elif sys.argv[1] == 3:
        get_token_id()
