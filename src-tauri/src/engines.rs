use crate::{
    ProbeState,
    process::{Control, PrivateDir, Result, Running, command},
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, DirBuilder, File, Permissions},
    io::{self, Read},
    os::unix::fs::{DirBuilderExt, PermissionsExt},
    path::{Path, PathBuf},
    process::Stdio,
    sync::Mutex,
    time::Duration,
};
use tauri::{AppHandle, Manager, path::BaseDirectory};
use uuid::Uuid;

const MAX_ARCHIVE: u64 = 64 * 1024 * 1024;
const MAX_BINARY: u64 = 128 * 1024 * 1024;
const RECHECK_AFTER: i64 = 60;
const SHOW_CACHED_FOR: i64 = 3 * 24 * 60 * 60;

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Engine {
    Cfst,
    Xray,
}

impl Engine {
    fn info(self) -> (&'static str, &'static str, &'static str, &'static str) {
        match self {
            Self::Cfst => (
                "cfst",
                "speedtest/cfst/cfst",
                "XIU2/CloudflareSpeedTest",
                "cfst_darwin_arm64.zip",
            ),
            Self::Xray => (
                "xray",
                "speedtest/xray",
                "XTLS/Xray-core",
                "Xray-macos-arm64-v8a.zip",
            ),
        }
    }

    fn read_version(self, binary: &Path) -> Result<Version> {
        // CFST's -v makes a network request; -h prints the version offline.
        let (arg, prefix) = match self {
            Self::Cfst => ("-h", "CloudflareSpeedTest "),
            Self::Xray => ("version", "Xray "),
        };
        let control = Control::default();
        let (status, output) =
            Running::start(command(binary).arg(arg).stdout(Stdio::piped()), &control)?
                .output(&control, Duration::from_secs(5))?;
        if status.success()
            && let Some(version) = output.lines().find_map(|line| line.strip_prefix(prefix))
            && let Some(version) = version
                .split_whitespace()
                .next()
                .and_then(|s| parse_version(s.trim_start_matches('v')))
        {
            return Ok(version);
        }
        Err(format!("无法读取 {} 的版本", self.info().0).into())
    }

    fn release(self) -> Result<Release> {
        let (_, _, repo, _) = self.info();
        let mut body = Vec::new();
        fetch(
            &format!("https://api.github.com/repos/{repo}/releases/latest"),
            false,
            &mut body,
            2 * 1024 * 1024,
        )?;
        Release::parse(self, &body)
    }
}

type Version = [u32; 3];

fn parse_version(raw: &str) -> Option<Version> {
    let mut parts = raw.split('.');
    let mut version = [0; 3];
    for part in &mut version {
        let value = parts.next()?;
        *part = value.parse().ok()?;
        if part.to_string() != value {
            return None;
        }
    }
    parts.next().is_none().then_some(version)
}

fn tag([major, minor, patch]: Version) -> String {
    format!("v{major}.{minor}.{patch}")
}

#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    digest: Option<String>,
    size: u64,
}

struct Release {
    version: Version,
    digest: String,
    size: u64,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
struct CheckedVersion {
    version: Version,
    checked_at: i64,
}

impl CheckedVersion {
    fn younger_than(self, seconds: i64) -> bool {
        (0..seconds).contains(&Utc::now().timestamp().saturating_sub(self.checked_at))
    }

    fn read(path: &Path) -> Option<Self> {
        let meta = fs::symlink_metadata(path).ok()?;
        if !meta.is_file() || meta.len() > 1024 {
            return None;
        }
        serde_json::from_reader(File::open(path).ok()?.take(1024)).ok()
    }

