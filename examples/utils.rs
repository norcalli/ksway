use anyhow::{anyhow, ensure, Result};
use serde_json::Value as JsonValue;

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

pub fn extract_path<'a, 'b, S: AsRef<str>, I: IntoIterator<Item = S>>(
    value: &'a JsonValue,
    path: I,
) -> &'a JsonValue {
    let mut target = value;
    for part in path.into_iter() {
        let part = part.as_ref();
        target = match part.parse::<usize>() {
            Ok(index) => &target[index],
            Err(_) => &target[part],
        };
    }
    target
}

#[derive(parse_display::FromStr, parse_display::Display, Debug)]
#[display(style = "snake_case")]
pub enum AlignmentVerbs {
    Top,
    Left,
    Right,
    Bottom,
    Center,
    CenterX,
    CenterY,
}

pub fn calculate_coords(
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

    if verbs.is_empty() {
        return (ix, iy);
    }

    let mut x = ix;
    let mut y = iy;
    for verb in verbs {
        match verb {
            Top => y = w_y,
            Bottom => y = w_y + w_h - h,
            Left => x = w_x,
            Right => x = w_x + w_w - w,
            Center => {
                x = (w_w - w) / 2 + w_x;
                y = (w_h - h) / 2 + w_y;
            }
            CenterX => x = (w_w - w) / 2 + w_x,
            CenterY => y = (w_h - h) / 2 + w_y,
        }
    }
    (x, y)
}

pub fn get_rect(value: &JsonValue) -> Result<(i32, i32, i32, i32)> {
    let rect = &value["rect"];
    ensure!(rect.is_object(), "Not a rect");
    let or_err = || anyhow!("Invalid rect");
    Ok((
        rect["x"].as_i64().ok_or_else(or_err)? as i32,
        rect["y"].as_i64().ok_or_else(or_err)? as i32,
        rect["width"].as_i64().ok_or_else(or_err)? as i32,
        rect["height"].as_i64().ok_or_else(or_err)? as i32,
    ))
}

pub const FOCUSED_WINDOWS_KEY: &'static str = "sway:focused-windows";
