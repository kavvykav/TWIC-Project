import time
import RPi.GPIO as gpio
from mfrc522 import SimpleMFRC522
import sys

def write_rfid(id: int):
    r = SimpleMFRC522()

    while True: 
        # Check for an RFID tag
        try:
            id_detected, data = r.read()  # This returns a tuple (id, data)
        except Exception as e:
            continue  # Skip to the next iteration if there's an error

        if id_detected is not None:  # Tag detected
            r.write(str(id))  # Write to the detected RFID tag
            gpio.cleanup()
            return True

        # If no tag detected, wait a bit before checking again
        time.sleep(0.5)


def read_rfid():
    r = SimpleMFRC522()

    try:
        start_time = time.time()  # Record the start time
        while True:
            # Check if more than 30 seconds have passed
            if time.time() - start_time > 30:
                return None

            # Check for an RFID tag
            try:
                id, data = r.read()
            except Exception as e:
                continue  # Skip to the next iteration if there's an error

            if id is not None:  # Successfully read an RFID tag
                return data

            # If no tag detected, wait a bit before checking again
            time.sleep(0.5)

    except Exception as e:
        return None
    finally:
        gpio.cleanup()


if __name__ == "__main__":
    if len(sys.argv) > 2 and sys.argv[1] == "1":
        id_to_write = int(sys.argv[2])
        write_rfid(id_to_write)
    else:
        read_rfid()
