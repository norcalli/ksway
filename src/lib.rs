pub mod client;

pub use client::Client;

use itertools::join;
use serde::Serialize;
pub use serde_json::Value as JsonValue;
use std::io::{self, Write};
use std::path::PathBuf;

// Naming convention: https://docs.microsoft.com/en-us/dotnet/standard/design-guidelines/enum
#[derive(Serialize, Debug, num_derive::FromPrimitive)]
#[serde(rename_all = "snake_case")]
#[repr(u32)]
pub enum IpcEvent {
    Workspace = 0x8000_0000,
    Mode = 0x8000_0002,
    Window = 0x8000_0003,
    BarconfigUpdate = 0x8000_0004,
    Binding = 0x8000_0005,
    Shutdown = 0x8000_0006,
    Tick = 0x8000_0007,
    BarStatusUpdate = 0x8000_0014,
}

#[derive(Debug)]
enum IpcCommandCode {
    RunCommand = 0,
    GetWorkspaces = 1,
    Subscribe = 2,
    GetOutputs = 3,
    GetTree = 4,
    GetMarks = 5,
    GetBarConfig = 6,
    GetVersion = 7,
    GetBindingModes = 8,
    GetConfig = 9,
    SendTick = 10,
}

#[derive(Debug)]
pub enum IpcCommand {
    Run(String),
    GetBarConfig,
    GetBindingModes,
    GetConfig,
    GetMarks,
    GetOutputs,
    GetTree,
    GetVersion,
    GetWorkspaces,
    SendTick(Vec<u8>),
    Subscribe(Vec<IpcEvent>),
}

impl IpcCommand {
    fn write<W: Write>(&self, w: &mut W) -> io::Result<()> {
        w.write_all(b"i3-ipc")?;
        match self {
            IpcCommand::Run(command) => {
                let payload = command.as_bytes();
                w.write_all(&(payload.len() as u32).to_ne_bytes())?;
                w.write_all(&(self.code() as u32).to_ne_bytes())?;
                w.write_all(payload)?;
            }
            IpcCommand::SendTick(payload) => {
                w.write_all(&(payload.len() as u32).to_ne_bytes())?;
                w.write_all(&(self.code() as u32).to_ne_bytes())?;
                w.write_all(payload)?;
            }
            IpcCommand::Subscribe(events) => {
                let mut payload = Vec::new();
                serde_json::to_writer(&mut payload, &events)?;
                w.write_all(&(payload.len() as u32).to_ne_bytes())?;
                w.write_all(&(self.code() as u32).to_ne_bytes())?;
                w.write_all(&payload)?;
            }
            _ => {
                w.write_all(&0u32.to_ne_bytes())?;
                w.write_all(&(self.code() as u32).to_ne_bytes())?;
            }
        }
        Ok(())
    }

    fn code(&self) -> IpcCommandCode {
        use IpcCommandCode::*;
        match self {
            IpcCommand::GetBarConfig => GetBarConfig,
            IpcCommand::GetBindingModes => GetBindingModes,
            IpcCommand::GetConfig => GetConfig,
            IpcCommand::GetMarks => GetMarks,
            IpcCommand::GetOutputs => GetOutputs,
            IpcCommand::GetTree => GetTree,
            IpcCommand::GetVersion => GetVersion,
            IpcCommand::GetWorkspaces => GetWorkspaces,
            IpcCommand::Run(_) => RunCommand,
            IpcCommand::SendTick(_) => SendTick,
            IpcCommand::Subscribe(_) => Subscribe,
        }
    }
}

