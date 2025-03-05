import time
import RPi.GPIO as gpio
from mfrc522 import SimpleMFRC522
import sys

def write_rfid(id: int):
    r = SimpleMFRC522()

    try:
        print("Data for RFID is: ", id)
        print("Waiting for RFID tag to write...")

        start_time = time.time()  # Record the start time
        while True:
            # Check if more than 30 seconds have passed
            if time.time() - start_time > 30:
                print("Timeout: 30 seconds passed without detecting an RFID tag.")
                return False

            # Check for an RFID tag
            print("Place RFID tag near reader")
            try:
                id_detected, data = r.read()  # This returns a tuple (id, data)
            except Exception as e:
                print(f"Error reading RFID tag: {e}")
                continue  # Skip to the next iteration if there's an error

            if id_detected is not None:  # Tag detected
                print("RFID tag detected, proceeding to write...")
                r.write(str(id))  # Write to the detected RFID tag
                print("Write complete")
                return True

            # If no tag detected, wait a bit before checking again
            time.sleep(0.5)

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
            try:
                id, data = r.read()
            except Exception as e:
                print(f"Error reading RFID tag: {e}")
                continue  # Skip to the next iteration if there's an error

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
        id_to_write = int(sys.argv[2])
        write_rfid(id_to_write)
    else:
        print("Reading Token...")
        read_rfid()
