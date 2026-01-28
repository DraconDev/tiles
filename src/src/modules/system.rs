use terma::system::SystemMonitor;

pub struct SystemModule {
    monitor: SystemMonitor,
}

impl SystemModule {
    pub fn new() -> Self {
        Self {
            monitor: SystemMonitor::new(),
        }
    }

    pub fn get_data(&mut self) -> crate::app::SystemData {
        self.monitor.get_data()
    }
}
