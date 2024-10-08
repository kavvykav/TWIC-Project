use std::sync::mpsc::{Sender, Receiver};
use std::io;
use std::thread;
use std::time::Duration;


pub fn send_values(tx: Sender<String>, rx: Receiver<String>) {
    let mut input = String::new();//Variable to store input from console
    let mut finger = false;//Boolean to determine if finger input or id input
    //In the real implementation we won't have a finger variable, there will just be inputs from the id sensor and 
    //the finger reader but for what we want to implement right now it will work

    loop {
        if !finger{
            //ID or finger input
        
            println!("Please enter your card ID:");
            io::stdin().read_line(&mut input).expect("Failed to read line");

            let input_id = input.trim();
            if input_id.is_empty() {
                continue;  // Currently just skip empty input but later add something for empty input and non numerical input
                //Not a big worry as for real card reader or finger print, can't really send incorrect data type
            }

            // Send the input to machine.rs
            tx.send(input_id.to_string()).unwrap();
            thread::sleep(Duration::from_millis(500));  // Simulate some work

            // Receive response from machine.rs
            if let Ok(response) = rx.recv() {
                if response == "1" { //If bad input
                    break;
                }
                if response == "0"{ //If good input
                    finger = true;
                    input.clear(); //Need to clear here or will be used with finger
                }
            }
        }
        if finger{
            //Input finger
            println!("Please scan your finger:");
            io::stdin().read_line(&mut input).expect("Failed to read line");

            let in_finger = input.trim();
            if in_finger.is_empty() {
                continue;  // See above
            }

            //Send message
            tx.send(in_finger.to_string()).unwrap();
            thread::sleep(Duration::from_millis(500));  // Simulate some work
            
            if let Ok(response) = rx.recv() {
                if response == "5" {//If fingerprint good break, later improve behaviour for bad fingerprint
                    break;
                }
            }

        }

        // Clear the input buffer
        input.clear();
    }
}
