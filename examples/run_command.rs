use std::str;

use derive_more::*;
use ksway::{cmd, Client, IpcEvent, SwayClient};

#[derive(From, Display, Debug)]
enum Error {
    Io(std::io::Error),
    Sway(ksway::Error),
    Utf8(str::Utf8Error),
}

fn main() -> Result<(), Error> {
    let mut client = Client::connect()?;

    println!("{}", client.socket_path().display());
    // let cmd = command::raw("focus").with_criteria(vec![floating(), title("mpv")]);
    let cmd = cmd!([floating title="mpv"] "focus");
    println!("cmd: {}\n->{}", &cmd, str::from_utf8(&client.run(&cmd)?)?);

    let rx = client.subscribe(vec![IpcEvent::Window, IpcEvent::Tick])?;
    let mut i = 10;
    loop {
        if let Ok(c) = rx.try_recv() {
            println!("{:?}, {}", c.0, str::from_utf8(&c.1)?);
            println!("cmd: {}\n->{}", &cmd, str::from_utf8(&client.run(&cmd)?)?);
            i -= 1;
            if i < 0 {
                break;
            }
        }
        println!("timeout");
        client.poll()?;
    }
    Ok(())
}
