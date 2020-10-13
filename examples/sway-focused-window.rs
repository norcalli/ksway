use anyhow::*;
use ksway::{Client, SwayClientJson};

mod utils;

fn main() -> Result<(), Error> {
    let mut client = Client::connect()?;

    let value = client
        .focused_window()?
        .ok_or_else(|| anyhow!("Failed to find focused window"))?;
    let target = utils::extract_path(&value, std::env::args().skip(1));
    serde_json::to_writer(&mut std::io::stdout().lock(), target)?;
    Ok(())
}
