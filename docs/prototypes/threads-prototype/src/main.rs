mod person;
mod machine;

use std::sync::mpsc;
use std::thread;

fn main() {
    // Create channels for two-way communication
    let (tx1, rx1) = mpsc::channel();  // person -> machine
    let (tx2, rx2) = mpsc::channel();  // machine -> person

    // Spawn the sender (person.rs) thread
    let sender = thread::spawn(move || {
        person::send_values(tx1, rx2);
    });

    // Spawn the receiver (machine.rs) thread
    let receiver = thread::spawn(move || {
        machine::receive_values(rx1, tx2);
    });

    sender.join().unwrap();
    receiver.join().unwrap();
}
