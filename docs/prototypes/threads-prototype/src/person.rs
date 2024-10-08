use std::sync::mpsc::{Sender, Receiver};
use std::io;
use std::thread;
use std::time::Duration;

pub fn send_values(tx: Sender<String>, rx: Receiver<String>) {
    let mut input = String::new();
    let mut finger = false;

    loop {
        if !finger{
            // Get user input
            println!("Please enter your card ID:");
            io::stdin().read_line(&mut input).expect("Failed to read line");

            let input_id = input.trim();
            if input_id.is_empty() {
                continue;  // Skip empty input
            }

            // Send the input to machine.rs
            tx.send(input_id.to_string()).unwrap();
            thread::sleep(Duration::from_millis(500));  // Simulate some work

            // Receive response from machine.rs
            if let Ok(response) = rx.recv() {
                if response == "1" {
                    break;
                }
                if response == "0"{
                    finger = true;
                    input.clear();
                }
            }
        }
        if finger{
            println!("Please scan your finger:");
            io::stdin().read_line(&mut input).expect("Failed to read line");

            let in_finger = input.trim();
            if in_finger.is_empty() {
                continue;  // Skip empty input
            }

            //Send message
            tx.send(in_finger.to_string()).unwrap();
            thread::sleep(Duration::from_millis(500));  // Simulate some work
            
            if let Ok(response) = rx.recv() {
                if response == "5" {
                    break;
                }
            }

        }

        // Clear the input buffer
        input.clear();
    }
}
