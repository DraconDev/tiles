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

    pub fn update(&mut self, state: &mut SystemState) {
        self.sys.refresh_cpu_usage();
        self.sys.refresh_memory();
        self.disks.refresh_list();
        self.sys.refresh_processes(ProcessesToUpdate::All, true);

        state.cpu_usage = self.sys.global_cpu_usage();
        state.mem_usage = self.sys.used_memory() as f64 / 1024.0 / 1024.0 / 1024.0; // GB
        state.total_mem = self.sys.total_memory() as f64 / 1024.0 / 1024.0 / 1024.0; // GB

        state.disks = self
            .disks
            .iter()
            .map(|disk: &sysinfo::Disk| {
                DiskInfo {
                    name: disk.mount_point().to_string_lossy().to_string(),
                    used_space: (disk.total_space() - disk.available_space()) as f64
                        / 1024.0
                        / 1024.0
                        / 1024.0, // GB
                    total_space: disk.total_space() as f64 / 1024.0 / 1024.0 / 1024.0, // GB
                }
            })
            .collect();

        let mut processes: Vec<_> = self.sys.processes().values().collect();
        processes.sort_by(|a: &&sysinfo::Process, b: &&sysinfo::Process| {
            b.cpu_usage()
                .partial_cmp(&a.cpu_usage())
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        state.processes = processes
            .into_iter()
            .take(10)
            .map(|p: &sysinfo::Process| ProcessInfo {
                pid: p.pid().as_u32(),
                name: p.name().to_string_lossy().to_string(),
                cpu: p.cpu_usage(),
                mem: p.memory(),
            })
            .collect();
    }
}
