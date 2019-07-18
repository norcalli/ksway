use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use byteorder::{NativeEndian, ReadBytesExt};
use crossbeam_channel as chan;
use num_traits::FromPrimitive;

use crate::ipc_command;
use crate::{guess_sway_socket_path, Error, IpcCommand, IpcEvent, Result};

pub struct Client {
    socket: UnixStream,
    socket_path: PathBuf,
    subscription_events: Option<chan::Sender<(IpcEvent, Vec<u8>)>>,
}

type RawResponse = (u32, Vec<u8>);

impl Client {
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    pub fn connect_to_path<P: Into<PathBuf>>(path: P) -> Result<Self> {
        let path = path.into();
        let socket = UnixStream::connect(&path)?;
        // socket.set_nonblocking(true)?;
        socket.set_read_timeout(Some(Duration::from_secs(1)))?;
        Ok(Self {
            socket,
            socket_path: path,
            subscription_events: None,
        })
    }

    pub fn connect() -> Result<Self> {
        Self::connect_to_path(guess_sway_socket_path()?)
    }

    pub fn poll(&mut self) -> Result<()> {
        let (payload_type, payload) = match self.read_response() {
            Ok(value) => value,
            // EAGAIN/EWOULDBLOCK means there's no data right now, but this isn't
            // an error for us in this scenario since we are checking with a timeout.
            Err(Error::Io(ref err)) if err.raw_os_error() == Some(11) => return Ok(()),
            err => err?,
        };
        if payload_type & IpcEvent::Workspace as u32 > 0 {
            if let Some(ref tx) = self.subscription_events {
                tx.send((IpcEvent::from_u32(payload_type).unwrap(), payload))
                    .map_err(|_| Error::SubscriptionError)?;
            }
        } else {
            // TODO figure out
            unreachable!();
            // return Ok(payload);
        }
        Ok(())
    }

    fn read_response(&mut self) -> Result<RawResponse> {
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

    fn send_command(&mut self, command: IpcCommand) -> Result<()> {
        command.write(&mut self.socket).map_err(Error::Io)?;
        Ok(())
    }

    pub fn ipc(&mut self, command: IpcCommand) -> Result<Vec<u8>> {
        let code = command.code() as u32;
        self.send_command(command)?;
        loop {
            let (payload_type, payload) = self.read_response()?;
            if payload_type & IpcEvent::Workspace as u32 > 0 {
                if let Some(ref tx) = self.subscription_events {
                    tx.send((IpcEvent::from_u32(payload_type).unwrap(), payload))
                        .map_err(|_| Error::SubscriptionError)?;
                }
            } else {
                debug_assert_eq!(code, payload_type);
                return Ok(payload);
            }
        }
    }

    pub fn run<T: ToString>(&mut self, command: T) -> Result<Vec<u8>> {
        self.ipc(ipc_command::run(command.to_string()))
    }

    pub fn subscribe(
        &mut self,
        event_types: Vec<IpcEvent>,
    ) -> Result<chan::Receiver<(IpcEvent, Vec<u8>)>> {
        if self.subscription_events.is_some() {
            return Err(Error::AlreadySubscribed);
        }
        let (tx, rx) = chan::unbounded();
        self.subscription_events = Some(tx);
        self.ipc(ipc_command::subscribe(event_types))?;

        Ok(rx)
    }
}
