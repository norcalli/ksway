# `ksway`

[![docs.rs](https://docs.rs/ksway/badge.svg)](https://docs.rs/ksway)

This library provides a convenient interface for quickly making scripts for
`i3` and [`sway`](https://github.com/swaywm/sway) (since they share an IPC
interface API). It will mainly be focused on `sway` if that compatibility
changes.

It will also be a container for the many scripts I use on a daily basis which
live under `examples/`.

Those examples are the best resource for learning how to use this for complex situations, but here are some small examples:

## Short examples

### connect

```rust
let mut client = Client::connect()?;
let mut client = Client::connect_to_path("/run/user/1000/sway-ipc.1000.1.sock")?;
```

### run commands

The `criteria` implementation is complete and up to date as of 2019-07-27.

```rust
use ksway::{ipc_command, command};


// These are equivalent
client.ipc(ipc_command::run("exec st"))?;
client.ipc(ipc_command::run(command::raw("exec st"))?;
client.ipc(ipc_command::run(command::exec("st"))?;
client.run(command::exec("st"))?;

// The benefit of using command is the additional methods such as .with_criteria

use ksway::criteria::*;

let cmd = command::raw("focus").with_criteria(vec![floating(), title("mpv")]);
client.run(cmd)?;

// criteria examples
let cmd = command::raw("focus").with_criteria(vec![workspace(focused())]);
let cmd = command::raw("focus").with_criteria(vec![con_id(123)])
let cmd = command::raw("focus").with_criteria(vec![con_id(focused())])
```


### `get_*`

```rust
use ksway::ipc_command;

client.ipc(ipc_command::get_tree())?;

let tree_data = json::parse(str::from_utf8(&client.ipc(ipc_command::get_tree())?)?)?;

client.ipc(ipc_command::get_workspaces())?;
client.ipc(ipc_command::get_version())?;
```

### subscribe*

```rust
use ksway::IpcEvent;

let rx = client.subscribe(vec![IpcEvent::Window, IpcEvent::Tick])?;
loop {
	while let Ok((payload_type, payload)) = rx.try_recv() {
		match payload_type {
			IpcEvent::Window => {},
			_ => {},
		}
	}
	client.poll()?;
}
```

## Full examples

You can install these examples with `cargo install ksway --examples` to install all of them or
`cargo install ksway --example sway-focused-window` to install a specific example.

- `examples/sway-focused-window $PATH`: Outputs the json for the currently focused window with no arguments, but you can additionally specify a path to extract, e.g.
	- `sway-focused-window` -> full json
	- `sway-focused-window window_rect width`
	- `sway-focused-window window_properties title`
	- `sway-focused-window id`

- `examples/sway-focus-next $INCREMENT $EXPRESSIONS`: Focus the next window which matches the criteria matched by `$EXPRESSIONS`. By next, I mean, it will try to find the next window after the currently focused one (if the focused one is included in the set of windows specified by $EXPRESSIONS, otherwise it will choose the first window).
	- `sway-focus-next 1 visible==true`
	- `sway-focus-next 1 type==floating`
	- `sway-focus-next 1 visible==true type==$(sway-focused-window type)`
	- `sway-focus-next -1 visible==true type==$(sway-focused-window type)`

- `examples/watch-sway-windows`: Run rules based on the current windows. This is highly personal and customized for my needs and not very well documented.

## TODO

- [ ] Add `serde` typed interface under a feature gate. I plan to generate this with `json_typegen`.
- [ ] Think about making a future based interface for subscribe.
- [ ] Add more commands to `ksway::command::*`, such as `resize` and whatnot.
- [ ] Document all the examples

## Open Design Questions

- [ ] Should I add an `ipc_json` method for deserializing to `json` or `serde_json`?