#[derive(derive_more::From, derive_more::Display, Debug)]
pub enum Error {
    /// Could not find or reliably guess a SWAYSOCK
    SockPathNotFound,
    /// Generic error for subscription problems. Currently includes send failure on the channel
    /// used to contain subscription events.
    SubscriptionError,
    /// Error thrown when you try to subscribe multiple times on a single connection, which is
    /// not supported.
    AlreadySubscribed,
    Io(io::Error),
    Json(serde_json::Error),
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

/// Try to guess the value of SWAYSOCK by first checking for the environment variable or using the
/// most recently created sock file at /run/user/$UID/sway-ipc.*.sock. This is useful for the
/// situation where a command is being run from systemd or outside of the GUI environment.
pub fn guess_sway_socket_path() -> Result<PathBuf> {
    match std::env::var("SWAYSOCK") {
        Ok(path) => Ok(PathBuf::from(path)),
        Err(_) => {
            let entry = globwalk::glob("/run/user/*/sway-ipc.*.sock")
                // Failed to get glob
                .map_err(|_| Error::SockPathNotFound)?
                .next()
                // No entries found
                .ok_or_else(|| Error::SockPathNotFound)?
                // Failed to unwrap entry. oof
                .map_err(|_| Error::SockPathNotFound)?;
            Ok(entry.into_path())
        }
    }
}

pub trait HasIpc {
    /// Send an ipc command. Used with the IpcCommand enum or constructed from the convenience
    /// methods under ksway::ipc_command::*
    /// An alias for `client.ipc(ipc_command::run(...))` is provided at `client.run(...)`
    ///
    /// The result is immediately read, aka this is a synchronous call.
    /// The raw bytes are returned in order to avoid dependency on any particular json
    /// implementation.
    fn ipc(&mut self, command: IpcCommand) -> Result<Vec<u8>>;
}

impl HasIpc for Client {
    fn ipc(&mut self, command: IpcCommand) -> Result<Vec<u8>> {
        self.ipc(command)
    }
}

impl SwayClient for Client {}
impl SwayClientJson for Client {}

pub trait SwayClient: HasIpc {
    /// Alias for `client.ipc(ipc_command::run(...))`. Accepts any string as a parameter, which
    /// would be equivalent to `swaymsg $command`, but some type safety and convenience is provided
    /// via `ksway::Command` and `ksway::command::*` (which provides a function interface instead
    /// of an enum)
    ///
    /// The result is immediately read, aka this is a synchronous call.
    /// The raw bytes are returned in order to avoid dependency on any particular json
    /// implementation.
    fn run<T: ToString>(&mut self, command: T) -> Result<Vec<u8>> {
        self.ipc(ipc_command::run(command.to_string()))
    }

    // TODO:
    //  make into trait for everything other than ipc?
    //  then do HasIpc { ipc() } -> all these commands
    //    - ashkan, Tue 13 Oct 2020 05:57:26 PM JST
    fn get_bar_config(&mut self) -> Result<Vec<u8>> {
        self.ipc(crate::ipc_command::get_bar_config())
    }

    fn get_binding_modes(&mut self) -> Result<Vec<u8>> {
        self.ipc(crate::ipc_command::get_binding_modes())
    }

    fn get_config(&mut self) -> Result<Vec<u8>> {
        self.ipc(crate::ipc_command::get_config())
    }

    fn get_marks(&mut self) -> Result<Vec<u8>> {
        self.ipc(crate::ipc_command::get_marks())
    }

    fn get_outputs(&mut self) -> Result<Vec<u8>> {
        self.ipc(crate::ipc_command::get_outputs())
    }

    fn get_tree(&mut self) -> Result<Vec<u8>> {
        self.ipc(crate::ipc_command::get_tree())
    }

    fn get_version(&mut self) -> Result<Vec<u8>> {
        self.ipc(crate::ipc_command::get_version())
    }

    fn get_workspaces(&mut self) -> Result<Vec<u8>> {
        self.ipc(crate::ipc_command::get_workspaces())
    }
}

mod json {
    use super::{JsonValue, Result, SwayClient};

    pub fn preorder<T, F: FnMut(&JsonValue) -> Option<T>>(
        value: &JsonValue,
        visitor: &mut F,
    ) -> Option<T> {
        match visitor(value) {
            None => (),
            value => return value,
        }
        if let Some(obj) = value.as_object() {
            for (_k, v) in obj.iter() {
                match preorder(v, visitor) {
                    None => (),
                    value => return value,
                }
            }
        } else if let Some(arr) = value.as_array() {
            for v in arr.iter() {
                match preorder(v, visitor) {
                    None => (),
                    value => return value,
                }
            }
        }
        None
    }

