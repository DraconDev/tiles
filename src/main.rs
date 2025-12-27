
fn update_commands(app: &mut App) {
    let mut commands = vec![
        CommandItem { label: "Quit".to_string(), action: CommandAction::Quit },
        CommandItem { label: "Toggle Zoom".to_string(), action: CommandAction::ToggleZoom },
        CommandItem { label: "View: Files".to_string(), action: CommandAction::SwitchView(CurrentView::Files) },
        CommandItem { label: "View: Docker".to_string(), action: CommandAction::SwitchView(CurrentView::Docker) },
        CommandItem { label: "View: System".to_string(), action: CommandAction::SwitchView(CurrentView::System) },
    ];
    
    // Add dynamic commands (Docker containers)
    for name in &app.docker_state.containers {
         commands.push(CommandItem { 
             label: format!("Start Container: {}", name), 
             action: CommandAction::StartContainer(name.clone()) 
         });
         commands.push(CommandItem { 
             label: format!("Stop Container: {}", name), 
             action: CommandAction::StopContainer(name.clone()) 
         });
    }

    app.filtered_commands = commands.into_iter()
        .filter(|cmd| cmd.label.to_lowercase().contains(&app.input.to_lowercase()))
        .collect();
    
    app.command_index = 0;
}

fn execute_command(action: CommandAction, app: &mut App, docker_module: &Option<Arc<DockerModule>>) {
    match action {
        CommandAction::Quit => {
            app.running = false;
        },
        CommandAction::ToggleZoom => app.toggle_zoom(),
        CommandAction::SwitchView(view) => app.current_view = view,
        CommandAction::StartContainer(name) => {
             if let Some(docker) = docker_module {
                let docker = docker.clone();
                tokio::spawn(async move {
                    let _ = docker.start_container(&name).await;
                });
            }
        },
        CommandAction::StopContainer(name) => {
            if let Some(docker) = docker_module {
                let docker = docker.clone();
                tokio::spawn(async move {
                    let _ = docker.stop_container(&name).await;
                });
            }
        },
    }
}
