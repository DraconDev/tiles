use sysinfo::{System, Disks};
use crate::app::{SystemState, DiskInfo};

pub struct SystemModule {
    sys: System,
    disks: Disks,
}

impl SystemModule {
    pub fn new() -> Self {
        let mut sys = System::new_all();
        sys.refresh_all();
        let disks = Disks::new_with_refreshed_list();
        Self { sys, disks }
    }

    pub fn update(&mut self, state: &mut SystemState) {
        self.sys.refresh_cpu_usage();
        self.sys.refresh_memory();
        self.disks.refresh_list();

        state.cpu_usage = self.sys.global_cpu_usage();
        state.mem_usage = self.sys.used_memory() as f64 / 1024.0 / 1024.0 / 1024.0; // GB
        state.total_mem = self.sys.total_memory() as f64 / 1024.0 / 1024.0 / 1024.0; // GB
        
        state.disks = self.disks.iter().map(|disk| {
            DiskInfo {
                name: disk.mount_point().to_string_lossy().to_string(),
                used_space: (disk.total_space() - disk.available_space()) as f64 / 1024.0 / 1024.0 / 1024.0, // GB
                total_space: disk.total_space() as f64 / 1024.0 / 1024.0 / 1024.0, // GB
            }
        }).collect();
    }
}