use sysinfo::{Disks, System};

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

    pub fn get_data(&mut self) -> crate::app::SystemData {
        self.sys.refresh_cpu_usage();
        self.sys.refresh_memory();
        self.disks.refresh_list();

        let cpu_usage = self.sys.global_cpu_usage();
        let mem_usage = self.sys.used_memory() as f64 / 1024.0 / 1024.0 / 1024.0; // GB
        let total_mem = self.sys.total_memory() as f64 / 1024.0 / 1024.0 / 1024.0; // GB

        let disks = self
            .disks
            .iter()
            .map(|disk: &sysinfo::Disk| crate::app::DiskInfo {
                name: disk.mount_point().to_string_lossy().to_string(),
                used_space: (disk.total_space() - disk.available_space()) as f64
                    / 1024.0
                    / 1024.0
                    / 1024.0, // GB
                total_space: disk.total_space() as f64 / 1024.0 / 1024.0 / 1024.0, // GB
            })
            .collect();

        crate::app::SystemData {
            cpu_usage,
            mem_usage,
            total_mem,
            disks,
        }
    }
}
