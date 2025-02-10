mod app;

use app::App;
use color_eyre::Result;

fn main() -> Result<()> {
    // Run the TUI app. When a form is submitted, the result is returned.
    let submission = App::new().run()?;
    if let Some(sub) = submission {
        println!("Submission Result: {:#?}", sub);
    } else {
        println!("No submission was made.");
    }
    Ok(())
}
