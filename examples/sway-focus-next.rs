use std::str;

use anyhow::*;
use itertools::Itertools;
use ksway::{cmd, Client, JsonValue, SwayClient, SwayClientJson};
use log::*;
use parse_display::*;

mod utils;

type Expression = Box<dyn Fn(&JsonValue) -> bool>;

fn make_operator(s: &str) -> fn(&JsonValue, &JsonValue) -> bool {
    match s {
        "==" => |a, b| a == b,
        "!=" => |a, b| a != b,
        "^=" => |a, b| {
            a.as_str()
                .zip(b.as_str())
                .map(|(a, b)| a.starts_with(b))
                .unwrap_or(false)
        },
        "$=" => |a, b| {
            a.as_str()
                .zip(b.as_str())
                .map(|(a, b)| a.ends_with(b))
                .unwrap_or(false)
        },
        ">" => |a, b| {
            a.as_f64()
                .zip(b.as_f64())
                .map(|(a, b)| a > b)
                .unwrap_or(false)
        },
        "<" => |a, b| {
            a.as_f64()
                .zip(b.as_f64())
                .map(|(a, b)| a < b)
                .unwrap_or(false)
        },
        "<=" => |a, b| {
            a.as_f64()
                .zip(b.as_f64())
                .map(|(a, b)| a <= b)
                .unwrap_or(false)
        },
        ">=" => |a, b| {
            a.as_f64()
                .zip(b.as_f64())
                .map(|(a, b)| a >= b)
                .unwrap_or(false)
        },
        "" => |_, _| true,
        _ => panic!("invalid operator"),
    }
}

#[derive(Debug, Clone)]
struct Path(Vec<String>);

impl std::str::FromStr for Path {
    type Err = Error;
    fn from_str(s: &str) -> Result<Path> {
        Ok(Path(s.split('/').map(|s| s.to_owned()).collect()))
    }
}

impl std::fmt::Display for Path {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.0.iter().format("/"))
    }
}

#[derive(FromStr, Clone, Debug)]
enum Value {
    #[display("{0}")]
    Value(JsonValue),
    #[display("{0}")]
    Path(Path),
}

impl Value {
    fn extract<'a>(&'a self, js: &'a JsonValue) -> &'a JsonValue {
        match &self {
            Value::Path(path) => utils::extract_path(js, &path.0),
            Value::Value(value) => &value,
        }
    }
}

impl std::str::FromStr for Box<Operator> {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self> {
        Ok(Box::new(Operator::from_str(s)?))
    }
}

#[derive(FromStr, Clone, Debug)]
enum Operator {
    #[display("{0}&&{1}")]
    And(Box<Operator>, Box<Operator>),
    #[display("{0}||{1}")]
    Or(Box<Operator>, Box<Operator>),
    #[display("{0}>={1}")]
    Ge(Value, Value),
    #[display("{0}<={1}")]
    Le(Value, Value),
    #[display("{0}=={1}")]
    Eq(Value, Value),
    #[display("{0}!={1}")]
    Ne(Value, Value),
    #[display("{0}^={1}")]
    StartsWith(Value, Value),
    #[display("{0}$={1}")]
    EndsWith(Value, Value),
    #[display("{0}>{1}")]
    Gt(Value, Value),
    #[display("{0}<{1}")]
    Lt(Value, Value),
}

impl Operator {
    fn into_expression(self) -> Expression {
        let (op, l, r) = match self {
            Operator::Gt(a, b) => (make_operator(">"), a, b),
            Operator::Lt(a, b) => (make_operator("<"), a, b),
            Operator::Ge(a, b) => (make_operator(">="), a, b),
            Operator::Le(a, b) => (make_operator("<="), a, b),
            Operator::Eq(a, b) => (make_operator("=="), a, b),
            Operator::Ne(a, b) => (make_operator("!="), a, b),
            Operator::StartsWith(a, b) => (make_operator("^="), a, b),
            Operator::EndsWith(a, b) => (make_operator("$="), a, b),
            Operator::And(a, b) => {
                let l = a.into_expression();
                let r = b.into_expression();
                return Box::new(move |js: &JsonValue| l(js) && r(js));
            }
            Operator::Or(a, b) => {
                let l = a.into_expression();
                let r = b.into_expression();
                return Box::new(move |js: &JsonValue| l(js) || r(js));
            }
        };
        Box::new(move |js: &JsonValue| op(l.extract(js), r.extract(js)))
    }
}

use structopt::StructOpt;

#[derive(StructOpt, Debug)]
struct Opt {
    offset: Option<isize>,
    operations: Vec<Operator>,
    #[structopt(short = "c")]
    count: Option<usize>,
    #[structopt(short = "e")]
    extract: Option<Path>,
}

fn main() -> Result<()> {
    env_logger::init();
    let opt = Opt::from_args();
    debug!("{:?}", opt.operations);
    let mut operation = Box::new("id>=0".parse()?);
    for new_op in opt.operations.into_iter() {
        operation = Box::new(Operator::And(operation, Box::new(new_op)));
    }

    let filter = operation.into_expression();

    let mut client = Client::connect()?;

    let tree_data = client.get_tree_json()?;
    let mut windows = Vec::new();

    utils::preorder(&tree_data, &mut |value| {
        if filter(value) {
            windows.push(value.clone());
        }
        None::<()>
    });

    if windows.is_empty() {
        bail!("no windows found");
    }

    let print_json = opt.count.is_some() || opt.extract.is_some();

    let focused_idx = windows
        .iter()
        .position(|w| w["focused"].as_bool() == Some(true))
        .unwrap_or(0) as isize;
    let start_idx = (focused_idx + opt.offset.unwrap_or(0) as isize).rem_euclid(windows.len() as isize) as usize;
    if print_json {
        let count = opt.count.unwrap_or(1);
        for i in (start_idx..).take(count.min(windows.len()))
        {
            let i = (i as usize).rem_euclid(windows.len());
            println!(
                "{}",
                opt.extract
                    .as_ref()
                    .map(|p| utils::extract_path(&windows[i], &p.0))
                    .unwrap_or(&windows[i])
            );
        }
    } else {
        let mut target_window = &windows[start_idx];
        let window_id = target_window["id"]
            .as_u64()
            .expect("Couldn't find a window id");
        client.run(cmd!([con_id=window_id] "focus"))?;
    };
    Ok(())
}
