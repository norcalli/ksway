use std::str;

use derive_more::*;
use json::JsonValue;
use ksway::{command, criteria::*, ipc_command, Client};
use lazy_static::lazy_static;

mod utils;

#[derive(From, Display, Debug)]
enum Error {
    Io(std::io::Error),
    Sway(ksway::Error),
    Utf8(str::Utf8Error),
    Json(json::Error),
    InvalidExpression,
    NoWindows,
}

type Expression = Box<Fn(&JsonValue) -> bool>;

fn make_operator(s: &str) -> fn(&JsonValue, &JsonValue) -> bool {
    match s {
        "==" => |a, b| a == b,
        "!=" => |a, b| a != b,
        "^=" => |a, b| {
            a.as_str()
                .and_then(|a| b.as_str().map(|b| (a, b)))
                .map(|(a, b)| a.starts_with(b))
                .unwrap_or(false)
        },
        "$=" => |a, b| {
            a.as_str()
                .and_then(|a| b.as_str().map(|b| (a, b)))
                .map(|(a, b)| a.ends_with(b))
                .unwrap_or(false)
        },
        ">" => |a, b| {
            a.as_f64()
                .and_then(|a| b.as_f64().map(|b| (a, b)))
                .map(|(a, b)| a > b)
                .unwrap_or(false)
        },
        "<" => |a, b| {
            a.as_f64()
                .and_then(|a| b.as_f64().map(|b| (a, b)))
                .map(|(a, b)| a < b)
                .unwrap_or(false)
        },
        "<=" => |a, b| {
            a.as_f64()
                .and_then(|a| b.as_f64().map(|b| (a, b)))
                .map(|(a, b)| a <= b)
                .unwrap_or(false)
        },
        ">=" => |a, b| {
            a.as_f64()
                .and_then(|a| b.as_f64().map(|b| (a, b)))
                .map(|(a, b)| a >= b)
                .unwrap_or(false)
        },
        _ => panic!("invalid operator"),
    }
}

fn parse_expression(expr: impl AsRef<str>) -> Result<Expression, Error> {
    let s = expr.as_ref();

    lazy_static! {
        static ref RE: regex::Regex = regex::Regex::new(
            r#"(?x)
                      ^(?P<path> # path delimited by /
                          (?:[^/]+)
                          (?:/[^/]+)*
                      )
                      (?P<op>[><^$]=?|[!=]=) # operator
                      (?P<rval>.*)$ # everything on the right (including whitespace)
                      "#
        )
        .unwrap();
    }

    let cap = RE.captures(s).ok_or_else(|| Error::InvalidExpression)?;
    let path = cap.name("path").unwrap().as_str();
    let operator = cap.name("op").unwrap().as_str();
    let rval = cap.name("rval").unwrap().as_str().to_owned();

    let path: Vec<String> = path.split('/').map(|s| s.to_owned()).collect();
    let operator = make_operator(operator);
    let target = json::parse(&rval)?;

    Ok(Box::new(move |js: &JsonValue| {
        operator(utils::extract_path(js, &path), &target)
    }))
}

fn main() -> Result<(), Error> {
    let mut expression: Option<Expression> = None;
    let mut args = std::env::args().skip(1);
    let increment = args
        .next()
        .expect("Need an increment")
        .parse::<i32>()
        .unwrap();
    let mut dry_run = false;
    for arg in args {
        if arg == "-d" {
            dry_run = true;
            continue;
        }
        let new_clause = parse_expression(arg)?;
        expression = match expression {
            Some(expression) => Some(Box::new(move |js: &JsonValue| {
                expression(js) && new_clause(js)
            })),
            None => Some(new_clause),
        };
    }

    let expression = expression.unwrap_or_else(|| Box::new(|_: &JsonValue| true));

    let mut client = Client::connect()?;

    client.ipc(ipc_command::get_tree())?;
    let tree_data = json::parse(str::from_utf8(&client.ipc(ipc_command::get_tree())?)?)?;
    let mut windows = Vec::new();

    utils::preorder(&tree_data, &mut |value| {
        // if value["visible"].as_bool() == Some(true) && expression(value) {
        if expression(value) {
            windows.push(value.clone());
        }
        None::<()>
    });

    if windows.len() == 0 {
        return Err(Error::NoWindows);
    }

    let mut target_window = &windows[0];

    for (i, window) in windows.iter().enumerate() {
        if window["focused"].as_bool() == Some(true) {
            let next_window_idx = ((i + windows.len()) as i32 + increment) as usize % windows.len();
            target_window = &windows[next_window_idx];
            break;
        }
    }

    let window_id = target_window["id"].as_u32().expect("Couldn't find a window id");
    if dry_run {
        println!("{}", window_id);
    } else {
        let _result = client.run(command::raw("focus").with_criteria(vec![con_id(window_id)]));
    }
    Ok(())
}
