use sysinfo::{CpuRefreshKind, System};
use crate::app::SystemState;

pub struct SystemModule {
    sys: System,
}

impl SystemModule {
    pub fn new() -> Self {
        let mut sys = System::new_all();
        sys.refresh_all();
        Self { sys }
    }

    pub fn update(&mut self, state: &mut SystemState) {
        self.sys.refresh_cpu_usage();
        self.sys.refresh_memory();

        state.cpu_usage = self.sys.global_cpu_usage();
        state.mem_usage = self.sys.used_memory() as f64 / 1024.0 / 1024.0 / 1024.0; // GB
        state.total_mem = self.sys.total_memory() as f64 / 1024.0 / 1024.0 / 1024.0; // GB
    }
}