    fn payload_to_json(payload: Vec<u8>) -> Result<JsonValue> {
        Ok(serde_json::from_slice(&payload)?)
    }

    pub trait SwayClientJson: SwayClient {
        /// Alias for `client.ipc(ipc_command::run(...))`. Accepts any string as a parameter, which
        /// would be equivalent to `swaymsg $command`, but some type safety and convenience is provided
        /// via `ksway::Command` and `ksway::command::*` (which provides a function interface instead
        /// of an enum)
        ///
        /// The result is immediately read, aka this is a synchronous call.
        /// The raw bytes are returned in order to avoid dependency on any particular json
        /// implementation.
        fn run_json<T: ToString>(&mut self, command: T) -> Result<JsonValue> {
            payload_to_json(self.run(command)?)
        }

        fn get_bar_config_json(&mut self) -> Result<JsonValue> {
            payload_to_json(self.get_bar_config()?)
        }

        fn get_binding_modes_json(&mut self) -> Result<JsonValue> {
            payload_to_json(self.get_binding_modes()?)
        }

        fn get_config_json(&mut self) -> Result<JsonValue> {
            payload_to_json(self.get_config()?)
        }

        fn get_marks_json(&mut self) -> Result<JsonValue> {
            payload_to_json(self.get_marks()?)
        }

        fn get_outputs_json(&mut self) -> Result<JsonValue> {
            payload_to_json(self.get_outputs()?)
        }

        fn get_tree_json(&mut self) -> Result<JsonValue> {
            payload_to_json(self.get_tree()?)
        }

        fn get_version_json(&mut self) -> Result<JsonValue> {
            payload_to_json(self.get_version()?)
        }

        fn get_workspaces_json(&mut self) -> Result<JsonValue> {
            payload_to_json(self.get_workspaces()?)
        }

        fn focused_workspace(&mut self) -> Result<Option<JsonValue>> {
            Ok(self.get_workspaces_json()?.as_array().and_then(|arr| {
                arr.iter()
                    .find(|c| c["focused"].as_bool() == Some(true))
                    .cloned()
            }))
        }

        fn focused_window(&mut self) -> Result<Option<JsonValue>> {
            let tree_data = self.get_tree_json()?;

            Ok(preorder(&tree_data, &mut |value| {
                if value["focused"].as_bool() == Some(true) {
                    return Some(value.clone());
                }
                None
            }))
        }
    }
}

pub use json::SwayClientJson;

pub mod criteria {
    use std::fmt::Display;

    #[derive(derive_more::Display, Debug)]
    pub enum Criteria {
        /// Compare value against the app id. Can be a regular expression. If value is __focused__, then the app id must be the same as that of the
        /// currently focused window. app_id are specific to Wayland applications.
        #[display(fmt = "app_id=\"{}\"", "_0")]
        AppId(OrFocused<String>),

        /// Compare value against the window class. Can be a regular expression. If value is __focused__, then the window class must be the same as
        /// that of the currently focused window. class are specific to X11 applications.
        #[display(fmt = "class=\"{}\"", "_0")]
        Class(OrFocused<String>),

        /// Compare against the internal container ID, which you can find via IPC. If value is __focused__, then the id must be the same as that of the
        /// currently focused window.
        #[display(fmt = "con_id=\"{}\"", "_0")]
        ConId(OrFocused<u64>),

        /// Compare against the window marks. Can be a regular expression.
        #[display(fmt = "con_mark=\"{}\"", "_0")]
        ConMark(String),

        /// Matches floating windows.
        #[display(fmt = "floating")]
        Floating,

        /// Compare value against the X11 window ID. Must be numeric.
        #[display(fmt = "id=\"{}\"", "_0")]
        Id(u64),

