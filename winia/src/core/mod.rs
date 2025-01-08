use std::collections::HashMap;

use lazy_static::lazy_static;
use std::sync::Mutex;

lazy_static!(
    static ref PRE_ID: Mutex<usize> = Mutex::new(0);
    static ref STR_TO_ID: Mutex<HashMap<String, usize>> = Mutex::new(HashMap::new());
);

/// Generate a unique id
pub fn generate_id()->usize{
    let mut id = PRE_ID.lock().unwrap();
    *id += 1;
    *id
}

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

pub fn get_id_by_str(s: &str) ->Option<usize>{
    let str_to_id = STR_TO_ID.lock().unwrap();
    str_to_id.get(s).map(|id| *id)
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