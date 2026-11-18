//! `snowdrift update`: asks GitHub for the latest release, downloads it for this machine, puts it in place and restarts.
//! Uses `curl` and `tar`, which the one-line installer needs as well.

use std::path::{Path, PathBuf};
use std::process::Command;

use snowdrift_core::version::{field, newer};

use crate::control;

const REPOSITORY: &str = "LerkOFF/snowdrift";

fn run(program: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new(program).args(args).output().map_err(|error| format!("can't run {program}: {error}"))?;
    if !output.status.success() {
        let text = String::from_utf8_lossy(&output.stderr);
        return Err(format!("{program} failed: {}", text.trim()));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Updates this copy to the latest release. Returns what to tell the user.
pub fn update(version: &str) -> Result<String, String> {
    let home = std::env::var_os("HOME").map(PathBuf::from).ok_or("HOME is not set")?;
    let installed = home.join(".local/bin/snowdrift");
    let exe = std::env::current_exe().map_err(|error| error.to_string())?;
    if exe != installed {
        return Err(format!(
            "this copy ({}) wasn't put there by `snowdrift install`; update it the way you installed it (package manager or the install script)",
            exe.display()
        ));
    }

    let body = run("curl", &["-fsSL", "--max-time", "20", "-H", "Accept: application/vnd.github+json", &format!("https://api.github.com/repos/{REPOSITORY}/releases/latest")])?;
    let tag = field(&body, "tag_name").ok_or("the latest release has no tag")?;
    let latest = tag.trim_start_matches('v');
    if !newer(latest, version) {
        return Ok(format!("You're up to date: Snowdrift {version} is the latest version."));
    }

    let arch = match std::env::consts::ARCH {
        "x86_64" => "x86_64",
        "aarch64" => "aarch64",
        other => return Err(format!("there are no Linux builds for {other}")),
    };
    println!("Snowdrift {latest} is available (you have {version}). Downloading...");
    let work = std::env::temp_dir().join(format!("snowdrift-update-{}", std::process::id()));
    std::fs::create_dir_all(&work).map_err(|error| format!("{}: {error}", work.display()))?;
    let result = install_from(&work, latest, arch, &installed);
    let _ = std::fs::remove_dir_all(&work);
    result?;
    Ok(format!("Updated Snowdrift {version} to {latest}."))
}

fn install_from(work: &Path, latest: &str, arch: &str, installed: &Path) -> Result<(), String> {
    let archive = work.join("snowdrift.tar.gz");
    let url = format!("https://github.com/{REPOSITORY}/releases/download/v{latest}/Snowdrift-linux-{arch}.tar.gz");
    run("curl", &["-fsSL", "--max-time", "300", "-o", &archive.display().to_string(), &url])?;
    run("tar", &["-xzf", &archive.display().to_string(), "-C", &work.display().to_string()])?;

    let fresh = std::fs::read_dir(work)
        .map_err(|error| error.to_string())?
        .flatten()
        .map(|entry| entry.path().join("snowdrift"))
        .find(|path| path.is_file())
        .ok_or("the download has no snowdrift program")?;
    let reported = run(&fresh.display().to_string(), &["--version"])?;
    if reported.trim() != format!("Snowdrift {latest}") {
        return Err(format!("the download is {}, not Snowdrift {latest}", reported.trim()));
    }

    let was_running = control::send("ping").is_some();
    if was_running {
        let _ = control::send("stop");
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    run(&fresh.display().to_string(), &["install"])?;
    if was_running {
        // The old copy's autostart and launcher entries were rewritten by `install`; start the new program.
        Command::new(installed).arg("start").status().map_err(|error| format!("can't restart: {error}"))?;
    }
    Ok(())
}
