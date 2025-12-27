use bollard::Docker;
use bollard::container::ListContainersOptions;
use std::default::Default;

pub struct DockerModule {
    docker: Docker,
}

impl DockerModule {
    pub fn new() -> color_eyre::Result<Self> {
        let docker = Docker::connect_with_local_defaults()?;
        Ok(Self { docker })
    }

use bollard::models::ContainerSummary;

    pub async fn get_containers(&self) -> color_eyre::Result<Vec<ContainerSummary>> {
        let options = Some(ListContainersOptions::<String> {
            all: true,
            ..Default::default()
        });

        let containers = self.docker.list_containers(options).await?;
        Ok(containers)
    }

    pub async fn start_container(&self, name: &str) -> color_eyre::Result<()> {
        self.docker.start_container::<String>(name, None).await?;
        Ok(())
    }

    pub async fn stop_container(&self, name: &str) -> color_eyre::Result<()> {
        self.docker.stop_container(name, None).await?;
        Ok(())
    }
}