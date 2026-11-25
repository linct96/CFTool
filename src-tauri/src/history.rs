use crate::{
    probe::{CandidateGroup, CandidateItem, Group, Row, sort_cfst},
    process::{Control, PrivateDir, Result},
    xray,
};
use chrono::{NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs::{self, DirBuilder, File, OpenOptions},
    io::{self, BufReader, BufWriter, Write},
    net::SocketAddr,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::{Path, PathBuf},
};
use uuid::Uuid;

const TIME_FORMAT: &str = "%Y%m%dT%H%M%S%.3fZ";
const VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    Running,
    Paused,
    Interrupted,
    Failed,
    Completed,
    Unavailable,
}

impl Status {
    fn in_history(self) -> Self {
        if self == Self::Running {
            Self::Interrupted
        } else {
            self
        }
    }
}

#[derive(Clone, Deserialize, Serialize)]
pub struct NodeRef {
    pub default_port: u16,
    pub fingerprint: String,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct Batch {
    pub end: usize,
    // An empty result still means this batch completed, unlike None.
    pub rows: Option<Vec<Row>>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct Scan {
    pub default_port: u16,
    pub port: u16,
    pub items: Vec<CandidateItem>,
    pub batches: Vec<Batch>,
    pub next_size: usize,
}

impl Scan {
    pub fn done_end(&self) -> usize {
        self.batches
            .iter()
            .rev()
            .find(|batch| batch.rows.is_some())
            .map_or(0, |batch| batch.end)
    }
}

#[derive(Deserialize, Serialize)]
pub struct Retest {
    pub endpoint: SocketAddr,
    pub row: Option<Row>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Record {
    version: u32,
    pub status: Status,
    pub settings: HashMap<String, String>,
    pub inherited_port: Option<u16>,
    pub source_id: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub appended_xray: bool,
    pub scans: Vec<Scan>,
    pub nodes: Vec<NodeRef>,
    // Indexed by original node order. None means CFST hasn't finalized the target list yet.
    pub retests: Option<Vec<Vec<Retest>>>,
}

impl Record {
    pub fn new(
        settings: HashMap<String, String>,
        inherited_port: Option<u16>,
        scans: Vec<Scan>,
        nodes: Vec<NodeRef>,
    ) -> Self {
        Self {
            version: VERSION,
            status: Status::Running,
            settings,
            inherited_port,
            source_id: None,
            appended_xray: false,
            scans,
            nodes,
            retests: None,
        }
    }

    fn group_ports(&self) -> impl Iterator<Item = (Option<usize>, u16)> + '_ {
        self.scans
            .first()
            .filter(|_| self.nodes.is_empty() || self.appended_xray)
            .map(|scan| (None, scan.default_port))
            .into_iter()
            .chain(
                self.nodes
                    .iter()
                    .enumerate()
                    .map(|(idx, node)| (Some(idx + 1), node.default_port)),
            )
    }

    pub fn candidates(&self) -> Vec<CandidateGroup> {
        self.group_ports()
            .map(|(node, default_port)| CandidateGroup {
                node,
                default_port,
                items: self
                    .scans
                    .iter()
                    .filter(|scan| scan.default_port == default_port)
                    .flat_map(|scan| scan.items.iter().cloned())
                    .collect(),
            })
            .collect()
    }

    pub fn scans_by_default(&self) -> BTreeMap<u16, Vec<Row>> {
        let mut all = BTreeMap::<_, Vec<Row>>::new();
        for scan in &self.scans {
            all.entry(scan.default_port).or_default().extend(
                scan.batches
                    .iter()
                    .filter_map(|batch| batch.rows.as_ref())
                    .flatten()
                    .cloned(),
            );
        }
        for rows in all.values_mut() {
            sort_cfst(rows);
        }
        all
    }

    pub fn groups(&self) -> Vec<Group> {
        let scans = self.scans_by_default();
        self.group_ports()
            .map(|(node, default)| {
                let cfst = scans.get(&default).cloned().unwrap_or_default();
                let mut xray: Vec<_> = node
                    .and_then(|idx| self.retests.as_ref()?.get(idx - 1))
                    .into_iter()
                    .flatten()
                    .filter_map(|test| test.row.clone())
                    .collect();
                xray::sort_results(&mut xray);
                Group { node, cfst, xray }
            })
            .collect()
    }

    // Don't execute a record with inconsistent boundaries or target identities.
    fn validate(&self) -> Result<()> {
        if self.version != VERSION {
            return Err("不支持此测速历史版本".into());
        }
        if self.status == Status::Unavailable {
            return Err("测速历史状态无效".into());
        }
        if self.appended_xray && (self.nodes.is_empty() || self.retests.is_none()) {
            return Err("测速历史缺少追加 Xray 计划".into());
        }
        let mut groups = BTreeSet::new();
        let mut qualified = BTreeMap::<_, BTreeSet<_>>::new();
        for scan in &self.scans {
            let mut start = 0;
            let mut ips = BTreeSet::new();
            if scan.port == 0
                || scan.default_port == 0
                || scan.items.is_empty()
                || scan.next_size == 0
                || !groups.insert((scan.default_port, scan.port))
                || scan.items.iter().any(|item| {
                    item.endpoint.port() != scan.port || !ips.insert(item.endpoint.ip())
                })
            {
                return Err("CFST 历史计划无效".into());
            }
            for (idx, batch) in scan.batches.iter().enumerate() {
                if batch.end <= start
                    || batch.end > scan.items.len()
                    || batch.rows.is_none() && idx + 1 != scan.batches.len()
                {
                    return Err("CFST 历史检查点无效".into());
                }
                if let Some(rows) = &batch.rows {
                    let allowed: BTreeSet<_> = scan.items[start..batch.end]
                        .iter()
                        .map(|item| item.endpoint)
                        .collect();
                    let qualified = qualified.entry(scan.default_port).or_default();
                    if rows.iter().any(|row| {
                        !allowed.contains(&row.endpoint) || !qualified.insert(row.endpoint)
                    }) {
                        return Err("CFST 检查点含未知或重复的入口".into());
                    }
                }
                start = batch.end;
            }
        }
        if groups.is_empty()
            || self.nodes.iter().any(|node| {
                !groups
                    .iter()
                    .any(|(default, _)| *default == node.default_port)
            })
        {
            return Err("测速历史缺少分组".into());
        }
        if self.retests.is_some() || self.status == Status::Completed {
            if self
                .scans
                .iter()
                .any(|scan| scan.done_end() != scan.items.len())
            {
                return Err("测速历史缺少已完成的 CFST 检查点".into());
            }
            let retests = self.retests.as_ref().ok_or("测速历史缺少 Xray 计划")?;
            if retests.len() != self.nodes.len()
                || retests.iter().zip(&self.nodes).any(|(tests, node)| {
                    let mut seen = BTreeSet::new();
                    tests.len() > xray::TOP
                        || tests.iter().any(|test| {
                            !qualified
                                .get(&node.default_port)
                                .is_some_and(|rows| rows.contains(&test.endpoint))
                                || !seen.insert(test.endpoint)
                                || match &test.row {
                                    Some(row) => {
                                        row.endpoint != test.endpoint || row.sent != xray::ROUNDS
                                    }
                                    None => self.status == Status::Completed,
                                }
                        })
                })
            {
                return Err("Xray 历史检查点无效".into());
            }
        }
        Ok(())
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: String,
    started_at: i64,
    pub status: Status,
}

impl Entry {
    fn parse(id: &str, status: Status) -> Option<Self> {
        let (time, suffix) = id.split_once('-')?;
        let timestamp = NaiveDateTime::parse_from_str(time, TIME_FORMAT).ok()?;
        if timestamp.format(TIME_FORMAT).to_string() != time
            || Uuid::parse_str(suffix).ok()?.simple().to_string() != suffix
        {
            return None;
        }
        Some(Self {
            id: id.into(),
            started_at: timestamp.and_utc().timestamp_millis(),
            status,
        })
    }
}

pub fn new_id() -> String {
    format!(
        "{}-{}",
        Utc::now().format(TIME_FORMAT),
        Uuid::new_v4().simple()
    )
}

// Serialize write/delete across app instances as well as across Xray worker lanes.
pub fn lock(root: &Path) -> Result<File> {
    DirBuilder::new().recursive(true).mode(0o700).create(root)?;
    check_root(root)?;
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .open(root.join(".lock"))?;
    match file.try_lock() {
        Ok(()) => Ok(file),
        Err(std::fs::TryLockError::WouldBlock) => Err("另一轮测速正在使用历史，请稍后重试".into()),
        Err(std::fs::TryLockError::Error(err)) => Err(err.into()),
    }
}

fn check_root(root: &Path) -> Result<()> {
    if !fs::symlink_metadata(root)?.is_dir() {
        return Err("测速历史目录无效".into());
    }
    Ok(())
}

#[derive(Debug)]
pub struct WriteError(pub Box<dyn std::error::Error + Send + Sync>);

impl std::fmt::Display for WriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "无法保存测速历史：{}", self.0)
    }
}

