fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let Some(rc_exe) = find_rc_exe() else {
        panic!("rc.exe not found");
    };
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let res = out_dir.join("taskbar_audio.res");
    let status = std::process::Command::new(rc_exe)
        .args(["/nologo", "/fo"])
        .arg(&res)
        .arg("asset\\taskbar_audio.rc")
        .status()
        .unwrap();
    if !status.success() {
        panic!("rc.exe failed");
    }
    println!("cargo:rustc-link-arg-bins={}", res.display());
    println!("cargo:rerun-if-changed=asset\\taskbar_audio.rc");
    println!("cargo:rerun-if-changed=asset\\logo.ico");
}

fn find_rc_exe() -> Option<std::path::PathBuf> {
    use winreg::enums::HKEY_LOCAL_MACHINE;
    use winreg::RegKey;

    let arch = match std::env::var("CARGO_CFG_TARGET_ARCH").ok().as_deref() {
        Some("x86_64") => "x64",
        Some("x86") => "x86",
        Some("aarch64") => "arm64",
        _ => "x64",
    };
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let mut roots: Vec<std::path::PathBuf> = Vec::new();
    if let Ok(key) = hklm.open_subkey("SOFTWARE\\Microsoft\\Windows Kits\\Installed Roots") {
        if let Ok(v) = key.get_value::<String, _>("KitsRoot10") {
            roots.push(std::path::PathBuf::from(v));
        }
        if let Ok(v) = key.get_value::<String, _>("KitsRoot81") {
            roots.push(std::path::PathBuf::from(v));
        }
    }
    if let Ok(key) = hklm.open_subkey("SOFTWARE\\WOW6432Node\\Microsoft\\Windows Kits\\Installed Roots") {
        if let Ok(v) = key.get_value::<String, _>("KitsRoot10") {
            roots.push(std::path::PathBuf::from(v));
        }
        if let Ok(v) = key.get_value::<String, _>("KitsRoot81") {
            roots.push(std::path::PathBuf::from(v));
        }
    }
    if let Ok(pfx86) = std::env::var("ProgramFiles(x86)") {
        let base = std::path::PathBuf::from(pfx86);
        roots.push(base.join("Windows Kits").join("10"));
        roots.push(base.join("Windows Kits").join("11"));
        roots.push(base.join("Windows Kits").join("8.1"));
    }
    for root in roots {
        if let Some(p) = find_rc_exe_under(&root, arch) {
            return Some(p);
        }
    }
    None
}

fn find_rc_exe_under(root: &std::path::Path, arch: &str) -> Option<std::path::PathBuf> {
    let direct = root.join("bin").join(arch).join("rc.exe");
    if direct.is_file() {
        return Some(direct);
    }
    let bin_dir = root.join("bin");
    let mut best: Option<std::path::PathBuf> = None;
    let mut best_ver: Option<(u32, u32, u32, u32)> = None;
    let entries = std::fs::read_dir(&bin_dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|v| v.to_str()) else {
            continue;
        };
        let Some(ver) = parse_ver(name) else {
            continue;
        };
        let candidate = path.join(arch).join("rc.exe");
        if !candidate.is_file() {
            continue;
        }
        if best_ver.map(|b| ver > b).unwrap_or(true) {
            best_ver = Some(ver);
            best = Some(candidate);
        }
    }
    best
}

fn parse_ver(s: &str) -> Option<(u32, u32, u32, u32)> {
    let mut parts = s.split('.');
    let a = parts.next()?.parse().ok()?;
    let b = parts.next()?.parse().ok()?;
    let c = parts.next()?.parse().ok()?;
    let d = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((a, b, c, d))
}
