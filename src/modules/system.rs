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
            .filter(|disk| {
                let mount = disk.mount_point().to_string_lossy();
                // Filter out pseudo-filesystems and small system partitions
                // Standard external/removable mount points
                let is_removable_path = mount.starts_with("/media") || 
                                       mount.starts_with("/mnt") || 
                                       mount.starts_with("/run/media");

                (is_removable_path
                    || (!mount.starts_with("/boot")
                        && !mount.starts_with("/nix")
                        && !mount.starts_with("/snap")
                        && !mount.starts_with("/var/snap")
                        && !mount.starts_with("/run/payload") // specific system run paths
                        && !mount.starts_with("/sys")
                        && !mount.starts_with("/proc")
                        && !mount.starts_with("/dev")
                        && !mount.starts_with("/tmp")
                        && mount != "/efi"))
                    && disk.total_space() > 100_000_000 // At least 100MB (catch smaller USBs)
            })
            .map(|disk: &sysinfo::Disk| crate::app::DiskInfo {
                name: disk.mount_point().to_string_lossy().to_string(),
                used_space: (disk.total_space() - disk.available_space()) as f64,
                available_space: disk.available_space() as f64,
                total_space: disk.total_space() as f64,
            })
            .collect();

        crate::app::SystemData {
            cpu_usage,
            mem_usage,
            total_mem,
            disks,
            processes: Vec::new(), // Placeholder for now
        }
    }
}
