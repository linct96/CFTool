mod candidates;
mod config;
mod engines;
mod history;
mod probe;
mod process;
mod xray;

use engines::{Component, Engine, UpdateCache, engine_update};
use process::Control;
use std::{
    fs::{self, OpenOptions},
    io::{self, Read, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use tauri::{AppHandle, Manager, RunEvent, State, ipc::Channel};

#[derive(Default)]
struct ProbeState {
    engine_files: Mutex<()>,
    active: Mutex<Option<(String, Arc<Control>)>>,
    append_source: Mutex<Option<probe::AppendSource>>,
    finished: Condvar,
    exiting: AtomicBool,
}

struct ActiveRun<'a>(&'a ProbeState);

impl Drop for ActiveRun<'_> {
    fn drop(&mut self) {
        *self.0.active.lock().unwrap() = None;
        self.0.finished.notify_all();
    }
}

fn history_dir(app: &AppHandle) -> tauri::Result<PathBuf> {
    Ok(app.path().app_data_dir()?.join("history"))
}

const INPUT_LIMIT: usize = 1024 * 1024;
const IP_LIST: StoredInput = StoredInput {
    filename: "ip-list.txt",
    label: "IP 列表",
};
const VLESS_NODES: StoredInput = StoredInput {
    filename: "nodes.txt",
    label: "VLESS 节点",
};

struct StoredInput {
    filename: &'static str,
    label: &'static str,
}

impl StoredInput {
    fn check_size(&self, text: &str) -> io::Result<()> {
        if text.len() > INPUT_LIMIT {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{}超过 1 MB", self.label),
            ));
        }
        Ok(())
    }

    fn read(&self, dir: &Path) -> io::Result<Option<String>> {
        let file = match fs::File::open(dir.join(self.filename)) {
            Ok(file) => file,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(err),
        };
        let mut text = String::new();
        file.take(INPUT_LIMIT as u64 + 1)
            .read_to_string(&mut text)?;
        self.check_size(&text)?;
        Ok(Some(text))
    }

    fn write(&self, dir: &Path, text: &str) -> io::Result<()> {
        self.check_size(text)?;
        fs::create_dir_all(dir)?;
        let temporary =
            process::PrivateDir::new(dir.join(format!(".write-{}", uuid::Uuid::new_v4())))?;
        let path = temporary.path().join(self.filename);
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&path)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        fs::rename(path, dir.join(self.filename))
    }
}

#[tauri::command]
async fn load_ip_list(app: AppHandle) -> Result<Option<String>, String> {
    let dir = app.path().app_data_dir().map_err(|err| err.to_string())?;
    tauri::async_runtime::spawn_blocking(move || IP_LIST.read(&dir).map_err(|err| err.to_string()))
        .await
        .map_err(|err| err.to_string())?
}

#[tauri::command]
async fn save_ip_list(app: AppHandle, ips: String) -> Result<(), String> {
    let dir = app.path().app_data_dir().map_err(|err| err.to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        IP_LIST.write(&dir, &ips).map_err(|err| err.to_string())
    })
    .await
    .map_err(|err| err.to_string())?
}

#[tauri::command]
async fn load_vless_nodes(app: AppHandle) -> Result<Option<String>, String> {
    let dir = app.path().app_data_dir().map_err(|err| err.to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        VLESS_NODES.read(&dir).map_err(|err| err.to_string())
    })
    .await
    .map_err(|err| err.to_string())?
}

#[tauri::command]
async fn save_vless_nodes(app: AppHandle, nodes: String) -> Result<(), String> {
    let dir = app.path().app_data_dir().map_err(|err| err.to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        VLESS_NODES
            .write(&dir, &nodes)
            .map_err(|err| err.to_string())
    })
    .await
    .map_err(|err| err.to_string())?
}

#[tauri::command]
async fn run_probe(
    app: AppHandle,
    operation_id: String,
    request: String,
    progress: Channel<probe::Event>,
) -> Result<Option<probe::Report>, String> {
    if request.len() > 1024 * 1024 {
        return Err("测速输入超过 1 MB".into());
    }
    let request: probe::Request = serde_json::from_str(&request).map_err(|_| "测速输入格式无效")?;
    let cfst = Component::new(&app, Engine::Cfst).map_err(|err| err.to_string())?;
    let xray = Component::new(&app, Engine::Xray).map_err(|err| err.to_string())?;
    let root = history_dir(&app).map_err(|err| err.to_string())?;
    probe_task(app, operation_id, move |state, control| {
        let has_nodes = request.has_nodes();
        let engines = || {
            let xray = if has_nodes {
                xray.for_probe()
            } else {
                xray.bundled
            };
            (cfst.for_probe(), xray)
        };
        let (groups, source) = probe::run(request, engines, &root, control, |event| {
            if matches!(event, probe::Event::Candidates { .. }) {
                *state.append_source.lock().unwrap() = None;
            }
            let _ = progress.send(event);
        })?;
        *state.append_source.lock().unwrap() = source;
        Ok(groups)
    })
    .await
}

