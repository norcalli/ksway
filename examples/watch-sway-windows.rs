use log::*;
use redis::{Client as RedisClient, Commands, Connection};

use anyhow::{anyhow, Result};
use ksway::{cmd, Client, IpcEvent, JsonValue, SwayClient, SwayClientJson};

mod utils;

use utils::*;

fn sploosh(client: &mut Client, redis_conn: &mut Connection, container: &JsonValue) -> Result<()> {
    use AlignmentVerbs::*;

    // Get focused workspace
    let focused_workspace = client
        .focused_workspace()?
        .ok_or_else(|| anyhow!("Couldn't find focused workspace"))?;

    debug!("workspace: {}", focused_workspace);

    // Get focused window topleft coords
    let (fx, fy, fw, fh) = get_rect(&container)?;
    // Get workspace rectangle
    let (w_x, w_y, w_w, w_h) = get_rect(&focused_workspace)?;

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

    // let (w_x, w_y) = (0, 0);
    // let outputs = payload_to_json(client.ipc(ipc_command::get_outputs())?)?;
    // let focused_output = outputs
    //     .members()
    //     .find(|c| c["focused"].as_bool() == Some(true))
    //     .ok_or_else(|| anyhow!("Failed to find focused output"))?;
    // let (w_w, w_h) = {
    //     let mode = &focused_output["current_mode"];
    //     (
    //         mode["width"]
    //             .as_i32()
    //             .ok_or_else(|| anyhow!("Invalid mode"))?,
    //         mode["height"]
    //             .as_i32()
    //             .ok_or_else(|| anyhow!("Invalid mode"))?,
    //     )
    // };

    let (cx, cy) = ((fx + fw) / 2, (fy + fh) / 2);

    for value in focused_workspace["floating_nodes"]
        .as_array()
        .unwrap()
        .iter()
    {
        if !(value["type"].as_str() == Some("floating_con")
            && value["visible"].as_bool() == Some(true))
        {
            continue;
        }
        let (r_x, r_y, r_w, r_h) = get_rect(&value)?;
        let window_id = value["id"].as_u64().unwrap();
        debug!("sploosh/window/id = {}", window_id);
        // Use redis to check if something is splooshy
        // TODO: vacuum this out sometimes?
        let n: i32 = redis_conn.hget("sway:splooshy", window_id).unwrap_or(0i32);
        debug!("sploosh/window/sploosh_factor = {}", n);
        if n % 2 == 0 {
            continue;
        }

        // Find the furthest place to send this to based on the verbs.
        // This overlaps windows currently.
        // I should investigate layout algorithms such as cassowary instead.
        let (mx, my) = VERBS
            .iter()
            // Possible positions
            .map(|verbs| {
                let (x, y) = calculate_coords(w_x, w_y, w_w, w_h, r_w, r_h, r_x, r_y, verbs);
                let clamped = (
                    x.max(w_x).min(w_x + w_w - r_w),
                    y.max(w_y).min(w_y + w_h - r_h),
                );
                debug!(
                    "verb test: {:?} -> {:?} -> {:?}",
                    (w_x, w_y, w_w, w_h, r_w, r_h, r_x, r_y, verbs),
                    (x, y),
                    clamped
                );
                clamped
            })
            .max_by_key(|(x, y)| {
                // Distance from topleft of focused window
                let result = (cx - x).pow(2) + (cy - y).pow(2);
                (result as f32).sqrt() as i32
            })
            .unwrap();
        debug!("sploosh/window/(mx,my) = ({}, {})", mx, my);
        // Move the floating window.
        if let Err(err) =
            client.run(cmd!([con_id=window_id] "move absolute position {} {}", mx, my))
        {
            error!("sploosh/move() = {:?}", err);
        }
    }
    Ok(())
}

struct Config {
    max_focused_windows: usize,
}

fn main() -> Result<()> {
    env_logger::init();

    let config = Config {
        max_focused_windows: 100,
    };

    let mut client = Client::connect()?;
    let redis_client = RedisClient::open("redis://127.0.0.1")?;
    let mut redis_conn = redis_client.get_connection()?;

    info!("{}", client.socket_path().display());

    // NOTE:
    //  Clean up the previous FOCUSED_WINDOWS_KEY and ignore errors.
    //    - ashkan, Wed 02 Sep 2020 02:20:30 PM JST
    let _ = redis_conn.del::<_, ()>(FOCUSED_WINDOWS_KEY).ok();

    let rx = client.subscribe(vec![IpcEvent::Window, IpcEvent::Tick])?;
    let mut last_focused = None;
    loop {
        while let Ok((payload_type, payload)) = rx.try_recv() {
            let event: JsonValue = serde_json::from_slice(&payload)?;
            let should_sploosh = match payload_type {
                IpcEvent::Window => {
                    // Focus changes only
                    match event["change"].as_str() {
                        Some("focus") => {
                            let container = &event["container"];
                            // Tiling windows only
                            if container.is_object()
                                && container["type"].as_str() != Some("floating_con")
                            {
                                last_focused = Some(container.clone());
                                // TODO provide a connection to execute actions without
                                // using redis as a middle man.
                                (|| -> Option<()> {
                                    let id = container["id"].as_u64()?;
                                    redis_conn.lpush(FOCUSED_WINDOWS_KEY, id).ok()?;
                                    redis_conn
                                        .ltrim(
                                            FOCUSED_WINDOWS_KEY,
                                            0,
                                            config.max_focused_windows as isize,
                                        )
                                        .ok()
                                })();
                                true
                            } else {
                                false
                            }
                        }
                        Some("floating") => true,
                        _ => false,
                    }
                }
                IpcEvent::Tick => {
                    if event["first"].as_bool() == Some(true) {
                        continue;
                    }
                    let payload = event["payload"].as_str().unwrap();
                    payload == "sploosh"
                }
                _ => false,
            };
            if should_sploosh {
                debug!("tick/sploosh");
                if let Some(ref container) = last_focused {
                    match sploosh(&mut client, &mut redis_conn, &container) {
                        Ok(res) => info!("sploosh() = {:?}", res),
                        Err(err) => error!("sploosh() = {:?}", err),
                    }
                }
            }
        }
        client.poll()?;
    }
}
