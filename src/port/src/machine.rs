use std::sync::mpsc::{Sender, Receiver};

pub fn receive_values(rx: Receiver<String>, tx: Sender<String>) {
    let id = [101, 95, 43, 48, 86]; //List of 'IDS'
    let mut count: u16 = 0; //Count number of attempts
    let mut found = false; //Found ID (essentially the finger variable for person.rs but didn't want another finger variable with fingers)
    let fingers:[i32;5] = [4,2,3,1,6]; //List of 'Finger IDs'

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
                    tx.send(String::from("0")).unwrap(); //Found!
                    found = true;
                }
            }
    
            if !found {
                println!("Card not recognized.");
                count += 1;
                if count >= 4 {
                    println!("Too many attempts. Please contact the main office.");
                    tx.send(String::from("1")).unwrap();//They tried too much kill
                    break;
                }
                else{
                    tx.send(String::from("2")).unwrap();//Keep receiving inputs
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
                //Need to add more here for if finger isn't good
            }

        }

    }
}
