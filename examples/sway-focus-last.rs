use anyhow::Result;
use log::*;
use redis::{Client as RedisClient, Commands};

use ksway::{cmd, Client, SwayClient};

mod utils;

use utils::*;

/*
#!/usr/bin/env lua
local Sway = require 'sway'
local redis = require 'redis'

local sway = Sway.connect()
local red = redis.connect()

local last_id = red:lindex("sway:focused-windows", 1)
local command = Sway.formatCriteria({con_id = last_id}).." focus"

sway:msg(command)
sway:close()
*/

fn main() -> Result<()> {
    env_logger::init();

    let mut client = Client::connect()?;

    info!("{}", client.socket_path().display());

    let redis_client = RedisClient::open("redis://127.0.0.1")?;
    let redis_conn = redis_client.get_connection()?;

    let last_id: u64 = redis_conn.lindex(FOCUSED_WINDOWS_KEY, 1)?;

    client.run(cmd!([con_id=last_id] "focus"))?;
    Ok(())
}
