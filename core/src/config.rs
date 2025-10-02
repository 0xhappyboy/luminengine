use std::sync::Mutex;

use lazy_static::lazy_static;

lazy_static! {
    pub static ref HTTP_LISTENER_PORT: Mutex<String> = Mutex::new("0.0.0.0:8080".to_string());
    pub static ref RPC_LISTENER_PORT: Mutex<String> = Mutex::new("0.0.0.0:8081".to_string());
}
