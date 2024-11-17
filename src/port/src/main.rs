mod person;
mod machine;
mod client;

use std::sync::mpsc;
use std::thread;

fn main() {

    // Start the database updater thread
    let message_handler = thread::spawn(move || { 
        let _ = client::start_client();
    });
    message_handler.join().unwrap();

    loop {
        println!("---------------------------------------------");
        // Create channels for two-way communication
        let (tx1, rx1) = mpsc::channel();  // person -> machine
        let (tx2, rx2) = mpsc::channel();  // machine -> person

        // Spawn person thread
        let sender = thread::spawn(move || {
            person::send_values(tx1, rx2);
        });

        // Spawn machine thread
        let receiver = thread::spawn(move || {
            machine::receive_values(rx1, tx2);
        });

        sender.join().unwrap();
        receiver.join().unwrap();
    }
}
