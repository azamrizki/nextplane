//! `nextplane update`: download a release, check its SHA-256 against the release's
//! `SHA256SUMS`, replace this binary (the old one stays as `nextplane.old`) and restart the
//! service if it is installed.

use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};

pub fn repo() -> String {
    std::env::var("NEXTPLANE_UPDATE_REPO").unwrap_or_else(|_| "azamrizki/nextplane".into())
}

pub fn asset_name() -> Result<String> {
    let os = match std::env::consts::OS {
        "linux" => "linux",
        "macos" => "darwin",
        o => bail!("no release builds for {o}"),
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        a => bail!("no release builds for {a}"),
    };
    Ok(format!("nextplane-{os}-{arch}.tar.gz"))
}

/// Finds `name` in a `SHA256SUMS` file (`<hex>  <name>` lines).
pub fn expected_sum(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|l| {
        let mut it = l.split_whitespace();
        let (h, n) = (it.next()?, it.next()?);
        (n.trim_start_matches('*') == name).then(|| h.to_lowercase())
    })
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The `nextplane` binary inside a release archive.
pub fn binary_from_archive(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut a = tar::Archive::new(flate2::read::GzDecoder::new(bytes));
    for e in a.entries()? {
        let mut e = e?;
        if e.path()?.file_name().and_then(|n| n.to_str()) == Some("nextplane") {
            let mut out = Vec::new();
            e.read_to_end(&mut out)?;
            return Ok(out);
        }
    }
    bail!("the release archive has no nextplane binary")
}

/// Puts `new` in place of `target`, keeping the previous binary as `<target>.old`.
pub fn swap_binary(target: &Path, new: &[u8]) -> Result<PathBuf> {
    let tmp = target.with_extension("new");
    std::fs::write(&tmp, new).with_context(|| format!("write {}", tmp.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))?;
    }
    let old = target.with_extension("old");
    if target.exists() {
        std::fs::rename(target, &old).with_context(|| format!("keep {}", old.display()))?;
    }
    std::fs::rename(&tmp, target)?;
    Ok(old)
}

pub async fn run(version: Option<String>, check_only: bool) -> Result<()> {
    let client = reqwest::Client::builder()
        .user_agent("nextplane-updater")
        .build()?;
    let repo = repo();
    let api = match &version {
        Some(v) => format!("https://api.github.com/repos/{repo}/releases/tags/{v}"),
        None => format!("https://api.github.com/repos/{repo}/releases/latest"),
    };
    let rel: serde_json::Value = client
        .get(&api)
        .send()
        .await?
        .error_for_status()
        .context("find the release")?
        .json()
        .await?;
    let tag = rel["tag_name"].as_str().unwrap_or("?").to_string();
    let current = format!("v{}", env!("CARGO_PKG_VERSION"));
    println!("installed {current}, release {tag}");
    if check_only {
        return Ok(());
    }
    if version.is_none() && tag == current {
        println!("already up to date");
        return Ok(());
    }
    let name = asset_name()?;
    let url_of = |n: &str| -> Option<String> {
        rel["assets"].as_array()?.iter().find(|a| a["name"] == n)?["browser_download_url"]
            .as_str()
            .map(String::from)
    };
    let (Some(asset), Some(sums)) = (url_of(&name), url_of("SHA256SUMS")) else {
        bail!("release {tag} has no {name} or SHA256SUMS");
    };
    let sums = client
        .get(sums)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    let want =
        expected_sum(&sums, &name).with_context(|| format!("{name} is not in SHA256SUMS"))?;
    let bytes = client
        .get(asset)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    let got = sha256_hex(&bytes);
    if got != want {
        bail!("checksum mismatch for {name}: expected {want}, got {got}; nothing was changed");
    }
    let bin = binary_from_archive(&bytes)?;
    let target = std::env::current_exe()?;
    let old = swap_binary(&target, &bin)?;
    println!(
        "updated {} to {tag} (previous binary: {})",
        target.display(),
        old.display()
    );
    if !cfg!(target_os = "macos") {
        let st = std::process::Command::new("systemctl")
            .args(["--user", "try-restart", crate::service::UNIT])
            .status();
        if st.is_ok_and(|s| s.success()) {
            println!("service restarted");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sums_archive_and_swap() {
        let sums = "abc123  nextplane-linux-amd64.tar.gz\nDEF456 *nextplane-darwin-arm64.tar.gz\n";
        assert_eq!(
            expected_sum(sums, "nextplane-darwin-arm64.tar.gz").unwrap(),
            "def456"
        );
        assert!(expected_sum(sums, "other").is_none());
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );

        let mut buf = Vec::new();
        {
            let mut t = tar::Builder::new(flate2::write::GzEncoder::new(
                &mut buf,
                flate2::Compression::fast(),
            ));
            let mut h = tar::Header::new_gnu();
            h.set_size(4);
            h.set_mode(0o755);
            h.set_cksum();
            t.append_data(&mut h, "nextplane-linux-amd64/nextplane", &b"BIN!"[..])
                .unwrap();
            t.into_inner().unwrap().finish().unwrap();
        }
        assert_eq!(binary_from_archive(&buf).unwrap(), b"BIN!");

        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("nextplane");
        std::fs::write(&target, b"OLD").unwrap();
        let old = swap_binary(&target, b"NEW").unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"NEW");
        assert_eq!(std::fs::read(old).unwrap(), b"OLD");
    }
}
