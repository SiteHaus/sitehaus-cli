use crate::config::{ServerConfig, ServerType, write_config, read_config};
use crate::theme;
use anyhow::Result;
use dialoguer::{Input, Select, Confirm, theme::ColorfulTheme};

pub fn run() -> Result<()> {
    let theme = ColorfulTheme::default();
    let mut config = read_config().unwrap_or_default();

    println!();
    println!("  {} — first-run setup", theme::yellow("sitehaus"));
    println!();

    loop {
        // 1. Server name
        let name: String = Input::with_theme(&theme)
            .with_prompt("Server name (e.g. commerce-staging)")
            .interact_text()?;

        // 2. Type
        let types = &["ecom", "platform"];
        let type_idx = Select::with_theme(&theme)
            .with_prompt("Server type")
            .items(types)
            .default(0)
            .interact()?;
        let server_type = match type_idx {
            0 => ServerType::Ecom,
            _ => ServerType::Platform,
        };

        // 3. Host
        let host: String = Input::with_theme(&theme)
            .with_prompt("Host IP or domain")
            .interact_text()?;

        // 4. Repo path
        let repo_path: String = Input::with_theme(&theme)
            .with_prompt("Repo path on server")
            .default("/srv/sitehaus-commerce".to_string())
            .interact_text()?;

        // 5. Health URL
        let health_url: String = Input::with_theme(&theme)
            .with_prompt("Health check URL")
            .interact_text()?;

        // 6. SSH user
        let ssh_user: String = Input::with_theme(&theme)
            .with_prompt("SSH user")
            .default("deploy".to_string())
            .interact_text()?;

        // 7. SSH key path
        let ssh_key_input: String = Input::with_theme(&theme)
            .with_prompt("SSH key path (leave blank for default ~/.ssh/id_ed25519)")
            .allow_empty(true)
            .interact_text()?;
        let ssh_key_path = if ssh_key_input.trim().is_empty() {
            let home = dirs::home_dir().expect("could not determine home directory");
            Some(home.join(".ssh/id_ed25519").to_string_lossy().into_owned())
        } else {
            Some(ssh_key_input.trim().to_string())
        };

        config.servers.insert(name.clone(), ServerConfig {
            server_type,
            host,
            ssh_user,
            ssh_key_path,
            repo_path,
            health_url,
        });

        theme::success(&format!("Server \"{}\" added.", theme::yellow(&name)));
        println!();

        // 8. Add another?
        let another = Confirm::with_theme(&theme)
            .with_prompt("Add another server?")
            .default(false)
            .interact()?;

        if !another {
            break;
        }
        println!();
    }

    // 9. Set active server
    if config.servers.len() == 1 {
        let name = config.servers.keys().next().unwrap().clone();
        config.active_server = Some(name.clone());
        theme::success(&format!("Active server set to \"{}\".", theme::yellow(&name)));
    } else if config.servers.len() > 1 {
        let names: Vec<String> = config.servers.keys().cloned().collect();
        let idx = Select::with_theme(&theme)
            .with_prompt("Which server should be active?")
            .items(&names)
            .default(0)
            .interact()?;
        config.active_server = Some(names[idx].clone());
        theme::success(&format!("Active server set to \"{}\".", theme::yellow(&names[idx])));
    }

    write_config(&config)?;
    println!();
    println!("  Config written to ~/.sitehaus/config.yml");
    println!("  Run {} to verify.", theme::yellow("sitehaus status"));
    println!();

    Ok(())
}
