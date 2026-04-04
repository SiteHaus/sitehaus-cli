use crate::config::ServerConfig;
use std::process::Command;

pub fn ssh_exec(server: &ServerConfig, remote_cmd: &str) -> i32 {
    let mut args = vec![
        "-t".to_string(),
        "-o".to_string(),
        "LogLevel=ERROR".to_string(),
    ];

    if let Some(key) = &server.ssh_key_path {
        args.push("-i".to_string());
        args.push(key.clone());
    }

    args.push(format!("{}@{}", server.ssh_user, server.host));
    args.push(remote_cmd.to_string());

    Command::new("ssh")
        .args(&args)
        .status()
        .map(|s| s.code().unwrap_or(1))
        .unwrap_or(1)
}
