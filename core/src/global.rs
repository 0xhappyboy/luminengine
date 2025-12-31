use std::sync::Mutex;

/// This module contains global mutable static parameters and constants.
use lazy_static::lazy_static;

lazy_static! {
   /// server listening port,default 9999
    static ref SERVER_LISTENING_PORT: Mutex<String> = Mutex::new(String::default());
}
