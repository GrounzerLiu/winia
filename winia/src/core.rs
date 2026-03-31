use lazy_static::lazy_static;
use std::sync::Mutex;

lazy_static! {
    static ref PRE_ID: Mutex<u32> = Mutex::new(0);
}

/// Generate a unique id
pub fn next_id() -> u32 {
    let mut id = PRE_ID.lock().unwrap();
    *id += 1;
    *id
}