    fn write(self, path: &Path) -> Result<()> {
        let dir = path.parent().ok_or("组件缓存路径无效")?;
        DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
        let stage = PrivateDir::new(dir.join(format!(".pending-{}", Uuid::new_v4())))?;
        let file = stage.path().join("version.json");
        fs::write(&file, serde_json::to_vec(&self)?)?;
        fs::rename(file, path)?;
        Ok(())
    }
}

#[derive(Default)]
pub struct UpdateCache {
    cfst: Mutex<Option<CheckedVersion>>,
    xray: Mutex<Option<CheckedVersion>>,
}

fn refresh_release(
    engine: Engine,
    cached: &mut Option<CheckedVersion>,
    path: Option<&Path>,
) -> Result<Release> {
    let release = engine.release()?;
    let checked = CheckedVersion {
        version: release.version,
        checked_at: Utc::now().timestamp(),
    };
    *cached = Some(checked);
    if let Some(path) = path {
        // A disposable disk cache must not turn a successful check into a failure.
        let _ = checked.write(path);
    }
    Ok(release)
}

impl Release {
    fn parse(engine: Engine, body: &[u8]) -> Result<Self> {
        let GithubRelease {
            tag_name,
            draft,
            prerelease,
            assets,
        } = serde_json::from_slice(body)?;
        let version = tag_name
            .strip_prefix('v')
            .and_then(parse_version)
            .filter(|_| !draft && !prerelease)
            .ok_or("GitHub 未返回有效的稳定版版本号")?;
        let expected = engine.info().3;
        let asset = assets
            .into_iter()
            .find(|asset| asset.name == expected)
            .ok_or("该版本没有受支持的 macOS ARM64 压缩包")?;
        let digest = asset
            .digest
            .as_deref()
            .and_then(|digest| digest.strip_prefix("sha256:"))
            .filter(|hex| hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
            .ok_or("GitHub 未提供有效的 SHA-256 校验值")?;
        if !(1..=MAX_ARCHIVE).contains(&asset.size) {
            return Err("组件压缩包大小超出限制".into());
        }
        Ok(Self {
            version,
            digest: digest.to_ascii_lowercase(),
            size: asset.size,
        })
    }
}

fn fetch(
    url: &str,
    allow_redirects: bool,
    destination: &mut impl io::Write,
    limit: u64,
) -> Result<u64> {
    let control = Control::default();
    let mut curl = command("/usr/bin/curl");
    curl.args([
        "-q",
        "--fail",
        "--silent",
        "--proto",
        "=https",
        "--proto-redir",
        "=https",
        "--connect-timeout",
        "10",
        "--max-time",
        if allow_redirects { "120" } else { "15" },
        "--max-redirs",
        "3",
        "--user-agent",
        "cftool",
    ]);
    if allow_redirects {
        curl.arg("--location");
    }
    let mut child = Running::start(curl.arg(url).stdout(Stdio::piped()), &control)?;
    let bytes = child.copy_stdout(destination, limit)?;
    if !child
        .wait(&control, Some(Duration::from_secs(2)))?
        .success()
    {
        return Err("无法从 GitHub 获取组件，请检查网络或稍后重试".into());
    }
    Ok(bytes)
}

pub struct Component {
    engine: Engine,
    pub bundled: PathBuf,
    root: PathBuf,
}

impl Component {
    pub fn new(app: &AppHandle, engine: Engine) -> tauri::Result<Self> {
        Ok(Self {
            engine,
            bundled: app
                .path()
                .resolve(engine.info().1, BaseDirectory::Resource)?,
            root: app.path().app_data_dir()?.join("engines"),
        })
    }

    fn downloaded(&self) -> PathBuf {
        self.root.join(self.engine.info().0)
    }

    fn has_download(&self) -> Result<bool> {
        match fs::symlink_metadata(&self.root) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
            Ok(meta) if meta.is_dir() => (),
            Ok(_) => return Err("组件安装目录无效".into()),
            Err(e) => return Err(e.into()),
        }
        match fs::symlink_metadata(self.downloaded()) {
            Ok(meta) if meta.is_file() => Ok(true),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
            Ok(_) => Err("已下载的组件不是普通文件".into()),
            Err(e) => Err(e.into()),
        }
    }

    fn current(&self, has_download: bool) -> Result<(Version, bool)> {
        let bundled = self.engine.read_version(&self.bundled)?;
        if has_download
            && let Ok(downloaded) = self.engine.read_version(&self.downloaded())
            && downloaded > bundled
        {
            return Ok((downloaded, true));
        }
        Ok((bundled, false))
    }

    pub fn for_probe(&self) -> PathBuf {
        if self.has_download().unwrap_or(false) && matches!(self.current(true), Ok((_, true))) {
            self.downloaded()
        } else {
            self.bundled.clone()
        }
    }

    fn status(&self, latest: Option<Version>) -> Result<Status> {
        let has_download = self.has_download()?;
        let (current, downloaded) = self.current(has_download)?;
        Ok(Status {
            current: tag(current),
            downloaded,
            has_download,
            latest: latest.map(tag),
            update_available: latest.is_some_and(|latest| latest > current),
        })
    }