impl std::error::Error for WriteError {}

pub fn save(
    root: &Path,
    id: &str,
    record: &Record,
    guarded: Option<(&Control, bool)>,
) -> Result<()> {
    let destination = json_path(root, id)?;
    let temporary = PrivateDir::new(root.join(format!(".write-{}", Uuid::new_v4())))?;
    let path = temporary.path().join("record.json");
    let mut file = BufWriter::new(
        OpenOptions::new()
            .create_new(true)
            .mode(0o600)
            .write(true)
            .open(&path)?,
    );
    serde_json::to_writer(&mut file, record)?;
    file.flush()?;
    file.get_ref().sync_all()?;
    if let Some((control, finish)) = guarded {
        control.commit(finish, || fs::rename(&path, &destination))?;
    } else {
        fs::rename(&path, &destination)?;
    }
    Ok(())
}

fn json_path(root: &Path, id: &str) -> Result<PathBuf> {
    Entry::parse(id, Status::Running).ok_or("无效的测速记录编号")?;
    check_root(root)?;
    Ok(root.join(format!("{id}.json")))
}

fn record_reader(path: &Path) -> Result<BufReader<File>> {
    if !fs::symlink_metadata(path)?.is_file() {
        return Err("测速历史文件无效".into());
    }
    Ok(BufReader::new(File::open(path)?))
}

