use byteorder::{NativeEndian, ReadBytesExt};
use crossbeam_channel as chan;
use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use try_block::try_block;

use crate::ipc_command;
use crate::{guess_sway_socket_path, Error, IpcCommand, IpcEvent, SwayResult};

pub struct Client {
    socket: UnixStream,
    socket_path: PathBuf,
    subscription_events: Option<chan::Sender<Response>>,
}

// struct Subscription {
//     socket: UnixStream,
//     buffer: Vec<u8>,
// }

// impl Iterator for Subscription {
//     type Item = (u32, Vec<u8>);
// }

type Response = (u32, Vec<u8>);

impl Client {
    // fn receive() -> SwayResult<(u32, Vec<u8>)> {
    //     Ok((0, vec![]))
    // }

    // fn try_receive(&mut self, types: &[u32]) -> SwayResult<Option<(u32, Vec<u8>)>> {
    //     let mut i = self.payloads.len();
    //     while i > 0 {
    //         let payload = &self.payloads[i];
    //         if types.contains(&payload.0) {
    //             return Ok(Some(self.payloads.remove(i)));
    //         }
    //         i -= 1;
    //     }
    //     let payload = self.read_response()?;
    //     if types.contains(&payload.0) {
    //         Ok(Some(payload))
    //     } else {
    //         self.payloads.push(payload);
    //         Ok(None)
    //     }
    // }

    pub fn path(&self) -> &Path {
        &self.socket_path
    }

    pub fn connect_to_path<P: Into<PathBuf>>(path: P) -> SwayResult<Self> {
        let path = path.into();
        Ok(Self {
            socket: UnixStream::connect(&path)?,
            socket_path: path,
            subscription_events: None,
        })
    }

    pub fn connect() -> SwayResult<Self> {
        Self::connect_to_path(guess_sway_socket_path()?)
    }

    pub fn read_response(&mut self) -> SwayResult<Response> {
        let mut buffer = *b"i3-ipc";
        self.socket.read_exact(&mut buffer).map_err(Error::Io)?;
        debug_assert_eq!(b"i3-ipc", &buffer);
        let payload_length = self.socket.read_u32::<NativeEndian>().map_err(Error::Io)?;
        let payload_type = self.socket.read_u32::<NativeEndian>().map_err(Error::Io)?;
        let mut buffer = vec![0u8; payload_length as usize];
        self.socket.read_exact(&mut buffer).map_err(Error::Io)?;
        let payload = (payload_type, buffer);
        Ok(payload)
    }

    fn send_command(&mut self, command: IpcCommand) -> SwayResult<()> {
        command.write(&mut self.socket).map_err(Error::Io)?;
        Ok(())
    }

    pub fn ipc(&mut self, command: IpcCommand) -> SwayResult<Response> {
        self.send_command(command)?;
        loop {
            let payload = self.read_response()?;
            if payload.0 & IpcEvent::Workspace as u32 > 0 {
                if let Some(ref tx) = self.subscription_events {
                    tx.send(payload).map_err(|_| Error::SubscriptionError)?;
                }
            } else {
                return Ok(payload);
            }
        }
    }

    pub fn run<T: ToString>(&mut self, command: T) -> SwayResult<Response> {
        self.ipc(ipc_command::run(command.to_string()))
    }

    // fn subscribe(&self) -> SwayResult<impl Iterator<Item = (u32, &'_ [u8])>> {
    pub fn subscribe(
        &mut self,
        event_types: Vec<IpcEvent>,
    ) -> SwayResult<chan::Receiver<Response>> {
        if self.subscription_events.is_some() {
            return Err(Error::AlreadySubscribed);
        }
        let (tx, rx) = chan::unbounded();
        self.subscription_events = Some(tx);

        Ok(rx)

        //         let mut socket = self.socket.try_clone()?;
        //         let mut payload_buffer = Vec::new();
        //         ipc_command::subscribe(event_types).write(&mut socket)?;
        //         let iter = std::iter::from_fn(move || {
        //             // TODO figure out how to surface errors.
        //             let result: io::Result<_> = (|| {
        //                 {
        //                     let mut buffer = *b"i3-ipc";
        //                     socket.read_exact(&mut buffer)?;
        //                     debug_assert_eq!(b"i3-ipc", &buffer);
        //                 }
        //                 let payload_length = socket.read_u32::<NativeEndian>()?;
        //                 let payload_type = socket.read_u32::<NativeEndian>()?;
        //                 // let mut payload_buffer = vec![0u8; payload_length as usize];
        //                 if payload_length as usize > payload_buffer.len() {
        //                     payload_buffer.reserve(payload_length as usize - payload_buffer.len());
        //                 }
        //                 socket.read_exact(&mut payload_buffer)?;
        //                 Ok(payload_type)
        //             })();
        //             let payload_type = result.unwrap();
        //             Some((payload_type, payload_buffer.clone()))
        //         });
        //         Ok(iter)
    }
}
