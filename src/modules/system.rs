use crate::app::App;
use dracon_system::{
    DiskSnapshot, ProcessControlContract, ProcessController, ProcessSnapshot, SystemSnapshot,
    SystemSnapshotContract, SystemSnapshotProvider,
};
use dracon_terminal_engine::system::{DiskInfo, ProcessInfo};

pub struct SystemModule {
    monitor: SystemSnapshotProvider,
}

impl SystemModule {
    pub fn new() -> Self {
        Self {
            monitor: SystemSnapshotProvider::new(),
        }
    }

    pub fn get_data(&mut self) -> std::io::Result<SystemSnapshot> {
        self.monitor.capture_snapshot()
    }

    pub fn kill_process(pid: u32) -> std::io::Result<()> {
        let controller = ProcessController;
        controller.kill_process(pid, Some(9))
    }

    pub fn update_app_state(app: &mut App, data: SystemSnapshot) {
        let s = &mut app.system_state;
        s.cpu_usage = data.cpu_usage;
        s.cpu_cores = data.cpu_cores.to_vec();
        s.mem_usage = data.mem_usage as f32;
        s.total_mem = data.total_mem as f32;
        s.swap_usage = data.swap_usage as f32;
        s.total_swap = data.total_swap as f32;
        s.disks = data.disks.into_iter().map(map_disk).collect();
        s.uptime = data.uptime;
        s.processes = data.processes.into_iter().map(map_process).collect();
        s.hostname = data.hostname;
        s.os_name = data.os_name;
        s.os_version = data.os_version;
        s.kernel_version = data.kernel_version;

        s.cpu_history.push(data.cpu_usage as u64);
        if s.cpu_history.len() > 100 {
            s.cpu_history.remove(0);
        }

        if s.core_history.len() != data.cpu_cores.len() {
            s.core_history = vec![vec![0; 100]; data.cpu_cores.len()];
        }
        for (i, &usage) in data.cpu_cores.iter().enumerate() {
            s.core_history[i].push(usage as u64);
            if s.core_history[i].len() > 100 {
                s.core_history[i].remove(0);
            }
        }

        let mem_p = if data.total_mem > 0.0 {
            (data.mem_usage / data.total_mem) * 100.0
        } else {
            0.0
        };
        s.mem_history.push(mem_p as u64);
        if s.mem_history.len() > 100 {
            s.mem_history.remove(0);
        }

        let swap_p = if data.total_swap > 0.0 {
            (data.swap_usage / data.total_swap) * 100.0
        } else {
            0.0
        };
        s.swap_history.push(swap_p as u64);
        if s.swap_history.len() > 100 {
            s.swap_history.remove(0);
        }

        if s.last_net_in > 0 {
            let diff_in = data.net_in.saturating_sub(s.last_net_in);
            let diff_out = data.net_out.saturating_sub(s.last_net_out);
            s.net_in_history.push(diff_in);
            s.net_out_history.push(diff_out);
            if s.net_in_history.len() > 100 {
                s.net_in_history.remove(0);
            }
            if s.net_out_history.len() > 100 {
                s.net_out_history.remove(0);
            }
        }
        s.last_net_in = data.net_in;
        s.last_net_out = data.net_out;
        s.net_in = data.net_in;
        s.net_out = data.net_out;

        app.apply_process_sort();
    }
}

fn map_disk(d: DiskSnapshot) -> DiskInfo {
    DiskInfo {
        name: d.name,
        device: d.device,
        used_space: d.used_space,
        available_space: d.available_space,
        total_space: d.total_space,
        is_mounted: d.is_mounted,
    }
}

fn map_process(p: ProcessSnapshot) -> ProcessInfo {
    ProcessInfo {
        pid: p.pid,
        name: p.name,
        cpu: p.cpu,
        mem: p.mem,
        user: p.user,
        status: p.status,
    }
}
