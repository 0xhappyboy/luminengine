use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Sys {}

impl Sys {
    pub fn get_cpu_num() -> u64 {
        let num = num_cpus::get();
        num.try_into().unwrap()
    }
}
