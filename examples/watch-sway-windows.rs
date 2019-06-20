use std::str;
use std::thread;
use std::time::Duration;

use criteria::*;
use derive_more::*;
use json::JsonValue;
use ksway::{command, criteria, ipc_command, Client, IpcEvent};
use log::*;
use redis::{Client as RedisClient, Commands, Connection};

#[derive(From, Display, Debug)]
enum Error {
    FocusedWorkspaceNotFound,
    Io(std::io::Error),
    Sway(ksway::Error),
    Utf8(str::Utf8Error),
    Json(json::Error),
    Redis(redis::RedisError),
}

fn preorder<T, F: FnMut(&JsonValue) -> Option<T>>(value: &JsonValue, visitor: &mut F) -> Option<T> {
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

type Result<T> = std::result::Result<T, Error>;

fn payload_to_json(payload: Vec<u8>) -> Result<JsonValue> {
    Ok(json::parse(str::from_utf8(&payload)?)?)
}

#[derive(Debug)]
enum AlignmentVerbs {
    Top,
    Left,
    Right,
    Bottom,
    Center,
    CenterX,
    CenterY,
}

const GAPX: i32 = 0;
const GAPY: i32 = 0;

fn calculate_coords(
    w_x: i32,
    w_y: i32,
    w_w: i32,
    w_h: i32,
    w: i32,
    h: i32,
    ix: i32,
    iy: i32,
    verbs: &[AlignmentVerbs],
) -> (i32, i32) {
    use AlignmentVerbs::*;

    let mut x = ix - w_x;
    let mut y = iy - w_y;
    for verb in verbs {
        match verb {
            Top => y = GAPY,
            Bottom => y = w_h - h - GAPY,
            Left => x = GAPX,
            Right => x = w_w - w - GAPX,
            Center => {
                x = (w_w - w) / 2;
                y = (w_h - h) / 2;
            }
            CenterX => x = (w_w - w) / 2,
            CenterY => y = (w_h - h) / 2,
        }
    }
    return (x + w_x, y + w_y);
}

fn sploosh(client: &mut Client, redis_conn: &mut Connection, container: &JsonValue) -> Result<()> {
    use AlignmentVerbs::*;

    // Get focused workspace
    let workspaces = payload_to_json(client.ipc(ipc_command::get_workspaces())?)?;
    let focused_workspace = workspaces
        .members()
        .find(|c| c["focused"].as_bool() == Some(true))
        .ok_or_else(|| Error::FocusedWorkspaceNotFound)?;

    // Get focused window topleft coords
    let (fx, fy) = {
        let rect = &container["rect"];
        (rect["x"].as_i32().unwrap(), rect["y"].as_i32().unwrap())
    };
    // Get workspace rectangle
    let (w_x, w_y, w_w, w_h) = {
        let rect = &focused_workspace["rect"];
        (
            rect["x"].as_i32().unwrap(),
            rect["y"].as_i32().unwrap(),
            rect["width"].as_i32().unwrap(),
            rect["height"].as_i32().unwrap(),
        )
    };

    // These are the possible positions to send the floating container
    const VERBS: &[&[AlignmentVerbs]] = &[
        &[Top, Left],
        &[Top, Right],
        &[Bottom, Left],
        &[Bottom, Right],
        &[Center],
        // TODO figure this out.
        // &[CenterX, Left],
        // &[CenterX, Right],
        // &[CenterY, Top],
        // &[CenterY, Bottom],
    ];

    let tree = payload_to_json(client.ipc(ipc_command::get_tree())?)?;
    // Find floating & visible windows to reposition
    preorder(&tree, &mut |value| -> Option<()> {
        if !(value["type"].as_str() == Some("floating_con")
            && value["visible"].as_bool() == Some(true))
        {
            return None;
        }
        let rect = &value["rect"];
        if !rect.is_object() {
            return None;
        }
        let window_id = value["id"].as_u32().unwrap();
        debug!("sploosh/window/id = {}", window_id);
        // Use redis to check if something is splooshy
        // TODO: vacuum this out sometimes?
        let n: i32 = redis_conn.hget("sway:splooshy", window_id).unwrap_or(0i32);
        debug!("sploosh/window/sploosh_factor = {}", n);
        if !(n % 2 == 1) {
            return None;
        }
        let r_w = rect["width"].as_i32().unwrap();
        let r_h = rect["height"].as_i32().unwrap();
        let r_x = rect["x"].as_i32().unwrap();
        let r_y = rect["y"].as_i32().unwrap();

        // Find the furthest place to send this to based on the verbs.
        // This overlaps windows currently.
        // I should investigate layout algorithms such as cassowary instead.
        let (mx, my) = VERBS
            .iter()
            // Possible positions
            .map(|verbs| calculate_coords(w_x, w_y, w_w, w_h, r_w, r_h, r_x, r_y, verbs))
            .max_by_key(|(x, y)| {
                // Distance from topleft of focused window
                let result = (fx - x).pow(2) + (fy - y).pow(2);
                (result as f32).sqrt() as i32
            })
            .unwrap();
        debug!("sploosh/window/(mx,my) = ({}, {})", mx, my);
        // Move the floating window.
        let _result = client.run(
            command::raw(format!("move absolute position {} {}", mx, my))
                .with_criteria(vec![con_id(window_id)]),
        );
        match _result {
            Err(err) => error!("sploosh/move() = {:?}", err),
            _ => (),
        }
        None
    });
    Ok(())
}

fn main() -> Result<()> {
    env_logger::init();

    let mut client = Client::connect()?;
    let redis_client = RedisClient::open("redis://127.0.0.1")?;
    let mut redis_conn = redis_client.get_connection()?;
    // let stdout = stdout();
    // let mut stdout = stdout.lock();

    info!("{}", client.path().display());

    let rx = client.subscribe(vec![IpcEvent::Window])?;
    loop {
        while let Ok((_payload_type, payload)) = rx.try_recv() {
            let payload = str::from_utf8(&payload)?;
            debug!("event: {}", &payload);
            let event = json::parse(payload)?;

            // Focus changes only
            match event["change"].as_str() {
                Some("focus") => {
                    let container = &event["container"];
                    // Tiling windows only
                    if container.is_object() && container["type"].as_str() != Some("floating_con") {
                        match sploosh(&mut client, &mut redis_conn, &container) {
                            Err(err) => error!("sploosh() = {:?}", err),
                            res => info!("sploosh() = {:?}", res),
                        }
                    }
                }
                Some("floating") => {
                    let container = &event["container"];
                    thread::sleep(Duration::from_millis(100));
                    // Trigger sploosh by refocusing.
                    client.run(command::raw("focus mode_toggle"))?;
                    client.run(command::raw("focus mode_toggle"))?;
                }
                _ => (),
            }
        }
        // client.poll()?;
        let poll_result = client.poll();
        debug!("poll() = {:?}", poll_result);
    }
    Ok(())
}
