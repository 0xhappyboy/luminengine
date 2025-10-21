use serde::{Deserialize, Serialize};

// order book trade target abstract
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct Target {
    pub symbol: String,
}

impl Target {
    pub fn new(symbol: String) -> Self {
        Self { symbol: symbol }
    }
}
