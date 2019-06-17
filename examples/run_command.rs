use std::str;

use ksway::{command, criteria, ipc_command, Client};

fn main() {
    let mut client = Client::connect().unwrap();

    use criteria::*;

    println!("{}", client.path().display());
    let cmd = command::raw("focus").with_criteria(vec![floating(), title("mpv".to_string())]);
    println!("cmd: {}\n->{}", &cmd, str::from_utf8(&client.run(&cmd).unwrap().1).unwrap());
}
