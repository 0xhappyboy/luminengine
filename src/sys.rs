#[derive(Debug, Clone)]
pub struct Sys {}

impl Sys {
    pub fn new() -> Self {
        Self {}
    }
    pub fn get_cpu_num(&self) -> u64 {
        let num = num_cpus::get();
        num.try_into().unwrap()
    }
}