#[tauri::command]
async fn append_xray_probe(
    app: AppHandle,
    operation_id: String,
    nodes: String,
    progress: Channel<probe::Event>,
) -> Result<Option<probe::Report>, String> {
    VLESS_NODES
        .check_size(&nodes)
        .map_err(|err| err.to_string())?;
    let xray = Component::new(&app, Engine::Xray).map_err(|err| err.to_string())?;
    let root = history_dir(&app).map_err(|err| err.to_string())?;
    probe_task(app, operation_id, move |state, control| {
        let source = state
            .append_source
            .lock()
            .unwrap()
            .clone()
            .ok_or("没有可追加的 CFST 结果，请先完成纯 CFST 测速")?;
        let groups = probe::append_xray(
            &source,
            &nodes,
            &xray.for_probe(),
            &root,
            control,
            |event| {
                if matches!(event, probe::Event::Candidates { .. }) {
                    *state.append_source.lock().unwrap() = None;
                }
                let _ = progress.send(event);
            },
        )?;
        Ok(probe::Report {
            id: source.history_id,
            groups,
        })
    })
    .await
}

#[tauri::command]
async fn resume_probe(
    app: AppHandle,
    operation_id: String,
    id: String,
    nodes: String,
    progress: Channel<probe::Event>,
) -> Result<Option<Vec<probe::Group>>, String> {
    VLESS_NODES
        .check_size(&nodes)
        .map_err(|err| err.to_string())?;
    let cfst = Component::new(&app, Engine::Cfst).map_err(|err| err.to_string())?;
    let xray = Component::new(&app, Engine::Xray).map_err(|err| err.to_string())?;
    let root = history_dir(&app).map_err(|err| err.to_string())?;
    probe_task(app, operation_id, move |state, control| {
        *state.append_source.lock().unwrap() = None;
        probe::resume(
            &id,
            &nodes,
            (&cfst.for_probe(), &xray.for_probe()),
            &root,
            control,
            |event| {
                let _ = progress.send(event);
            },
        )
    })
    .await
}

#[derive(serde::Deserialize)]
struct HistoryRetest {
    id: String,
    endpoint: std::net::SocketAddr,
    node: Option<usize>,
    kind: probe::RetestKind,
    nodes: String,
}

#[tauri::command]
async fn retest_history_probe(
    app: AppHandle,
    operation_id: String,
    request: HistoryRetest,
    progress: Channel<String>,
) -> Result<Option<probe::SingleResult>, String> {
    let HistoryRetest {
        id,
        endpoint,
        node,
        kind,
        nodes,
    } = request;
    VLESS_NODES
        .check_size(&nodes)
        .map_err(|err| err.to_string())?;
    let engine = match kind {
        probe::RetestKind::Cfst => Engine::Cfst,
        probe::RetestKind::Xray => Engine::Xray,
    };
    let component = Component::new(&app, engine).map_err(|err| err.to_string())?;
    let root = history_dir(&app).map_err(|err| err.to_string())?;
    probe_task(app, operation_id, move |_, control| {
        let request = probe::retest_history(&root, &id, endpoint, node, kind, &nodes)?;
        probe::retest_one(request, &component.for_probe(), control, |message| {
            let _ = progress.send(message);
        })
    })
    .await
}

#[tauri::command]
async fn retest_probe(
    app: AppHandle,
    operation_id: String,
    request: String,
    progress: Channel<String>,
) -> Result<Option<probe::SingleResult>, String> {
    if request.len() > 1024 * 1024 {
        return Err("复测输入超过 1 MB".into());
    }
    let request: probe::SingleRequest =
        serde_json::from_str(&request).map_err(|_| "复测输入格式无效")?;
    let engine = match &request.test {
        probe::SingleTest::Cfst { .. } => Engine::Cfst,
        probe::SingleTest::Xray { .. } => Engine::Xray,
    };
    let component = Component::new(&app, engine).map_err(|err| err.to_string())?;
    probe_task(app, operation_id, move |_, control| {
        probe::retest_one(request, &component.for_probe(), control, |message| {
            let _ = progress.send(message);
        })
    })
    .await
}

async fn probe_task<T: Send + 'static>(
    app: AppHandle,
    operation_id: String,
    task: impl FnOnce(&ProbeState, &Control) -> process::Result<T> + Send + 'static,
) -> Result<Option<T>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<ProbeState>();
        let mut active = state.active.lock().unwrap();
        if state.exiting.load(Ordering::Relaxed) {
            return Ok(None);
        }
        if active.is_some() {
            return Err("已有测速正在运行".into());
        }
        let _engine_files = state
            .engine_files
            .try_lock()
            .map_err(|_| "组件正在更新，请稍后测速")?;
        let control = Arc::new(Control::default());
        *active = Some((operation_id, control.clone()));
        let _run = ActiveRun(&state);
        drop(active);

        let result = task(&state, &control);
        match result {
            Ok(value) => Ok(Some(value)),
            Err(err) if control.cancelled() && !err.is::<history::WriteError>() => Ok(None),
            Err(err) => Err(err.to_string()),
        }
    })
    .await
    .map_err(|err| err.to_string())?
}

