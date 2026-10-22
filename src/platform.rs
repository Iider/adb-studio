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
            [root.join("bin/adb"), root.join("data/bin/adb")]
                .into_iter()
                .chain(cfg!(target_os = "macos").then(|| root.join("../platform-tools/adb")))
                .find(|path| path.is_file())
                .unwrap_or_else(|| PathBuf::from("adb"))
        })
}

pub fn local_ocr(root: &Path, data: &Path) -> Option<PathBuf> {
    [data.join("bin/ocr"), root.join("bin/ocr")]
        .into_iter()
        .find(|p| p.is_file())
}

pub fn has_local_ocr(root: &Path, data: &Path) -> bool {
    local_ocr(root, data).is_some()
        || (cfg!(target_os = "macos") && root.join("scripts/ocr.swift").is_file())
}

pub fn autostart() -> bool {
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
