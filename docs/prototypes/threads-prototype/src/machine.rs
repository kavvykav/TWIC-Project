use std::sync::mpsc::{Sender, Receiver};

pub fn receive_values(rx: Receiver<String>, tx: Sender<String>) {
    let id = [101, 95, 43, 48, 86];
    let mut count: u16 = 0;
    let mut found = false;
    let fingers:[i32;5] = [4,2,3,1,6];

    for received in rx {
        if !found{
            let card_id = match received.parse::<u128>() {
                Ok(val) => val,
                Err(_) => {
                    println!("Received invalid input.");
                    continue;
                }
            };
            
    
            for &i in id.iter() {
                if card_id == i {
                    println!("Card recognized, please use fingerprint scanner.");
                    // Send back a message to person.rs
                    tx.send(String::from("0")).unwrap();
                    found = true;
                }
            }
    
            if !found {
                println!("Card not recognized.");
                count += 1;
                if count >= 4 {
                    println!("Too many attempts. Please contact the main office.");
                    tx.send(String::from("1")).unwrap();
                    break;
                }
                else{
                    tx.send(String::from("2")).unwrap();
                }
            }

        }
        if found{
            let finger_id = match received.parse::<i32>() {
                Ok(val) => val,
                Err(_) => {
                    println!("Received invalid input.");
                    continue;
                }
            };

            for &i in fingers.iter() {
                if finger_id == i {
                    println!("Welcome!");
                    tx.send(String::from("5")).unwrap();
                    break;
                }
            }

        }

    }
}