#[tauri::command]
fn approve_probe(state: State<'_, ProbeState>, operation_id: String) -> Result<(), String> {
    if state
        .active
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|(id, control)| *id == operation_id && control.approve())
    {
        Ok(())
    } else {
        Err("没有待确认的测速".into())
    }
}

#[tauri::command]
fn stop_probe(state: State<'_, ProbeState>, operation_id: String) {
    if let Some((_, control)) = state
        .active
        .lock()
        .unwrap()
        .as_ref()
        .filter(|(id, _)| *id == operation_id)
    {
        control.cancel();
    }
}

async fn history_task<T: Send + 'static>(
    app: AppHandle,
    task: impl FnOnce(PathBuf) -> process::Result<T> + Send + 'static,
) -> Result<T, String> {
    let root = history_dir(&app).map_err(|err| err.to_string())?;
    tauri::async_runtime::spawn_blocking(move || task(root).map_err(|err| err.to_string()))
        .await
        .map_err(|err| err.to_string())?
}

#[tauri::command]
async fn list_probe_history(app: AppHandle) -> Result<Vec<history::Entry>, String> {
    history_task(app, |root| history::list(&root)).await
}

#[tauri::command]
async fn read_probe_history(app: AppHandle, id: String) -> Result<history::Detail, String> {
    history_task(app, move |root| history::read(&root, &id)).await
}

#[tauri::command]
async fn delete_probe_history(app: AppHandle, id: String) -> Result<(), String> {
    history_task(app, move |root| history::delete(&root, &id)).await
}

pub fn run() {
    tauri::Builder::default()
        .manage(ProbeState::default())
        .manage(UpdateCache::default())
        .invoke_handler(tauri::generate_handler![
            config::load_cfst_settings,
            config::save_cfst_settings,
            load_ip_list,
            save_ip_list,
            candidates::import_ip_subscription,
            load_vless_nodes,
            save_vless_nodes,
            engine_update,
            run_probe,
            append_xray_probe,
            resume_probe,
            retest_probe,
            retest_history_probe,
            approve_probe,
            stop_probe,
            list_probe_history,
            read_probe_history,
            delete_probe_history
        ])
        .build(tauri::generate_context!())
        .expect("failed to build cftool")
        .run(|app, event| {
            if let RunEvent::Exit = event {
                let state = app.state::<ProbeState>();
                let active = state.active.lock().unwrap();
                state.exiting.store(true, Ordering::Relaxed);
                if let Some((_, control)) = active.as_ref() {
                    control.cancel();
                    drop(
                        state
                            .finished
                            .wait_while(active, |run| run.is_some())
                            .unwrap(),
                    );
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn restores_ip_list_verbatim_including_an_intentionally_empty_list() {
        let temporary =
            process::PrivateDir::new(std::env::temp_dir().join(uuid::Uuid::new_v4().to_string()))
                .unwrap();
        let dir = temporary.path().join("app-data");
        assert_eq!(IP_LIST.read(&dir).unwrap(), None);

        let edited = "203.0.113.11:443 # 手动备注\n[2001:db8::1]:8443\n\n";
        IP_LIST.write(&dir, edited).unwrap();
        assert_eq!(IP_LIST.read(&dir).unwrap().as_deref(), Some(edited));
        let oversized = "a".repeat(INPUT_LIMIT + 1);
        assert!(IP_LIST.write(&dir, &oversized).is_err());
        assert_eq!(IP_LIST.read(&dir).unwrap().as_deref(), Some(edited));

        IP_LIST.write(&dir, "").unwrap();
        assert_eq!(IP_LIST.read(&dir).unwrap().as_deref(), Some(""));
        let path = dir.join("ip-list.txt");
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
        fs::write(&path, oversized).unwrap();
        assert!(IP_LIST.read(&dir).is_err());
        fs::write(&path, [0xff]).unwrap();
        assert!(IP_LIST.read(&dir).is_err());
    }

    #[test]
    fn restores_existing_vless_nodes_and_persists_clearing_them() {
        let temporary =
            process::PrivateDir::new(std::env::temp_dir().join(uuid::Uuid::new_v4().to_string()))
                .unwrap();
        let dir = temporary.path();
        let path = dir.join("nodes.txt");
        assert_eq!(VLESS_NODES.read(dir).unwrap(), None);
        let first = "vless://first.example\nvless://second.example\n";
        fs::write(&path, first).unwrap();
        assert_eq!(VLESS_NODES.read(dir).unwrap().as_deref(), Some(first));
        let second = "vless://third.example";
        VLESS_NODES.write(dir, second).unwrap();
        assert!(
            VLESS_NODES
                .write(dir, &"a".repeat(INPUT_LIMIT + 1))
                .is_err()
        );
        assert_eq!(VLESS_NODES.read(dir).unwrap().as_deref(), Some(second));
        VLESS_NODES.write(dir, "").unwrap();
        assert_eq!(VLESS_NODES.read(dir).unwrap().as_deref(), Some(""));
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(fs::read_dir(dir).unwrap().count(), 1);
    }
}
