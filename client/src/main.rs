use std::io;

pub mod core;
use crate::core::{
    ui::App,
};

fn main() -> io::Result<()> {
    let app = App::new();
    let terminal = ratatui::init();

    app.run(terminal)?;
    
    Ok(())
}
