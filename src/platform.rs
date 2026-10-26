use std::path::{Path, PathBuf};

pub fn root() -> PathBuf {
    std::env::var_os("ADB_STUDIO_ROOT")
        .map(PathBuf::from)
        .or_else(|| {
            let executable = std::env::current_exe().ok()?;
            let folder = executable.parent()?;
            folder
                .join("web/build/index.html")
                .is_file()
                .then(|| folder.to_owned())
        })
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")))
}

pub fn adb(root: &Path) -> PathBuf {
    std::env::var_os("ADB_STUDIO_ADB")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let name = if cfg!(windows) { "adb.exe" } else { "adb" };
            [
                root.join("bin").join(name),
                root.join("data/bin").join(name),
            ]
            .into_iter()
            .chain(cfg!(windows).then(|| root.join("data/bin/platform-tools/adb.exe")))
            .chain(cfg!(target_os = "macos").then(|| root.join("../platform-tools/adb")))
            .find(|path| path.is_file())
            .unwrap_or_else(|| PathBuf::from("adb"))
        })
}

pub fn local_ocr(root: &Path, data: &Path) -> Option<PathBuf> {
    #[cfg(windows)]
    for folder in [data.join("bin"), root.join("bin")] {
        for name in ["ocr.exe", "ocr.py"] {
            let path = folder.join(name);
            if path.is_file() {
                return Some(path);
            }
        }
    }
    [data.join("bin/ocr"), root.join("bin/ocr")]
        .into_iter()
        .find(|p| p.is_file())
}

pub fn has_local_ocr(root: &Path, data: &Path) -> bool {
    local_ocr(root, data).is_some()
        || (cfg!(target_os = "macos") && root.join("scripts/ocr.swift").is_file())
}

pub fn autostart() -> bool {
    if cfg!(windows) {
        return std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .is_some_and(|p| {
                p.join("Microsoft/Windows/Start Menu/Programs/Startup/ADB Studio.lnk")
                    .is_file()
            });
    }
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return false;
    };
    if cfg!(target_os = "macos") {
        home.join("Library/LaunchAgents/local.adb-studio.plist")
            .is_file()
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config"))
            .join("systemd/user/adb-studio.service")
            .is_file()
    }
}

pub fn command(path: impl AsRef<Path>) -> tokio::process::Command {
    let path = path.as_ref();
    #[cfg(windows)]
    {
        let mut command = if path.extension().is_some_and(|e| e == "py") {
            let mut command = tokio::process::Command::new(
                std::env::var_os("ADB_STUDIO_PYTHON").unwrap_or_else(|| "python".into()),
            );
            command.arg(path).env("PYTHONUTF8", "1");
            command
        } else {
            tokio::process::Command::new(path)
        };
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW：ADB 轮询不弹出控制台。
        command
    }
    #[cfg(not(windows))]
    tokio::process::Command::new(path)
}

#[cfg(windows)]
pub async fn private_directory(path: &Path) -> anyhow::Result<()> {
    let output = command("whoami")
        .args(["/user", "/fo", "csv", "/nh"])
        .output()
        .await?;
    anyhow::ensure!(output.status.success(), "无法读取当前 Windows 用户");
    let text = String::from_utf8_lossy(&output.stdout);
    let sid = text
        .split('"')
        .find(|s| s.starts_with("S-1-"))
        .ok_or_else(|| anyhow::anyhow!("无法读取 Windows 用户编号"))?;
    let output = command("icacls")
        .arg(path)
        .args(["/inheritance:r", "/grant:r", &format!("*{sid}:(OI)(CI)F")])
        .output()
        .await?;
    anyhow::ensure!(output.status.success(), "无法限制数据目录访问权限");
    Ok(())
}