        /// Compare value against the window instance. Can be a regular expression. If value is __focused__, then the window instance must be the same
        /// as that of the currently focused window.
        #[display(fmt = "instance=\"{}\"", "_0")]
        Instance(OrFocused<String>),

        /// Compare value against the window shell, such as "xdg_shell" or "xwayland".  Can be a regular expression. If value is __focused__, then the
        /// shell must be the same as that of the currently focused window.
        #[display(fmt = "shell=\"{}\"", "_0")]
        Shell(OrFocused<String>),

        /// Matches tiling windows.
        #[display(fmt = "tiling")]
        Tiling,

        /// Compare against the window title. Can be a regular expression. If value is __focused__, then the window title must be the same as that of
        /// the currently focused window.
        #[display(fmt = "title=\"{}\"", "_0")]
        Title(OrFocused<String>),

        /// Compares the urgent state of the window. Can be "first", "last", "latest", "newest", "oldest" or "recent".
        // TODO make enum
        #[display(fmt = "urgent=\"{}\"", "_0")]
        Urgent(String),

        /// Compare against the window role (WM_WINDOW_ROLE). Can be a regular expression. If value is __focused__, then the window role must be the
        /// same as that of the currently focused window.
        #[display(fmt = "window_role=\"{}\"", "_0")]
        WindowRole(OrFocused<String>),

        /// Compare against the window type (_NET_WM_WINDOW_TYPE). Possible values are normal, dialog, utility, toolbar, splash, menu, dropdown_menu,
        /// popup_menu, tooltip and notification.
        // TODO make enum
        #[display(fmt = "window_type=\"{}\"", "_0")]
        WindowType(String),

        /// Compare against the workspace name for this view. Can be a regular expression. If the value is __focused__, then all the views on the cur‐
        /// rently focused workspace matches.
        #[display(fmt = "workspace=\"{}\"", "_0")]
        Workspace(OrFocused<String>),
    }

    #[derive(derive_more::Display, Debug)]
    pub enum OrFocused<T> {
        #[display(fmt = "__focused__")]
        Focused,
        #[display(fmt = "{}", "_0")]
        Value(T),
    }

    impl<T> From<T> for OrFocused<T> {
        fn from(t: T) -> Self {
            OrFocused::Value(t)
        }
    }

    impl<T> From<Option<T>> for OrFocused<T> {
        fn from(t: Option<T>) -> Self {
            match t {
                Some(t) => OrFocused::Value(t),
                None => OrFocused::Focused,
            }
        }
    }

    impl<T> OrFocused<T> {
        fn map<U, F: FnOnce(T) -> U>(self, f: F) -> OrFocused<U> {
            match self {
                OrFocused::Focused => OrFocused::Focused,
                OrFocused::Value(t) => OrFocused::Value(f(t)),
            }
        }
    }

    pub fn focused<T>() -> OrFocused<T> {
        OrFocused::Focused
    }

    /// Compare value against the app id. Can be a regular expression. If value is __focused__, then the app id must be the same as that of the
    /// currently focused window. app_id are specific to Wayland applications.
    pub fn app_id<S: Display, T: Into<OrFocused<S>>>(t: T) -> Criteria {
        Criteria::AppId(t.into().map(|s| s.to_string()))
    }

    /// Compare value against the window class. Can be a regular expression. If value is __focused__, then the window class must be the same as
    /// that of the currently focused window. class are specific to X11 applications.
    pub fn class<S: Display, T: Into<OrFocused<S>>>(t: T) -> Criteria {
        Criteria::Class(t.into().map(|s| s.to_string()))
    }

    /// Compare against the internal container ID, which you can find via IPC. If value is __focused__, then the id must be the same as that of the
    /// currently focused window.
    pub fn con_id<T: Into<OrFocused<u64>>>(t: T) -> Criteria {
        Criteria::ConId(t.into())
    }

    /// Compare against the window marks. Can be a regular expression.
    pub fn con_mark(t: String) -> Criteria {
        Criteria::ConMark(t)
    }

    /// Matches floating windows.
    pub fn floating() -> Criteria {
        Criteria::Floating
    }

