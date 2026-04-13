use anyhow::{Result, bail};
use dialoguer::{Confirm, Input, theme::ColorfulTheme};

/// Simple y/N confirmation. Returns Ok(()) if confirmed, bails if not.
pub fn confirm(prompt: &str) -> Result<()> {
    let confirmed = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(prompt)
        .default(false)
        .interact()?;

    if !confirmed {
        bail!("Aborted.");
    }
    Ok(())
}

/// For prod — requires the user to type the server name exactly.
pub fn confirm_prod(server_name: &str) -> Result<()> {
    eprintln!();
    eprintln!("  ⚠️  You are targeting \"{server_name}\" (production).");
    eprintln!("  This is a destructive operation.");
    eprintln!();

    let input: String = Input::with_theme(&ColorfulTheme::default())
        .with_prompt(format!("Type \"{server_name}\" to confirm"))
        .interact_text()?;

    if input.trim() != server_name {
        bail!("Aborted — server name did not match.");
    }
    Ok(())
}

pub fn is_prod(server_name: &str) -> bool {
    server_name.contains("prod")
}
