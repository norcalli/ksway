use std::str;

use derive_more::*;
use ksway::{ipc_command, Client};

mod utils;

#[derive(From, Display, Debug)]
enum Error {
    Io(std::io::Error),
    Sway(ksway::Error),
    Utf8(str::Utf8Error),
    Json(json::Error),
}

fn main() -> Result<(), Error> {
    let mut client = Client::connect()?;

    let tree_data = json::parse(str::from_utf8(&client.ipc(ipc_command::get_tree())?)?)?;
    utils::preorder(&tree_data, &mut |value| {
        if value["focused"].as_bool() == Some(true) {
            let target = utils::extract_path(value, std::env::args().skip(1));
            target.write(&mut std::io::stdout().lock()).unwrap();
            return Some(());
        }
        None
    });
    Ok(())
}