    fn install(&self, expected: &str, release: &Release) -> Result<Status> {
        let latest = tag(release.version);
        if expected != latest {
            return Err("GitHub 上的最新版本已变化，请重新检查更新".into());
        }
        let status = self.status(Some(release.version))?;
        if !status.update_available {
            return Ok(status);
        }
        let (_, _, repo, asset) = self.engine.info();
        DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&self.root)?;
        let stage = PrivateDir::new(self.root.join(format!(".pending-{}", Uuid::new_v4())))?;
        let archive = stage.path().join("release.zip");
        let bytes = fetch(
            &format!("https://github.com/{repo}/releases/download/{latest}/{asset}"),
            true,
            &mut File::create(&archive)?,
            release.size,
        )?;
        if bytes != release.size {
            return Err("组件压缩包下载不完整".into());
        }
        self.publish(&stage, &archive, release)?;
        Ok(Status {
            current: latest.clone(),
            downloaded: true,
            has_download: true,
            latest: Some(latest),
            update_available: false,
        })
    }

    fn restore(&self, latest: Option<Version>) -> Result<Status> {
        if self.has_download()? {
            fs::remove_file(self.downloaded())?;
        }
        self.status(latest)
    }

    fn publish(&self, stage: &PrivateDir, archive: &Path, release: &Release) -> Result<()> {
        let mut hash = Sha256::new();
        io::copy(&mut File::open(archive)?, &mut hash)?;
        if format!("{:x}", hash.finalize()) != release.digest {
            return Err("组件压缩包 SHA-256 校验失败".into());
        }
        let control = Control::default();
        let name = self.engine.info().0;
        let binary = stage.path().join(name);
        let mut unzip = Running::start(
            command("/usr/bin/unzip")
                .env_clear()
                .args(["-p"])
                .arg(archive)
                .arg(name)
                .stdout(Stdio::piped()),
            &control,
        )?;
        let file = File::create(&binary)?;
        // Stream only the named binary; never let ZIP entries choose a filesystem destination.
        unzip.copy_stdout(&mut &file, MAX_BINARY)?;
        if !unzip
            .wait(&control, Some(Duration::from_secs(5)))?
            .success()
        {
            return Err("无法解压组件可执行文件".into());
        }
        let mut header = [0; 16];
        File::open(&binary)?.read_exact(&mut header)?;
        if header[..8] != [0xcf, 0xfa, 0xed, 0xfe, 0x0c, 0, 0, 1] || header[12..] != [2, 0, 0, 0] {
            return Err("发布包未包含 macOS ARM64 可执行文件".into());
        }
        file.set_permissions(Permissions::from_mode(0o700))?;
        file.sync_all()?;
        drop(file);
        if self.engine.read_version(&binary)? != release.version {
            return Err("组件版本与 GitHub 发布版本不一致".into());
        }
        fs::rename(binary, self.downloaded())?;
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Status,
    Check,
    Install,
    Restore,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    current: String,
    downloaded: bool,
    has_download: bool,
    latest: Option<String>,
    update_available: bool,
}

#[tauri::command]
pub async fn engine_update(
    app: AppHandle,
    engine: Engine,
    action: Action,
    expected: Option<String>,
) -> std::result::Result<Status, String> {
    if !cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        return Err("组件更新仅支持 macOS Apple Silicon".into());
    }
    let component = Component::new(&app, engine).map_err(|e| e.to_string())?;
    let cache_path = app
        .path()
        .app_data_dir()
        .ok()
        .map(|root| root.join("cache").join(format!("{}.json", engine.info().0)));
    tauri::async_runtime::spawn_blocking(move || -> Result<Status> {
        let cache = app.state::<UpdateCache>();
        let mut cached = match engine {
            Engine::Cfst => &cache.cfst,
            Engine::Xray => &cache.xray,
        }
        .lock()
        .unwrap();
        let path = cache_path.as_deref();
        if cached.is_none() {
            *cached = path.and_then(CheckedVersion::read);
        }
        let last_known = cached
            .filter(|entry| entry.younger_than(SHOW_CACHED_FOR))
            .map(|entry| entry.version);
        match action {
            Action::Status => {
                drop(cached);
                component.status(last_known)
            }
            Action::Check => {
                let version = match *cached {
                    Some(entry) if entry.younger_than(RECHECK_AFTER) => entry.version,
                    _ => refresh_release(engine, &mut cached, path)?.version,
                };
                drop(cached);
                component.status(Some(version))
            }
            Action::Install | Action::Restore => {
                let state = app.state::<ProbeState>();
                let _guard = state
                    .engine_files
                    .try_lock()
                    .map_err(|_| "测速或组件更新正在运行，请稍后重试")?;
                if let Action::Install = action {
                    let expected = expected.as_deref().ok_or("请先检查更新")?;
                    // Never use cached metadata to authorize a download.
                    let release = refresh_release(engine, &mut cached, path)?;
                    drop(cached);
                    return component.install(expected, &release);
                }
                drop(cached);
                component.restore(last_known)
            }
        }
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}

#[cfg(all(test, target_os = "macos", target_arch = "aarch64"))]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp() -> PrivateDir {
        PrivateDir::new(std::env::temp_dir().join(format!("cftool-update-{}", Uuid::new_v4())))
            .unwrap()
    }

