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

    pub async fn get_containers(&self) -> color_eyre::Result<Vec<String>> {
        let options = Some(ListContainersOptions::<String> {
            all: true,
            ..Default::default()
        });

        let containers = self.docker.list_containers(options).await?;
        let names = containers
            .into_iter()
            .filter_map(|c| c.names)
            .filter_map(|n| n.first().cloned())
            .collect();

        Ok(names)
    }
}