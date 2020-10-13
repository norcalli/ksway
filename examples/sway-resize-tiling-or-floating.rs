use anyhow::*;
use criteria::*;
use ksway::{cmd, SwayClient, SwayClientJson, criteria, Client};
use log::*;
use parse_display::*;
use structopt::StructOpt;

mod utils;

use utils::*;

#[derive(Display, FromStr, Debug, Copy, Clone)]
enum ResolutionPart {
    #[display("{0}/{1}")]
    Ratio(f32, f32),
    #[display("{0}%")]
    Percentage(f32),
    #[display("{0}")]
    Fixed(f32),
}

impl ResolutionPart {
    pub fn pixels(&self, value: f32) -> f32 {
        use ResolutionPart::*;
        match self {
            Ratio(a, b) => a * value / b,
            Fixed(a) => *a,
            Percentage(a) => a * value,
        }
    }
}

#[derive(Display, FromStr, Debug, Copy, Clone)]
enum Resolution {
    #[display("{0}x{1}")]
    Both(ResolutionPart, ResolutionPart),
    #[display("{0}")]
    One(ResolutionPart),
    #[display("W{0}")]
    SquareWidth(ResolutionPart),
    #[display("H{0}")]
    SquareHeight(ResolutionPart),
}

#[derive(StructOpt, Debug)]
struct Opt {
    #[structopt(short, long)]
    gap: Option<u32>,
    resolution: Resolution,
}

fn main() -> Result<()> {
    env_logger::init();
    let opt = Opt::from_args();
    let mut client = Client::connect()?;
    info!("{}", client.socket_path().display());
    let ws_dim = {
        let data = client
            .focused_workspace()?
            .ok_or_else(|| anyhow!("Couldn't find focused workspace"))?;
        let (_, _, w, h) = get_rect(&data).unwrap();
        (w as f32, h as f32)
    };
    let (w, h) = match opt.resolution {
        Resolution::Both(a, b) => (a.pixels(ws_dim.0), b.pixels(ws_dim.1)),
        Resolution::One(a) => (a.pixels(ws_dim.0), a.pixels(ws_dim.1)),
        Resolution::SquareWidth(a) => {
            let v = a.pixels(ws_dim.0);
            (v, v)
        }
        Resolution::SquareHeight(a) => {
            let v = a.pixels(ws_dim.1);
            (v, v)
        }
    };
    let (w, h) = (w as u32, h as u32);
    client.run(cmd!([floating con_id=focused()] "resize set width {} px height {} px", w, h))?;
    {
        let (w, h) = if let Some(gap) = opt.gap {
            ensure!(gap < w, "gap is larger than width");
            ensure!(gap < h, "gap is larger than width");
            (w - gap, h - gap)
        } else {
            (w, h)
        };
        client.run(cmd!([tiling con_id=focused()] "resize set width {} px height {} px", w, h))?;
    }

    Ok(())
}