pub fn load(root: &Path, id: &str) -> Result<Record> {
    let record: Record = serde_json::from_reader(record_reader(&json_path(root, id)?)?)?;
    record.validate()?;
    Ok(record)
}

pub fn list(root: &Path) -> Result<Vec<Entry>> {
    match fs::symlink_metadata(root) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Ok(meta) if meta.is_dir() => (),
        Ok(_) => return Err("测速历史目录无效".into()),
        Err(e) => return Err(e.into()),
    }
    #[derive(Deserialize)]
    struct Header {
        version: u32,
        status: Status,
    }
    let mut records = Vec::new();
    for file in fs::read_dir(root)? {
        let file = file?;
        let name = file.file_name();
        let Some(name) = name.to_str() else { continue };
        let kind = file.file_type()?;
        if kind.is_file()
            && let Some(id) = name.strip_suffix(".json")
            && let Some(mut entry) = Entry::parse(id, Status::Unavailable)
        {
            if let Ok(header) = record_reader(&file.path())
                .and_then(|file| Ok(serde_json::from_reader::<_, Header>(file)?))
                && header.version == VERSION
            {
                entry.status = header.status.in_history();
            }
            records.push(entry);
        }
    }
    records.sort_unstable_by(|a, b| b.id.cmp(&a.id));
    Ok(records)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Detail {
    status: Status,
    groups: Vec<Group>,
    download: bool,
    cfst_progress: [usize; 2],
    xray_progress: Option<[usize; 2]>,
    appended_xray: bool,
}

pub fn read(root: &Path, id: &str) -> Result<Detail> {
    let record = load(root, id)?;
    Ok(Detail {
        status: record.status.in_history(),
        appended_xray: record.appended_xray,
        groups: record.groups(),
        download: record
            .settings
            .get("download")
            .is_some_and(|value| value == "yes"),
        cfst_progress: [
            record.scans.iter().map(Scan::done_end).sum(),
            record.scans.iter().map(|scan| scan.items.len()).sum(),
        ],
        xray_progress: record
            .retests
            .as_ref()
            .filter(|_| !record.nodes.is_empty())
            .map(|tests| {
                [
                    tests
                        .iter()
                        .flatten()
                        .filter(|test| test.row.is_some())
                        .count(),
                    tests.iter().map(Vec::len).sum(),
                ]
            }),
    })
}

pub fn delete(root: &Path, id: &str) -> Result<()> {
    let _lock = lock(root)?;
    let path = json_path(root, id)?;
    if !fs::symlink_metadata(&path)?.is_file() {
        return Err("测速历史文件无效".into());
    }
    fs::remove_file(path)?;
    Ok(())
}
