//! Desktop integration on Linux (not retail): the client's copy in the data dir, the icon and the .desktop entry.
//! Wayland ignores window icons; the compositor finds ours through the app id ("vana-client") and an entry of that
//! name, so the entry is always written, hidden from menus (NoDisplay) unless a menu entry was asked for.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

const ICON_512: &[u8] = include_bytes!("../../../../assets/vana-client-512.png");
const ICON_256: &[u8] = include_bytes!("../../assets/icon-256.png");

/// Whether this platform gets the menu / desktop questions.
pub fn supported() -> bool {
    cfg!(target_os = "linux")
}

fn data_home() -> Result<PathBuf> {
    dirs::data_dir().context("no data directory")
}

/// The client's own copy, which the entries start.
fn home_exe() -> PathBuf {
    crate::config::data_dir().join("vana-client")
}

/// At every start: with no copy in the data dir yet, this binary is put there and the entries point at it (the menu
/// keeps an entry's shown / hidden; shown when there was none, as the first-start page's default).
pub fn settle() -> Result<()> {
    let home = home_exe();
    if home.exists() {
        return Ok(());
    }
    let exe = super::updater::exe().context("no executable path")?;
    std::fs::create_dir_all(home.parent().context("no data directory")?)?;
    let part = home.with_extension("part");
    std::fs::copy(&exe, &part)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&part, std::fs::Permissions::from_mode(0o755))?;
    }
    std::fs::rename(&part, &home)?;
    let menu = std::fs::read_to_string(data_home()?.join("applications/vana-client.desktop")).map_or(true, |t| !t.contains("NoDisplay=true"));
    install(menu)?;
    if dirs::desktop_dir().is_some_and(|d| d.join("vana-client.desktop").exists()) {
        desktop_shortcut()?;
    }
    log::info!("installed {} from {}", home.display(), exe.display());
    Ok(())
}

/// A path as a quoted Exec argument (the desktop entry spec's escapes: `\\\\` is one backslash).
fn quoted(p: &Path) -> String {
    let mut s = String::from("\"");
    for c in p.to_string_lossy().chars() {
        match c {
            '"' | '`' | '$' => s.extend(['\\', c]),
            '\\' => s.push_str("\\\\\\\\"),
            '%' => s.push_str("%%"),
            _ => s.push(c),
        }
    }
    s.push('"');
    s
}

fn entry(menu: bool) -> Result<String> {
    let home = home_exe();
    let exe = if home.is_file() { home } else { std::env::current_exe()? };
    Ok(format!(
        "[Desktop Entry]\nType=Application\nName=vana-client\nGenericName=FINAL FANTASY XI client\nComment=Native FINAL FANTASY XI client \
         for LandSandBoat servers\nExec={}\nIcon=vana-client\nTerminal=false\nCategories=Game;RolePlaying;\nStartupWMClass=vana-client\n{}",
        quoted(&exe),
        if menu { "" } else { "NoDisplay=true\n" }
    ))
}

/// The icons (256 and 512) and the application entry, shown in menus when `menu`.
pub fn install(menu: bool) -> Result<()> {
    let data = data_home()?;
    for (size, png) in [(256, ICON_256), (512, ICON_512)] {
        let dir = data.join(format!("icons/hicolor/{size}x{size}/apps"));
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join("vana-client.png"), png)?;
    }
    let apps = data.join("applications");
    std::fs::create_dir_all(&apps)?;
    std::fs::write(apps.join("vana-client.desktop"), entry(menu)?)?;
    Ok(())
}

/// A launcher on the desktop: executable, and trusted where GNOME's gio is there to mark it.
pub fn desktop_shortcut() -> Result<()> {
    let dir = dirs::desktop_dir().context("no desktop directory")?;
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("vana-client.desktop");
    std::fs::write(&path, entry(true)?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;
    }
    let _ = std::process::Command::new("gio").arg("set").arg(&path).arg("metadata::trusted").arg("true").output();
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn hidden_unless_in_the_menu() {
        let shown = super::entry(true).unwrap();
        let hidden = super::entry(false).unwrap();
        assert!(shown.contains("StartupWMClass=vana-client") && !shown.contains("NoDisplay"));
        assert!(hidden.ends_with("NoDisplay=true\n"));
        assert!(shown.contains("Exec=\""));
    }

    #[test]
    fn exec_escapes() {
        let q = super::quoted(std::path::Path::new("/home/a b/$x\\y/100%/\"z\""));
        assert_eq!(q, r#""/home/a b/\$x\\\\y/100%%/\"z\"""#);
    }
}