    /// Compare value against the X11 window ID. Must be numeric.
    pub fn id<T: Into<u64>>(t: T) -> Criteria {
        Criteria::Id(t.into())
    }

    /// Compare value against the window instance. Can be a regular expression. If value is __focused__, then the window instance must be the same
    /// as that of the currently focused window.
    pub fn instance<S: Display, T: Into<OrFocused<S>>>(t: T) -> Criteria {
        Criteria::Instance(t.into().map(|s| s.to_string()))
    }

    /// Compare value against the window shell, such as "xdg_shell" or "xwayland".  Can be a regular expression. If value is __focused__, then the
    /// shell must be the same as that of the currently focused window.
    pub fn shell<S: Display, T: Into<OrFocused<S>>>(t: T) -> Criteria {
        Criteria::Shell(t.into().map(|s| s.to_string()))
    }

    /// Matches tiling windows.
    pub fn tiling() -> Criteria {
        Criteria::Tiling
    }

    /// Compare against the window title. Can be a regular expression. If value is __focused__, then the window title must be the same as that of
    /// the currently focused window.
    pub fn title<S: Display, T: Into<OrFocused<S>>>(t: T) -> Criteria {
        Criteria::Title(t.into().map(|s| s.to_string()))
    }

    /// Compares the urgent state of the window. Can be "first", "last", "latest", "newest", "oldest" or "recent".
    // TODO make enum
    pub fn urgent<T: Display>(t: T) -> Criteria {
        Criteria::Urgent(t.to_string())
    }

    /// Compare against the window role (WM_WINDOW_ROLE). Can be a regular expression. If value is __focused__, then the window role must be the
    /// same as that of the currently focused window.
    pub fn window_role<S: Display, T: Into<OrFocused<S>>>(t: T) -> Criteria {
        Criteria::WindowRole(t.into().map(|s| s.to_string()))
    }

    /// Compare against the window type (_NET_WM_WINDOW_TYPE). Possible values are normal, dialog, utility, toolbar, splash, menu, dropdown_menu,
    /// popup_menu, tooltip and notification.
    // TODO make enum
    pub fn window_type<T: Display>(t: T) -> Criteria {
        Criteria::WindowType(t.to_string())
    }

    /// Compare against the workspace name for this view. Can be a regular expression. If the value is __focused__, then all the views on the cur‐
    /// rently focused workspace matches.
    pub fn workspace<T: Into<OrFocused<String>>>(t: T) -> Criteria {
        Criteria::Workspace(t.into())
    }
}

pub mod command {
    use super::Command;

    pub fn exec<T: Into<String>>(t: T) -> Command {
        Command::Exec(t.into())
    }

    pub fn raw<T: Into<String>>(t: T) -> Command {
        Command::Raw(t.into())
    }
}

pub mod ipc_command {
    use super::IpcCommand;
    use super::IpcEvent;

    pub fn get_bar_config() -> IpcCommand {
        IpcCommand::GetBarConfig
    }
    pub fn get_binding_modes() -> IpcCommand {
        IpcCommand::GetBindingModes
    }
    pub fn get_config() -> IpcCommand {
        IpcCommand::GetConfig
    }
    pub fn get_marks() -> IpcCommand {
        IpcCommand::GetMarks
    }
    pub fn get_outputs() -> IpcCommand {
        IpcCommand::GetOutputs
    }
    pub fn get_tree() -> IpcCommand {
        IpcCommand::GetTree
    }
    pub fn get_version() -> IpcCommand {
        IpcCommand::GetVersion
    }
    pub fn get_workspaces() -> IpcCommand {
        IpcCommand::GetWorkspaces
    }

    pub fn run<T: Into<String>>(t: T) -> IpcCommand {
        IpcCommand::Run(t.into())
    }

    pub fn tick<T: Into<Vec<u8>>>(t: T) -> IpcCommand {
        IpcCommand::SendTick(t.into())
    }

