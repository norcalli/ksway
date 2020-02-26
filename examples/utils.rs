use json::JsonValue;

pub fn preorder<T, F: FnMut(&JsonValue) -> Option<T>>(
    value: &JsonValue,
    visitor: &mut F,
    ) -> Option<T> {
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

pub fn extract_path<'a, 'b, S: AsRef<str>, I: IntoIterator<Item = S>>(
    value: &'a JsonValue,
    path: I,
    ) -> &'a JsonValue {
    let mut target = value;
    let mut it = path.into_iter();
    while let Some(part) = it.next() {
        let part = part.as_ref();
        target = match part.parse::<usize>() {
            Ok(index) => &target[index],
            Err(_) => &target[part],
        };
    }
    target
}

