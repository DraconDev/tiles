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

        let mut final_disks = Vec::new();
        let mut mounted_devices = std::collections::HashSet::new();

        // 1. Get mounted disks from sysinfo
        for disk in self.disks.iter() {
            let mount = disk.mount_point().to_string_lossy();
            let fs_type = disk.file_system().to_string_lossy().to_lowercase();
            
            if mount == "/" {
                final_disks.push(crate::app::DiskInfo {
                    name: mount.to_string(),
                    used_space: (disk.total_space() - disk.available_space()) as f64,
                    available_space: disk.available_space() as f64,
                    total_space: disk.total_space() as f64,
                    is_mounted: true,
                });
                continue;
            }

            let is_real_fs = fs_type.contains("ext") || fs_type.contains("btrfs") || 
                            fs_type.contains("xfs") || fs_type.contains("zfs") || 
                            fs_type.contains("vfat") || fs_type.contains("fat") || 
                            fs_type.contains("ntfs") || fs_type.contains("exfat") ||
                            fs_type.contains("fuseblk");

            let is_removable_path = mount.starts_with("/media") || mount.starts_with("/mnt") || mount.starts_with("/run/media");
            let is_system_path = (mount.starts_with("/boot") || mount.starts_with("/nix") || mount.starts_with("/run") || mount.starts_with("/sys") || mount.starts_with("/proc") || mount.starts_with("/dev") || mount.starts_with("/tmp")) && !is_removable_path;

            if is_real_fs && (is_removable_path || !is_system_path) && disk.total_space() > 100_000_000 {
                final_disks.push(crate::app::DiskInfo {
                    name: mount.to_string(),
                    used_space: (disk.total_space() - disk.available_space()) as f64,
                    available_space: disk.available_space() as f64,
                    total_space: disk.total_space() as f64,
                    is_mounted: true,
                });
                // Track this physical device if possible to avoid duplicates with lsblk
                // sysinfo doesn't easily give us the /dev/sda1 name, but we can try to find it via mount
            }
        }

        // 2. Supplement with unmounted drives from lsblk
        // We look for partitions that have a FSTYPE but no MOUNTPOINT
        if let Ok(output) = std::process::Command::new("lsblk")
            .arg("-rnbo")
            .arg("NAME,FSTYPE,SIZE,MOUNTPOINT,LABEL")
            .output() 
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                let parts: Vec<&str> = line.split(' ').collect();
                if parts.len() >= 3 {
                    let name = parts[0];
                    let fstype = parts[1];
                    let size_str = parts[2];
                    let mountpoint = parts.get(3).cloned().unwrap_or("");
                    let label = parts.get(4).cloned().unwrap_or("");

                    // Only care if it has a filesystem but NO mountpoint
                    if !fstype.is_empty() && mountpoint.is_empty() {
                        if let Ok(size) = size_str.parse::<f64>() {
                            if size > 100_000_000 { // > 100MB
                                let display_name = if !label.is_empty() { label.to_string() } else { format!("/dev/{}", name) };
                                // Avoid adding swap or system-specific types
                                if fstype != "swap" && !fstype.contains("member") {
                                    final_disks.push(crate::app::DiskInfo {
                                        name: display_name,
                                        used_space: 0.0,
                                        available_space: size,
                                        total_space: size,
                                        is_mounted: false,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        crate::app::SystemData {
            cpu_usage,
            mem_usage,
            total_mem,
            disks: final_disks,
            processes: Vec::new(),
        }
    }
}
