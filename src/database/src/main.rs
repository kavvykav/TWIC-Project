use mysql::*;
use mysql::prelude::*;

fn main() -> Result<()> {
    // MySQL connection URL
    let database_url = "mysql://Username:CardID:Fingerprint:Role@localhost:3306/database_name";

    // Establish a connection
    let pool = Pool::new(database_url)?;
    let mut conn = pool.get_conn()?;

    // Create a table
    conn.query_drop(
        r"CREATE TABLE IF NOT EXISTS localDatabase (
            Username VARCHAR(50) NOT NULL,
            CardID INT(50) NOT NULL PRIMARY KEY,
            Fingerprint VARCHAR(50) NOT NULL,
            Role VARCHAR(50) NOT NULL

        )"
    )?;

    // Insert data into the table
    conn.exec_drop(
        r"INSERT INTO localDatabase (Username, CardID, Fingerprint, Role) VALUES (:Username, :CardID, :Fingerprint, :Role)",
        params! {
            "Username" => "Jakob",
            "CardID" => "101",
            "Fingerprint" => "abcd",
            "Role" => "Jaintor",
        },
    )?;

    // Query data from the table
    let users: Vec<(u32, String, String)> = conn
        .query("SELECT Username, CardID, Fingerprint, Role FROM localDatabase")?;

    for user in users {
        println!("Username: {}, CardID: {}, Fingerprint: {}, Role: {}", user.0, user.1, user.2);
    }

    Ok(())
}