    pub fn subscribe<T: Into<Vec<IpcEvent>>>(t: T) -> IpcCommand {
        IpcCommand::Subscribe(t.into())
    }
}

#[derive(derive_more::Display, Debug)]
#[display(
    fmt = "[{}] {}",
    r#"join(criteria.iter().map(ToString::to_string), " ")"#,
    "command"
)]
pub struct CriteriaCommand {
    criteria: Vec<criteria::Criteria>,
    command: Box<Command>,
}

#[derive(derive_more::Display, Debug)]
pub enum Command {
    #[display(fmt = "{}", "_0")]
    WithCriteria(CriteriaCommand),
    #[display(fmt = "exec {}", "_0")]
    Exec(String),
    #[display(fmt = "{}", "_0")]
    Raw(String),
}

impl Command {
    /// Prepend criteria to this command. A vec is used so that ordering can be deterministic,
    /// which can be useful.
    pub fn with_criteria(self, criteria: Vec<criteria::Criteria>) -> Self {
        match self {
            Command::WithCriteria(mut cmd) => {
                cmd.criteria.extend(criteria);
                Command::WithCriteria(cmd)
            }
            _ => Command::WithCriteria(CriteriaCommand {
                criteria,
                command: Box::new(self),
            }),
        }
    }
}

#[macro_export]
macro_rules! cmd {
  ([$($k:ident$(=$v:expr)?)+] $($rest:tt)*) => {
      cmd!($($rest)*).with_criteria(vec![$($crate::criteria::$k($($v)?)),*])
    //   cmd!($($rest)*).with_criteria(vec![$($k($($v)?)),*])
  };
  (exec $($args:tt)*) => {
      $crate::command::exec(format!($($args)*))
  };
  ($($args:tt)*) => {
      $crate::command::raw(format!($($args)*))
  };
}

#[cfg(test)]
mod tests {
    use super::*;

    fn verify_buffer(buf: &[u8], code: IpcCommandCode, payload: &[u8]) {
        let prefix = b"i3-ipc";
        assert_eq!(&buf[0..prefix.len()], prefix);
        assert_eq!(
            buf[prefix.len()..prefix.len() + 4],
            dbg!(payload.len() as u64).to_ne_bytes()
        );
        assert_eq!(
            buf[prefix.len() + 4..prefix.len() + 8],
            dbg!(code as u64).to_ne_bytes()
        );
        assert_eq!(&buf[prefix.len() + 8..], payload);
    }

    #[test]
    fn ipc_command_write() {
        {
            let mut buffer = Vec::new();
            // dbg!(IpcCommand::Run("exec st".into()))
            dbg!(ipc_command::run("exec st"))
                .write(&mut buffer)
                .unwrap();
            verify_buffer(&buffer, IpcCommandCode::RunCommand, b"exec st");
        }
        {
            let mut buffer = Vec::new();
            // dbg!(IpcCommand::SendTick("HELLO WORLD".into()))
            dbg!(ipc_command::tick("HELLO WORLD"))
                .write(&mut buffer)
                .unwrap();
            verify_buffer(&buffer, IpcCommandCode::SendTick, b"HELLO WORLD");
        }
        {
            use IpcEvent::*;
            let mut buffer = Vec::new();
            // dbg!(IpcCommand::Subscribe(vec![Window, Tick]))
            dbg!(ipc_command::subscribe(vec![Window, Tick]))
                .write(&mut buffer)
                .unwrap();
            verify_buffer(&buffer, IpcCommandCode::Subscribe, b"[\"window\",\"tick\"]");
        }
    }

    #[test]
    fn criteria_command() {
        use command::*;
        use criteria::*;

        assert_eq!(&exec("st").to_string(), "exec st");
        assert_eq!(
            &exec("st").with_criteria(vec![con_id(123)]).to_string(),
            r#"[con_id="123"] exec st"#
        );
        assert_eq!(
            &raw("123123")
                .with_criteria(vec![
                    con_mark("123".into()),
                    con_id(123),
                    workspace(focused()),
                ])
                .to_string(),
            r#"[con_mark="123" con_id="123" workspace="__focused__"] 123123"#
        );
    }
}
