use crate::config::{ServerConfig, ServerType, read_config, write_config};
use crate::theme;
use anyhow::Result;
use dialoguer::{Confirm, Input, Select, theme::ColorfulTheme};
use std::process::Command;

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

        // 7. Local repo path
        let local_path_input: String = Input::with_theme(&theme)
            .with_prompt("Local repo path (where you cloned this project)")
            .default(format!(
                "{}/Dev/sitehaus-commerce",
                dirs::home_dir().unwrap().display()
            ))
            .interact_text()?;
        let local_path = if local_path_input.trim().is_empty() {
            None
        } else {
            Some(local_path_input.trim().to_string())
        };

        // 8. SSH key path
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

        // 8. Test key auth — offer ssh-copy-id if it fails
        let key = ssh_key_path.as_deref().unwrap_or("");
        let key_works = Command::new("ssh")
            .args([
                "-o",
                "BatchMode=yes",
                "-o",
                "ConnectTimeout=5",
                "-i",
                key,
                &format!("{}@{}", ssh_user, host),
                "true",
            ])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        if !key_works {
            theme::warn("Key auth failed — server is still using password auth.");
            let copy = Confirm::with_theme(&theme)
                .with_prompt(format!("Copy {} to {}@{}?", key, ssh_user, host))
                .default(true)
                .interact()?;

            if copy {
                let pub_key = format!("{}.pub", key);
                let status = Command::new("ssh-copy-id")
                    .args(["-i", &pub_key, &format!("{}@{}", ssh_user, host)])
                    .status()?;

                if status.success() {
                    theme::success("Key copied — password auth no longer needed.");
                } else {
                    theme::warn(
                        "ssh-copy-id failed. You can do it manually: ssh-copy-id -i {pub_key} {ssh_user}@{host}",
                    );
                }
            }
        } else {
            theme::success("Key auth confirmed.");
        }

        config.servers.insert(
            name.clone(),
            ServerConfig {
                server_type,
                host,
                ssh_user,
                ssh_key_path,
                repo_path,
                health_url,
                local_path,
            },
        );

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
        theme::success(&format!(
            "Active server set to \"{}\".",
            theme::yellow(&name)
        ));
    } else if config.servers.len() > 1 {
        let names: Vec<String> = config.servers.keys().cloned().collect();
        let idx = Select::with_theme(&theme)
            .with_prompt("Which server should be active?")
            .items(&names)
            .default(0)
            .interact()?;
        config.active_server = Some(names[idx].clone());
        theme::success(&format!(
            "Active server set to \"{}\".",
            theme::yellow(&names[idx])
        ));
    }

    write_config(&config)?;
    println!();
    println!("  Config written to ~/.sitehaus/config.yml");
    println!("  Run {} to verify.", theme::yellow("sitehaus status"));
    println!();

    Ok(())
}
