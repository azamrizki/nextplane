//! `nextplane install | uninstall | status | logs`: a per-user service, like NextBoard
//! (systemd user unit on Linux, LaunchAgent on macOS). `--dry-run` prints every step.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

pub const UNIT: &str = "nextplane.service";
pub const LABEL: &str = "id.nextplane";

pub struct Plan {
    pub binary: PathBuf,
    pub service_file: PathBuf,
    pub service_text: String,
    pub commands: Vec<Vec<String>>,
}

fn home() -> Result<PathBuf> {
    Ok(PathBuf::from(
        std::env::var_os("HOME").context("HOME is not set")?,
    ))
}

pub fn systemd_unit(binary: &Path, data: &Path) -> String {
    format!(
        "[Unit]\nDescription=NextPlane project management\nAfter=network.target\n\n\
         [Service]\nExecStart={} --data {} serve\nRestart=on-failure\nRestartSec=3\n\n\
         [Install]\nWantedBy=default.target\n",
        binary.display(),
        data.display()
    )
}

pub fn launch_agent(binary: &Path, data: &Path) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{LABEL}</string>
  <key>ProgramArguments</key>
  <array><string>{}</string><string>--data</string><string>{}</string><string>serve</string></array>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><true/>
  <key>StandardErrorPath</key><string>{}/logs/nextplane.log</string>
</dict>
</plist>
"#,
        binary.display(),
        data.display(),
        data.display()
    )
}

pub fn plan(home: &Path, data: &Path, macos: bool) -> Plan {
    let binary = home.join(".local/bin/nextplane");
    if macos {
        let f = home.join(format!("Library/LaunchAgents/{LABEL}.plist"));
        Plan {
            service_text: launch_agent(&binary, data),
            commands: vec![vec![
                "launchctl".into(),
                "load".into(),
                "-w".into(),
                f.display().to_string(),
            ]],
            service_file: f,
            binary,
        }
    } else {
        Plan {
            service_text: systemd_unit(&binary, data),
            commands: vec![
                vec!["systemctl".into(), "--user".into(), "daemon-reload".into()],
                vec![
                    "systemctl".into(),
                    "--user".into(),
                    "enable".into(),
                    "--now".into(),
                    UNIT.into(),
                ],
            ],
            service_file: home.join(".config/systemd/user").join(UNIT),
            binary,
        }
    }
}

fn run(cmd: &[String], dry: bool) -> Result<()> {
    println!("  $ {}", cmd.join(" "));
    if dry {
        return Ok(());
    }
    let st = Command::new(&cmd[0])
        .args(&cmd[1..])
        .status()
        .with_context(|| format!("run {}", cmd[0]))?;
    if !st.success() {
        bail!("{} failed ({st})", cmd.join(" "));
    }
    Ok(())
}

pub fn install(data: &Path, dry: bool, no_start: bool) -> Result<()> {
    let home = home()?;
    let p = plan(&home, data, cfg!(target_os = "macos"));
    let me = std::env::current_exe()?;
    println!(
        "Install NextPlane as a user service{}:",
        if dry { " (dry run)" } else { "" }
    );
    if me != p.binary {
        println!("  copy {} → {}", me.display(), p.binary.display());
        if !dry {
            std::fs::create_dir_all(p.binary.parent().unwrap())?;
            let tmp = p.binary.with_extension("new");
            std::fs::copy(&me, &tmp)?;
            std::fs::rename(&tmp, &p.binary)?;
        }
    }
    println!("  write {}", p.service_file.display());
    if !dry {
        std::fs::create_dir_all(p.service_file.parent().unwrap())?;
        std::fs::write(&p.service_file, &p.service_text)?;
        std::fs::create_dir_all(data.join("logs"))?;
    }
    if no_start {
        println!("  (not started: --no-start)");
        return Ok(());
    }
    for c in &p.commands {
        run(c, dry)?;
    }
    if !dry {
        println!(
            "\nInstalled. If this is the first start, the setup link is in the log: nextplane logs"
        );
    }
    Ok(())
}

pub fn uninstall(data: &Path, dry: bool) -> Result<()> {
    let home = home()?;
    let p = plan(&home, data, cfg!(target_os = "macos"));
    println!(
        "Remove the NextPlane service{}:",
        if dry { " (dry run)" } else { "" }
    );
    if cfg!(target_os = "macos") {
        let _ = run(
            &[
                "launchctl".into(),
                "unload".into(),
                "-w".into(),
                p.service_file.display().to_string(),
            ],
            dry,
        );
    } else {
        let _ = run(
            &[
                "systemctl".into(),
                "--user".into(),
                "disable".into(),
                "--now".into(),
                UNIT.into(),
            ],
            dry,
        );
    }
    if p.service_file.exists() {
        println!("  remove {}", p.service_file.display());
        if !dry {
            std::fs::remove_file(&p.service_file)?;
        }
    }
    println!(
        "The data folder {} and the binary stay where they are.",
        data.display()
    );
    Ok(())
}

pub async fn status(listen: std::net::SocketAddr) -> Result<()> {
    let url = format!("{}/api/v1/health", crate::local_url(listen));
    match reqwest::Client::new()
        .get(&url)
        .timeout(std::time::Duration::from_secs(3))
        .send()
        .await
    {
        Ok(r) if r.status().is_success() => {
            let v: serde_json::Value = r.json().await.unwrap_or_default();
            println!(
                "running on {} (schema {})",
                crate::local_url(listen),
                v["schema_version"]
            );
            Ok(())
        }
        Ok(r) => bail!(
            "answering on {} but unhealthy: {}",
            crate::local_url(listen),
            r.status()
        ),
        Err(_) => bail!("not running on {}", crate::local_url(listen)),
    }
}

pub fn logs(follow: bool, data: &Path) -> Result<()> {
    if cfg!(target_os = "macos") {
        let f = data.join("logs/nextplane.log");
        let mut c = Command::new("tail");
        c.arg("-n").arg("200");
        if follow {
            c.arg("-f");
        }
        c.arg(f).status()?;
    } else {
        let mut c = Command::new("journalctl");
        c.args(["--user", "-u", UNIT, "-n", "200", "--no-pager"]);
        if follow {
            c.arg("-f");
        }
        c.status()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plans() {
        let p = plan(Path::new("/home/u"), Path::new("/home/u/.nextplane"), false);
        assert_eq!(
            p.service_file,
            Path::new("/home/u/.config/systemd/user/nextplane.service")
        );
        assert!(
            p.service_text
                .contains("ExecStart=/home/u/.local/bin/nextplane --data /home/u/.nextplane serve")
        );
        let m = plan(
            Path::new("/Users/u"),
            Path::new("/Users/u/.nextplane"),
            true,
        );
        assert!(
            m.service_file
                .ends_with("Library/LaunchAgents/id.nextplane.plist")
        );
        assert!(m.service_text.contains("<string>serve</string>"));
    }
}