    fn simulate_older_bundle(engine: Engine, work: &Path) -> Component {
        let (name, _, _, _) = engine.info();
        let prefix = if name == "cfst" {
            "CloudflareSpeedTest v"
        } else {
            "Xray "
        };
        let bundled = work.join(format!("bundled-{name}"));
        fs::write(&bundled, format!("#!/bin/sh\necho '{prefix}0.0.0'\n")).unwrap();
        fs::set_permissions(&bundled, Permissions::from_mode(0o700)).unwrap();
        Component {
            engine,
            bundled,
            root: work.join("engines"),
        }
    }

    #[test]
    fn release_requires_stable_tag_exact_asset_and_sha256() {
        let mut release = json!({
            "tag_name": "v2.10.0", "draft": false, "prerelease": false,
            "assets": [{ "name": "cfst_darwin_arm64.zip", "size": 4096,
                         "digest": format!("sha256:{}", "a".repeat(64)) }],
        });
        let parse = |release: &serde_json::Value, engine| {
            Release::parse(engine, release.to_string().as_bytes())
        };
        assert!(parse(&release, Engine::Cfst).unwrap().version > [2, 9, 99]);
        assert!(parse(&release, Engine::Xray).is_err());
        release["prerelease"] = json!(true);
        assert!(parse(&release, Engine::Cfst).is_err());
        release["prerelease"] = json!(false);
        release["tag_name"] = json!("v2.10.0/../xray");
        assert!(parse(&release, Engine::Cfst).is_err());
        release["tag_name"] = json!("v2.10.0");
        release["assets"][0]["size"] = json!(MAX_ARCHIVE + 1);
        assert!(parse(&release, Engine::Cfst).is_err());
        release["assets"][0]["size"] = json!(4096);
        release["assets"][0]["digest"] = json!(null);
        assert!(parse(&release, Engine::Cfst).is_err());
    }

    #[test]
    fn validated_binary_replaces_atomically_and_can_be_restored() {
        let work = temp();
        let mut component = simulate_older_bundle(Engine::Cfst, work.path());
        assert_eq!(component.for_probe(), component.bundled);
        fs::create_dir(&component.root).unwrap();
        let stage = PrivateDir::new(component.root.join("pending")).unwrap();
        let archive = stage.path().join("release.zip");
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let bundled = manifest.join("resources/bin/cfst");
        assert!(
            command("/usr/bin/zip")
                .arg("-jq")
                .arg(&archive)
                .arg(&bundled)
                .arg(manifest.join("../src/assets/ip.txt"))
                .status()
                .unwrap()
                .success()
        );
        let version = Engine::Cfst.read_version(&bundled).unwrap();
        let mut release = Release {
            version,
            digest: format!("{:x}", Sha256::digest(fs::read(&archive).unwrap())),
            size: fs::metadata(&archive).unwrap().len(),
        };
        component.publish(&stage, &archive, &release).unwrap();
        let status = component.status(Some(version)).unwrap();
        assert!(status.downloaded && !status.update_available);
        assert_eq!(component.for_probe(), component.downloaded());
        assert!(!stage.path().join("ip.txt").exists());

        let installed = fs::read(component.downloaded()).unwrap();
        release.version = [99, 0, 0];
        assert!(component.publish(&stage, &archive, &release).is_err());
        release.digest = "0".repeat(64);
        assert!(component.publish(&stage, &archive, &release).is_err());
        assert_eq!(fs::read(component.downloaded()).unwrap(), installed);

        component.bundled = bundled;
        assert_eq!(component.for_probe(), component.bundled);
        assert!(!component.restore(None).unwrap().has_download);
        assert!(!component.downloaded().exists());
    }

    #[test]
    #[ignore = "downloads and validates the latest official GitHub assets"]
    fn downloads_both_official_releases_into_isolated_storage() {
        let work = temp();
        for engine in [Engine::Cfst, Engine::Xray] {
            let component = simulate_older_bundle(engine, work.path());
            let release = engine.release().unwrap();
            let latest = tag(release.version);
            let status = component.install(&latest, &release).unwrap();
            eprintln!("{} {latest}", engine.info().0);
            assert_eq!(status.current, latest);
            assert!(status.downloaded);
            assert_eq!(component.for_probe(), component.downloaded());
            assert_eq!(component.restore(None).unwrap().current, "v0.0.0");
        }
    }
}
