
import random
import string
import sys
import time  # Import time module

import RPi.GPIO as gpio
from mfrc522 import SimpleMFRC522


def write_rfid(id: int):
    r = SimpleMFRC522()

    try:
        print("Data for rfid is: ", id)
        print("Waiting for RFID tag to write...")

        start_time = time.time()  # Record the start time
        while True:
            # Check if more than 30 seconds have passed
            if time.time() - start_time > 30:
                print("Timeout: 30 seconds passed without detecting an RFID tag.")
                return False

            # Attempt to write if an RFID tag is detected
            r.write(id)
            print("Write complete")
            return True

    except Exception as e:
        print(f"Error: {e}")
        return False
    finally:
        print("Cleaning up...")
        gpio.cleanup()


def read_rfid():
    r = SimpleMFRC522()

    try:
        print("Waiting for RFID tag...")
        
        start_time = time.time()  # Record the start time
        while True:
            # Check if more than 30 seconds have passed
            if time.time() - start_time > 30:
                print("Timeout: 30 seconds passed without detecting an RFID tag.")
                return None

            # Check for an RFID tag
            print("Place RFID tag near reader")
            id, data = r.read()

            if id is not None:  # Successfully read an RFID tag
                print(f"RFID Tag ID: {id}")
                print(f"Data for this Tag: {data}")
                return data

            # If no tag detected, wait a bit before checking again
            time.sleep(0.5)

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
