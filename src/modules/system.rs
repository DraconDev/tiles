use crate::app::{DiskInfo, ProcessInfo, SystemState};
use sysinfo::{Disks, ProcessesToUpdate, System};

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
        self.sys.refresh_processes(ProcessesToUpdate::All, true);

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

        let mut processes_list: Vec<_> = self.sys.processes().values().collect();
        processes_list.sort_by(|a: &&sysinfo::Process, b: &&sysinfo::Process| {
            b.cpu_usage()
                .partial_cmp(&a.cpu_usage())
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let processes = processes_list
            .into_iter()
            .take(10)
            .map(|p: &sysinfo::Process| crate::app::ProcessInfo {
                pid: p.pid().as_u32(),
                name: p.name().to_string_lossy().to_string(),
                cpu: p.cpu_usage(),
                mem: p.memory(),
            })
            .collect();

        crate::app::SystemData {
            cpu_usage,
            mem_usage,
            total_mem,
            disks,
            processes,
        }
    }
}
