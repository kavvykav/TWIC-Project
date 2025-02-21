import sys
import RPi.GPIO as gpio
from mfrc522 import SimpleMFRC522


def read_rfid():
    r = SimpleMFRC522()

    try:
        _, data = r.read()
        print(data.strip())  # Print only the RFID data
    except Exception:
        print("ERROR")
    finally:
        gpio.cleanup()
        sys.stdout.flush()


if __name__ == "__main__":
    read_rfid()
