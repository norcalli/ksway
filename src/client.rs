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
    /// The socket path that we are currently connected to.
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    /// Connect to a specific socket.
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

    /// Guess which socket to connect to using `ksway::guess_sway_socket_path()`.
    /// This first checks for SWAYSOCK environment variable, or tries to find an appropriate
    /// socket when run outside of a graphical environment. See `guess_sway_socket_path()` for more.
    pub fn connect() -> Result<Self> {
        Self::connect_to_path(guess_sway_socket_path()?)
    }

    /// Call this to check for new subscription events from the socket.
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

    /// Send an ipc command. Used with the IpcCommand enum or constructed from the convenience
    /// methods under ksway::ipc_command::*
    /// An alias for `client.ipc(ipc_command::run(...))` is provided at `client.run(...)`
    ///
    /// The result is immediately read, aka this is a synchronous call.
    /// The raw bytes are returned in order to avoid dependency on any particular json
    /// implementation.
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

    /// Alias for `client.ipc(ipc_command::run(...))`. Accepts any string as a parameter, which
    /// would be equivalent to `swaymsg $command`, but some type safety and convenience is provided
    /// via `ksway::Command` and `ksway::command::*` (which provides a function interface instead
    /// of an enum)
    ///
    /// The result is immediately read, aka this is a synchronous call.
    /// The raw bytes are returned in order to avoid dependency on any particular json
    /// implementation.
    pub fn run<T: ToString>(&mut self, command: T) -> Result<Vec<u8>> {
        self.ipc(ipc_command::run(command.to_string()))
    }

    /// Subscribe to events from sway. You can only subscribe once for a client connection, but
    /// there's really no point to subscribing multiple times. It will return
    /// Error::AlreadySubscribed if you attempt to do so.
    ///
    /// Returns a crossbeam channel that you can use to poll for events.
    ///
    /// In order to receive events, you must call `client.poll()` to check for new subscription
    /// events. You can see an example of this in the examples.
    /// A minimal loop is as such:
    /// ```rust
    /// let rx = client.subscribe(vec![IpcEvent::Window, IpcEvent::Tick])?;
    /// loop {
    ///     while let Ok((payload_type, payload)) = rx.try_recv() {
    ///         match payload_type {
    ///             IpcEvent::Window => { ... }
    ///         }
    ///     }
    ///     client.poll()?;
    /// }
    /// ```
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
