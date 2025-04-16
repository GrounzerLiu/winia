use std::collections::HashMap;

use lazy_static::lazy_static;
use std::sync::Mutex;

lazy_static! {
    static ref PRE_ID: Mutex<usize> = Mutex::new(0);
    static ref STR_TO_ID: Mutex<HashMap<String, usize>> = Mutex::new(HashMap::new());
}

/// Generate a unique id
pub fn generate_id() -> usize {
    let mut id = PRE_ID.lock().unwrap();
    *id += 1;
    *id
}

/// Bind a string to an id
/// Use [get_id_by_name] to get the id by the string
pub fn bind_str_to_id(s: &str, id: usize) {
    if s.is_empty() {
        panic!("The string used to bind to an id cannot be empty");
    }
    let mut str_to_id = STR_TO_ID.lock().unwrap();
    if str_to_id.contains_key(s) {
        panic!("The string has already been bound to an id");
    }
    str_to_id.insert(s.to_string(), id);
}

/// Get the id by the string
/// Use [bind_str_to_id] to bind the string to an id
pub fn get_id_by_name(s: impl AsRef<str>) -> Option<usize> {
    let s = s.as_ref();
    let str_to_id = STR_TO_ID.lock().unwrap();
    str_to_id.get(s).map(|id| *id)
}

pub fn get_name_by_id(id: usize) -> Option<String> {
    let str_to_id = STR_TO_ID.lock().unwrap();
    for (key, value) in str_to_id.iter() {
        if *value == id {
            return Some(key.clone());
        }
    }
    None
}

pub fn unbind_str_to_id(s: impl AsRef<str>) {
    let s = s.as_ref();
    let mut str_to_id = STR_TO_ID.lock().unwrap();
    str_to_id.remove(s);
}

pub fn unbind_id(id: usize) {
    let mut str_to_id = STR_TO_ID.lock().unwrap();
    let mut keys_to_remove = Vec::new();
    for (key, value) in str_to_id.iter() {
        if *value == id {
            keys_to_remove.push(key.clone());
        }
    }
    for key in keys_to_remove {
        str_to_id.remove(&key);
    }
}

#[macro_export]
macro_rules! func{
    (|$($arg:ident),*|, $body:expr) => {
        {
            $(let mut $arg = $arg.clone();)*
            $body
        }
    };
}
