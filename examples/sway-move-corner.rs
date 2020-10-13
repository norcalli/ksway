use anyhow::{anyhow, bail, Result};
use log::*;

use ksway::{cmd, command, ipc_command, Client, SwayClient, SwayClientJson};

mod utils;

use utils::*;

fn main() -> Result<()> {
    env_logger::init();

    let mut args = std::env::args().skip(1).peekable();
    let mut gaps = vec![];
    while let Some(gap) = args.peek().and_then(|s| s.parse::<i32>().ok()) {
        gaps.push(gap);
        args.next();
        if gaps.len() == 2 {
            break;
        }
    }

    let gaps = match &gaps[..] {
        &[] => (0, 0),
        &[x] => (x, x),
        &[x, y] => (x, y),
        _ => bail!("Too many gaps params"),
    };

    let verbs: Vec<_> = args
        .filter_map(|s| s.parse::<AlignmentVerbs>().ok())
        .collect();

    anyhow::ensure!(!verbs.is_empty(), "No valid verbs found");

    let mut client = Client::connect()?;

    info!("{}", client.socket_path().display());

    let focused_workspace = client
        .focused_workspace()?
        .ok_or_else(|| anyhow!("Couldn't find focused workspace"))?;

    let (w_x, w_y, w_w, w_h) = get_rect(&focused_workspace)?;

    let focused_window = client
        .focused_window()?
        .ok_or_else(|| anyhow!("Couldn't find focused window"))?;
    let (r_x, r_y, r_w, r_h) = get_rect(&focused_window).unwrap();
    let (mx, my) = calculate_coords(w_x, w_y, w_w, w_h, r_w, r_h, r_x, r_y, &verbs);

    client.run(cmd!("move absolute position {} {}", mx, my))?;
    Ok(())
}
