use std::str;

use derive_more::*;
use json::JsonValue;
use ksway::{ipc_command, Client};

#[derive(From, Display, Debug)]
enum Error {
    Io(std::io::Error),
    Sway(ksway::Error),
    Utf8(str::Utf8Error),
    Json(json::Error),
}

fn preorder<T, F: Fn(&JsonValue) -> Option<T>>(value: &JsonValue, visitor: &F) -> Option<T> {
    match visitor(value) {
        None => (),
        value => return value,
    }
    match value {
        JsonValue::Object(obj) => {
            for (_k, v) in obj.iter() {
                match preorder(v, visitor) {
                    None => (),
                    value => return value,
                }
            }
        }
        JsonValue::Array(arr) => {
            for v in arr.iter() {
                match preorder(v, visitor) {
                    None => (),
                    value => return value,
                }
            }
        }
        _ => (),
    }
    None
}

fn main() -> Result<(), Error> {
    let mut client = Client::connect()?;

    client.ipc(ipc_command::get_tree())?;
    let tree_data = json::parse(str::from_utf8(&client.ipc(ipc_command::get_tree())?)?)?;
    preorder(&tree_data, &|value| {
        if value["focused"].as_bool() == Some(true) {
            let mut target = value;
            let mut it = std::env::args().skip(1);
            while let Some(part) = it.next() {
                target = &target[part];
            }
            target.write(&mut std::io::stdout().lock()).unwrap();
            return Some(());
        }
        None
    });
    Ok(())
}
