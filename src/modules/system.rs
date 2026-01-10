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
                let fs_type = disk.file_system().to_string_lossy().to_lowercase();
                
                // 1. Always include root
                if mount == "/" { return true; }

                // 2. Identify "Real" filesystems vs virtual/pseudo ones
                let is_real_fs = fs_type.contains("ext") || 
                                fs_type.contains("btrfs") || 
                                fs_type.contains("xfs") || 
                                fs_type.contains("zfs") || 
                                fs_type.contains("vfat") || 
                                fs_type.contains("fat") || 
                                fs_type.contains("ntfs") || 
                                fs_type.contains("exfat") ||
                                fs_type.contains("fuseblk") ||
                                fs_type.contains("apfs") ||
                                fs_type.contains("hfs");

                // 3. Mount point categories
                let is_removable_path = mount.starts_with("/media") || 
                                       mount.starts_with("/mnt") || 
                                       mount.starts_with("/run/media");

                let is_system_path = mount.starts_with("/boot")
                        || mount.starts_with("/nix")
                        || mount.starts_with("/snap")
                        || mount.starts_with("/run")
                        || mount.starts_with("/sys")
                        || mount.starts_with("/proc")
                        || mount.starts_with("/dev")
                        || mount.starts_with("/tmp")
                        || mount == "/efi";

                // Logic: Must be a real filesystem AND (be in a removable path OR not be a system path)
                // Also keep 100MB minimum to hide tiny EFI or recovery partitions
                is_real_fs && (is_removable_path || !is_system_path) && disk.total_space() > 100_000_000
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
