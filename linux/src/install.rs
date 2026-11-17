//! Putting the program where a desktop finds it: the binary, a launcher entry with its icon, and starting at login.

use std::path::{Path, PathBuf};

const ICON: &[u8] = include_bytes!("../assets/snowdrift.png");

fn home() -> Result<PathBuf, String> {
    std::env::var_os("HOME").map(PathBuf::from).ok_or_else(|| "HOME is not set".to_string())
}

fn data_home() -> Result<PathBuf, String> {
    Ok(std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).filter(|p| p.is_absolute()).unwrap_or(home()?.join(".local/share")))
}

fn autostart_file() -> Result<PathBuf, String> {
    let base = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).filter(|p| p.is_absolute()).unwrap_or(home()?.join(".config"));
    Ok(base.join("autostart/snowdrift.desktop"))
}

/// The launcher entry; its actions show up in the context menu of the launcher or dock.
pub fn desktop_entry(exec: &str) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName=Snowdrift\nName[ru]=Снегопад\nComment=Snowfall over all your windows\nComment[ru]=Снегопад поверх всех окон\n\
Exec={exec} run\nIcon=snowdrift\nCategories=Utility;\nKeywords=snow;winter;snowfall;снег;\nStartupNotify=false\nTerminal=false\n\
Actions=plow;clear;stop;\n\n\
[Desktop Action plow]\nName=Call the snowplow\nName[ru]=Вызвать снегоуборщик\nExec={exec} plow\n\n\
[Desktop Action clear]\nName=Blow the snow away\nName[ru]=Сдуть снег\nExec={exec} clear\n\n\
[Desktop Action stop]\nName=Quit\nName[ru]=Выйти\nExec={exec} stop\n"
    )
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
}

/// Copies this program to `~/.local/bin`, adds the launcher entry and icon. Returns where the program went.
pub fn install() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let target = home()?.join(".local/bin/snowdrift");
    if exe != target {
        // Copied through a temporary name so a running copy can be replaced.
        let staging = target.with_extension("new");
        if let Some(parent) = staging.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::copy(&exe, &staging).map_err(|e| format!("copying: {e}"))?;
        std::fs::rename(&staging, &target).map_err(|e| format!("placing: {e}"))?;
    }
    let data = data_home()?;
    write(&data.join("applications/snowdrift.desktop"), desktop_entry(&target.display().to_string()).as_bytes())?;
    write(&data.join("icons/hicolor/256x256/apps/snowdrift.png"), ICON)?;
    refresh_caches(&data);
    Ok(target)
}

pub fn uninstall() -> Result<(), String> {
    let data = data_home()?;
    for path in [
        home()?.join(".local/bin/snowdrift"),
        data.join("applications/snowdrift.desktop"),
        data.join("icons/hicolor/256x256/apps/snowdrift.png"),
        autostart_file()?,
    ] {
        let _ = std::fs::remove_file(path);
    }
    refresh_caches(&data);
    Ok(())
}

fn refresh_caches(data: &Path) {
    let _ = std::process::Command::new("update-desktop-database").arg(data.join("applications")).output();
    let _ = std::process::Command::new("gtk-update-icon-cache").arg("-f").arg("-t").arg(data.join("icons/hicolor")).output();
}

pub fn autostart(on: bool) -> Result<(), String> {
    let file = autostart_file()?;
    if on {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let entry = format!(
            "[Desktop Entry]\nType=Application\nName=Snowdrift\nExec={} run\nIcon=snowdrift\nX-GNOME-Autostart-enabled=true\nTerminal=false\n",
            exe.display()
        );
        write(&file, entry.as_bytes())
    } else {
        let _ = std::fs::remove_file(file);
        Ok(())
    }
}
