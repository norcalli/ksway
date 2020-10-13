use anyhow::*;
use ksway::{Client, JsonValue, SwayClientJson};
use structopt::StructOpt;

mod utils;

#[derive(StructOpt)]
struct Opt {
    #[structopt(short)]
    default: Option<JsonValue>,
    parts: Vec<String>,
}

fn main() -> Result<()> {
    let mut client = Client::connect()?;
    let opt = Opt::from_args();

    let value = client
        .focused_window()?
        .ok_or_else(|| anyhow!("Failed to find focused window"))?;
    let mut target = utils::extract_path(&value, &opt.parts);
    if target == &JsonValue::Null {
        if let Some(default) = opt.default.as_ref() {
            target = default;
        }
    }
    serde_json::to_writer(&mut std::io::stdout().lock(), target)?;
    Ok(())
}
