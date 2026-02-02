use terma::system::{SystemMonitor, SystemData};
use crate::app::{App, ProcessColumn};

pub struct SystemModule {
    monitor: SystemMonitor,
}

impl SystemModule {
    pub fn new() -> Self {
        Self {
            monitor: SystemMonitor::new(),
        }
    }

    pub fn get_data(&mut self) -> SystemData {
        self.monitor.get_data()
    }

    pub fn update_app_state(app: &mut App, data: SystemData) {
        let s = &mut app.system_state;
        s.cpu_usage = data.cpu_usage;
        s.cpu_cores = data.cpu_cores.clone();
        s.mem_usage = data.mem_usage;
        s.total_mem = data.total_mem;
        s.swap_usage = data.swap_usage;
        s.total_swap = data.total_swap;
        s.disks = data.disks;
        s.uptime = data.uptime;
        s.processes = data.processes;

        // Sort processes
        let sort_col = app.process_sort_col;
        let sort_asc = app.process_sort_asc;
        s.processes.sort_by(|a, b| {
            let cmp = match sort_col {
                ProcessColumn::Pid => a.pid.cmp(&b.pid),
                ProcessColumn::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                ProcessColumn::Cpu => a
                    .cpu
                    .partial_cmp(&b.cpu)
                    .unwrap_or(std::cmp::Ordering::Equal),
                ProcessColumn::Mem => a
                    .mem
                    .partial_cmp(&b.mem)
                    .unwrap_or(std::cmp::Ordering::Equal),
                ProcessColumn::User => a.user.to_lowercase().cmp(&b.user.to_lowercase()),
                ProcessColumn::Status => a.status.to_lowercase().cmp(&b.status.to_lowercase()),
            };
            if sort_asc {
                cmp
            } else {
                cmp.reverse()
            }
        });

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