use crate::{
    candidates,
    history::{self, Batch, NodeRef, Record, Retest, Scan, Status},
    process::{Control, PrivateDir, Result, Running, command},
    xray,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fmt::Display,
    fs,
    net::{IpAddr, SocketAddr},
    path::{Path, PathBuf},
    process::Stdio,
    str::FromStr,
    sync::Mutex,
    time::{Duration, Instant},
};
use url::Url;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct Request {
    ips: String,
    nodes: String,
    settings: HashMap<String, String>,
}

impl Request {
    pub fn has_nodes(&self) -> bool {
        !self.nodes.trim().is_empty()
    }
}

#[derive(Clone)]
pub struct AppendSource {
    pub history_id: String,
    inherited_port: Option<u16>,
}

#[derive(Deserialize)]
pub struct SingleRequest {
    endpoint: SocketAddr,
    pub test: SingleTest,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum SingleTest {
    Cfst { settings: HashMap<String, String> },
    Xray { node: String },
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RetestKind {
    Cfst,
    Xray,
}

#[derive(Serialize)]
pub struct SingleResult {
    pub row: Option<Row>,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct Row {
    pub endpoint: SocketAddr,
    pub sent: u32,
    pub received: u32,
    pub loss: f64,
    pub latency: Option<f64>,
    pub latency_min: Option<f64>,
    pub latency_max: Option<f64>,
    pub speed: Option<f64>,
    pub colo: Option<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, PartialEq, Serialize)]
pub struct Group {
    pub node: Option<usize>,
    pub cfst: Vec<Row>,
    pub xray: Vec<Row>,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct CandidateItem {
    pub endpoint: SocketAddr,
    pub notes: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CandidateGroup {
    pub node: Option<usize>,
    pub default_port: u16,
    pub items: Vec<CandidateItem>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct XrayNodeTargets {
    pub node: usize,
    pub endpoints: Vec<SocketAddr>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Stage {
    CheckingNetwork,
    PreparingTask,
    Cfst,
    PreparingXray,
    Xray,
    SavingResults,
    Completed,
    Paused,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Event {
    Stage {
        stage: Stage,
        progress: Option<[usize; 2]>,
    },
    Subscription {
        line: usize,
    },
    RouteWarning {
        message: String,
    },
    Candidates {
        history_id: String,
        groups: Vec<CandidateGroup>,
    },
    CfstBatchStart {
        default_port: u16,
        endpoints: Vec<SocketAddr>,
    },
    CfstBatchDone {
        default_port: u16,
        endpoints: Vec<SocketAddr>,
        rows: Vec<Row>,
    },
    XrayTargets {
        targets: Vec<XrayNodeTargets>,
    },
    XrayUpdate {
        node: usize,
        row: Row,
    },
}

fn stage(stage: Stage) -> Event {
    Event::Stage {
        stage,
        progress: None,
    }
}

struct Options {
    port: Option<u16>,
    threads: usize,
    args: Vec<String>,
    download: bool,
}

impl Options {
    fn parse(settings: &HashMap<String, String>) -> Result<Self> {
        let get = |key: &str| {
            settings
                .get(key)
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
        };
        let port = get("tp")
            .map(|s| number("tp", Some(s), 443, 1, u16::MAX))
            .transpose()?;
        let low = number("tll", get("tll"), 0, 0, 300000)?;
        let httping = get("mode") == Some("http");
        let download = get("download") == Some("yes");
        let mut args = vec!["-p".into(), "0".into()];
        let threads = number("n", get("n"), 200, 1, 1000)?;
        add(&mut args, "n", threads);
        add(&mut args, "t", number("t", get("t"), 4, 1, 100)?);
        add(&mut args, "tll", low);
        add(&mut args, "tl", number("tl", get("tl"), 9999, low, 300000)?);
        add(
            &mut args,
            "tlr",
            number("tlr", get("tlr"), 0.0f64, 0.0, 1.0)?,
        );
        if httping {
            args.push("-httping".into());
            if let Some(code) = get("httping-code") {
                add(
                    &mut args,
                    "httping-code",
                    number("httping-code", Some(code), 200, 100, 599)?,
                );
            }
            if let Some(colo) = get("cfcolo") {
                if !colo.split(',').all(|s| {
                    (2..=3).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_alphabetic())
                }) {
                    return Err("地区过滤应填写逗号分隔的地区代码，如 HKG,NRT".into());
                }
                add(&mut args, "cfcolo", colo.to_ascii_uppercase());
            }
        }
        if download {
            add(&mut args, "dn", number("dn", get("dn"), 10, 1, 1000)?);
            add(&mut args, "dt", number("dt", get("dt"), 10, 1, 600)?);
            add(
                &mut args,
                "sl",
                number("sl", get("sl"), 0.0f64, 0.0, 100000.0)?,
            );
        } else {
            args.push("-dd".into());
        }
        if httping || download {
            let raw = get("url").unwrap_or("https://cf.xiu2.xyz/url");
            let invalid = "CFST 测速地址必须为有效的 HTTP / HTTPS URL";
            let url = Url::parse(raw).map_err(|_| invalid)?;
            if !matches!(url.scheme(), "http" | "https")
                || !url.has_host()
                || !url.username().is_empty()
                || url.password().is_some()
                || raw.chars().any(char::is_whitespace)
            {
                return Err(invalid.into());
            }
            add(&mut args, "url", url);
        }
        Ok(Self {
            port,
            threads,
            args,
            download,
        })
    }

    fn snapshot(&self) -> HashMap<String, String> {
        let mut settings = HashMap::from([
            ("mode".into(), "tcp".into()),
            (
                "download".into(),
                if self.download { "yes" } else { "no" }.into(),
            ),
        ]);
        if let Some(port) = self.port {
            settings.insert("tp".into(), port.to_string());
        }
        let mut args = self.args[2..].iter(); // -p 0 only changes console output.
        while let Some(flag) = args.next() {
            match flag.as_str() {
                "-httping" => {
                    settings.insert("mode".into(), "http".into());
                }
                "-dd" => (),
                _ => {
                    settings.insert(flag[1..].to_string(), args.next().unwrap().clone());
                }
            }
        }
        settings
    }
}

fn add(args: &mut Vec<String>, key: &str, value: impl Display) {
    args.extend([format!("-{key}"), value.to_string()]);
}

fn number<T: FromStr + PartialOrd + Display>(
    key: &str,
    raw: Option<&str>,
    default: T,
    min: T,
    max: T,
) -> Result<T> {
    let value = match raw {
        Some(s) => s.parse().map_err(|_| format!("CFST 参数 -{key} 无效"))?,
        None => default,
    };
    if !(min <= value && value <= max) {
        return Err(format!("CFST 参数 -{key} 必须在 {min}–{max} 之间").into());
    }
    Ok(value)
}

fn tunnel_interface(interface: &str) -> bool {
    ["utun", "tun", "tap", "ppp", "ipsec"]
        .iter()
        .any(|prefix| interface.starts_with(prefix))
}

fn network_output(program: &str, args: &[&str], control: &Control) -> Result<Option<String>> {
    let output = Running::start(command(program).args(args).stdout(Stdio::piped()), control)
        .ok()
        .and_then(|child| {
            child
                .capture(1024 * 1024, control, Duration::from_secs(2))
                .ok()
        })
        .filter(|(status, _)| status.success())
        .and_then(|(_, bytes)| String::from_utf8(bytes).ok());
    control.check()?;
    Ok(output)
}

fn system_proxy_enabled(output: &str) -> bool {
    output
        .lines()
        .filter_map(|line| line.split_once(':'))
        .any(|(key, value)| {
            matches!(
                key.trim(),
                "HTTPEnable"
                    | "HTTPSEnable"
                    | "SOCKSEnable"
                    | "ProxyAutoConfigEnable"
                    | "ProxyAutoDiscoveryEnable"
            ) && value.trim() == "1"
        })
}

fn proxy_warning(control: &Control) -> Result<Option<String>> {
    if [
        "http_proxy",
        "https_proxy",
        "all_proxy",
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
    ]
    .iter()
    .any(|key| std::env::var_os(key).is_some_and(|value| !value.is_empty()))
    {
        return Ok(Some(
            "检测到代理环境变量，订阅或测速流量可能经过代理。".into(),
        ));
    }
    Ok(
        match network_output("/usr/sbin/scutil", &["--proxy"], control)? {
            Some(output) if system_proxy_enabled(&output) => {
                Some("macOS 已启用系统代理，订阅或测速流量可能经过代理。".into())
            }
            Some(_) => None,
            None => Some("无法确认 macOS 系统代理状态。".into()),
        },
    )
}

fn subscription_tunnel(routes: &str) -> Option<&str> {
    routes.lines().find_map(|line| {
        let mut columns = line.split_ascii_whitespace();
        let destination = columns.next()?;
        let _gateway = columns.next()?;
        let flags = columns.next()?;
        let interface = columns.next()?;
        // macOS creates scoped IPv6 defaults and link-local/multicast routes on idle utun devices.
        let local = destination == "default" && flags.contains('I')
            || destination.starts_with("fe80:")
            || destination.starts_with("ff")
            || destination.starts_with("169.254")
            || destination
                .split('.')
                .next()
                .and_then(|first| first.parse::<u8>().ok())
                .is_some_and(|first| first >= 224);
        (!local && flags.contains('U') && tunnel_interface(interface)).then_some(interface)
    })
}

fn subscription_route_warning(control: &Control) -> Result<Option<String>> {
    Ok(
        match network_output("/usr/sbin/netstat", &["-rn"], control)? {
            Some(routes) => subscription_tunnel(&routes).map(|interface| {
                format!("系统存在经过 {interface} 的 TUN/VPN 路由，订阅中的候选 IP 可能经过隧道。")
            }),
            None => Some("无法确认订阅候选 IP 是否会经过 TUN/VPN。".into()),
        },
    )
}

fn route_warning(ips: &BTreeSet<IpAddr>, control: &Control) -> Result<Option<String>> {
    let mut incomplete = false;
    for ip in ips {
        control.check()?;
        let family = if ip.is_ipv4() { "-inet" } else { "-inet6" };
        let lookup = Running::start(
            command("/sbin/route")
                .args(["-n", "get", family, &ip.to_string()])
                .stdout(Stdio::piped()),
            control,
        );
        match lookup {
            Ok(mut child) => match child.output(control, Duration::from_secs(1)) {
                Ok((status, output)) if status.success() => {
                    if let Some(interface) = output
                        .lines()
                        .find_map(|line| line.trim_start().strip_prefix("interface:"))
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                    {
                        if tunnel_interface(interface) {
                            return Ok(Some(format!(
                                "测速前提示：{ip} 的系统路由经过隧道接口 {interface}（TUN/VPN）。结果不一定代表直连；若要测直连，请关闭 TUN/VPN 或排除候选 IP 后重试。"
                            )));
                        }
                    } else {
                        incomplete = true;
                    }
                }
                Ok(_) => incomplete = true,
                Err(_) => {
                    incomplete = true;
                    break;
                }
            },
            Err(_) => {
                incomplete = true;
                break;
            }
        }
    }
    control.check()?;
    Ok(incomplete
        .then(|| "测速前提示：部分候选 IP 的系统路由无法确认，测速结果不一定代表直连。".into()))
}

const CFST_HEADER: &str = "IP 地址,已发送,已接收,丢包率,平均延迟,下载速度(MB/s),地区码";

pub fn parse_cfst(input: &str, port: u16, download: bool) -> Result<Vec<Row>> {
    let mut lines = input.trim_start_matches('\u{feff}').lines();
    if lines.next() != Some(CFST_HEADER) {
        return Err("无法识别 CFST 的 CSV 表头".into());
    }
    // shortcut: CFST 2.3.5 emits only addresses, numbers and region codes; use a CSV parser if quoted fields are introduced.
    lines
        .filter(|line| !line.is_empty())
        .map(|line| parse_cfst_row(&line.split(',').collect::<Vec<_>>(), port, download))
        .collect()
}

fn parse_cfst_row(fields: &[&str], port: u16, download: bool) -> Result<Row> {
    let [ip, sent, received, loss, delay, speed, colo] = fields else {
        return Err("CFST CSV 行格式不正确".into());
    };
    let sent = sent.parse()?;
    let received = received.parse()?;
    if sent == 0 || received > sent {
        return Err("CFST CSV 测速次数不正确".into());
    }
    Ok(Row {
        endpoint: SocketAddr::new(ip.parse()?, port),
        notes: Vec::new(),
        sent,
        received,
        loss: number("丢包率", Some(loss), 0.0f64, 0.0, 1.0)?,
        latency: Some(finite(delay)?),
        latency_min: None,
        latency_max: None,
        speed: if download { Some(finite(speed)?) } else { None },
        colo: (!colo.is_empty() && *colo != "N/A").then(|| colo.to_string()),
    })
}

pub fn finite(raw: &str) -> Result<f64> {
    let value: f64 = raw.parse()?;
    if !value.is_finite() || value < 0.0 {
        return Err("测速结果含无效数值".into());
    }
    Ok(value)
}

pub fn sort_cfst(rows: &mut [Row]) {
    rows.sort_by(|a, b| {
        b.speed
            .partial_cmp(&a.speed)
            .unwrap()
            .then_with(|| a.latency.partial_cmp(&b.latency).unwrap())
            .then_with(|| {
                a.endpoint
                    .ip()
                    .to_string()
                    .cmp(&b.endpoint.ip().to_string())
            })
            .then_with(|| a.endpoint.port().cmp(&b.endpoint.port()))
    });
}

fn scan(
    cfst: &Path,
    work: &Path,
    path: &Path,
    options: &Options,
    port: u16,
    ips: impl IntoIterator<Item = IpAddr>,
    control: &Control,
) -> Result<()> {
    let source = work.join("ips.txt");
    let text = ips
        .into_iter()
        .map(|ip| format!("{ip}\n"))
        .collect::<String>();
    fs::write(&source, text)?;
    let status = Running::start(
        command(cfst)
            .current_dir(work)
            .args(["-f"])
            .arg(source)
            .args(["-tp", &port.to_string(), "-o"])
            .arg(path)
            .args(&options.args),
        control,
    )
    .map_err(|e| format!("无法启动 CFST：{e}"))?
    .wait(control, None)?;
    if !status.success() {
        return Err(format!("CFST 测速失败（{status}）").into());
    }
    if !path.try_exists()? {
        // CFST exits successfully without a file when there are no qualifying IPs.
        fs::write(path, format!("{CFST_HEADER}\n"))?;
    }
    Ok(())
}

pub fn retest_one(
    request: SingleRequest,
    engine: &Path,
    control: &Control,
    progress: impl Fn(String) + Sync,
) -> Result<SingleResult> {
    let endpoint = request.endpoint;
    if endpoint.port() == 0 {
        return Err("复测端口无效".into());
    }
    progress("正在检查候选 IP 的路由…".into());
    if let Some(warning) = route_warning(&BTreeSet::from([endpoint.ip()]), control)? {
        control.confirm(|| progress(warning))?;
    }
    let work = PrivateDir::new(std::env::temp_dir().join(format!("cftool-{}", Uuid::new_v4())))?;
    let row = match request.test {
        SingleTest::Cfst { settings } => {
            let options = Options::parse(&settings)?;
            let path = work.path().join("cfst.csv");
            progress(format!("CFST 重测 {endpoint}…"));
            scan(
                engine,
                work.path(),
                &path,
                &options,
                endpoint.port(),
                BTreeSet::from([endpoint.ip()]),
                control,
            )?;
            parse_cfst(
                &fs::read_to_string(path)?,
                endpoint.port(),
                options.download,
            )?
            .into_iter()
            .find(|row| row.endpoint == endpoint)
        }
        SingleTest::Xray { node } => {
            let node = xray::Node::parse(&node)
                .ok_or("VLESS 节点无效或不受支持（仅 VLESS + WS + TLS）")?;
            let rows = xray::retest(
                engine,
                work.path(),
                &[(&node, &[endpoint])],
                control,
                progress,
                |_, _| Ok(()),
            )?;
            Some(
                rows.into_iter()
                    .flatten()
                    .next()
                    .ok_or("Xray 未返回复测结果")?,
            )
        }
    };
    Ok(SingleResult { row })
}

fn parse_nodes(text: &str, engine: &Path) -> Result<Vec<xray::Node>> {
    let nodes = text
        .lines()
        .enumerate()
        .map(|(index, line)| (index, line.trim()))
        .filter(|(_, line)| !line.is_empty())
        .map(|(index, link)| {
            xray::Node::parse(link).ok_or_else(|| {
                format!(
                    "VLESS 第 {} 行无效或不受支持（仅 VLESS + WS + TLS）",
                    index + 1
                )
            })
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if !nodes.is_empty() && !engine.is_file() {
        return Err("缺少 Xray，无法复测".into());
    }
    Ok(nodes)
}

fn confirm_network(
    ips: &BTreeSet<IpAddr>,
    has_subscriptions: bool,
    control: &Control,
    emit: &impl Fn(Event),
) -> Result<()> {
    control.check()?;
    if ips.is_empty() && !has_subscriptions {
        return Ok(());
    }
    emit(stage(Stage::CheckingNetwork));
    let route = if has_subscriptions && let Some(warning) = subscription_route_warning(control)? {
        Some(warning)
    } else {
        route_warning(ips, control)?
    };
    let warnings = route.into_iter().chain(proxy_warning(control)?);
    let message = warnings
        .map(|warning| {
            warning
                .strip_prefix("测速前提示：")
                .unwrap_or(&warning)
                .to_string()
        })
        .collect::<Vec<_>>()
        .join(" ");
    if !message.is_empty() {
        control.confirm(|| emit(Event::RouteWarning { message }))?;
    }
    Ok(())
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub id: String,
    pub groups: Vec<Group>,
}

pub fn run(
    request: Request,
    engines: impl FnOnce() -> (PathBuf, PathBuf),
    root: &Path,
    control: &Control,
    emit: impl Fn(Event) + Sync,
) -> Result<(Report, Option<AppendSource>)> {
    let input = candidates::parse(&request.ips)?;
    let options = Options::parse(&request.settings)?;
    confirm_network(
        &input.ips.iter().map(|candidate| candidate.ip).collect(),
        input.has_subscriptions(),
        control,
        &emit,
    )?;
    control.check()?;
    let _lock = history::lock(root)?;
    let id = history::new_id();
    emit(stage(Stage::PreparingTask));
    control.check()?;
    let (cfst, xray) = engines();
    let nodes = parse_nodes(&request.nodes, &xray)?;
    let ips = input.resolve(|line, url| {
        control.check()?;
        emit(Event::Subscription { line });
        candidates::fetch(url, control)
    })?;
    let nodes_ref = node_refs(&nodes, |node| options.port.unwrap_or(node.port));
    let defaults: BTreeSet<_> = if nodes_ref.is_empty() {
        BTreeSet::from([options.port.unwrap_or(443)])
    } else {
        nodes_ref.iter().map(|node| node.default_port).collect()
    };
    let mut scans = Vec::new();
    for default_port in defaults {
        let mut grouped = BTreeMap::<u16, BTreeMap<IpAddr, BTreeSet<String>>>::new();
        for candidate in &ips {
            let notes = grouped
                .entry(candidate.port.unwrap_or(default_port))
                .or_default()
                .entry(candidate.ip)
                .or_default();
            if !candidate.note.is_empty() {
                notes.insert(candidate.note.clone());
            }
        }
        for (port, items) in grouped {
            scans.push(Scan {
                default_port,
                port,
                items: items
                    .into_iter()
                    .map(|(ip, notes)| CandidateItem {
                        endpoint: SocketAddr::new(ip, port),
                        notes: notes.into_iter().collect(),
                    })
                    .collect(),
                batches: Vec::new(),
                next_size: options.threads,
            });
        }
    }
    let inherited_port = (nodes.is_empty()
        && options.port.is_none()
        && ips.iter().any(|candidate| candidate.port.is_none()))
    .then_some(443);
    let mut record = Record::new(options.snapshot(), inherited_port, scans, nodes_ref);
    let runner = Runner {
        root,
        xray: &xray,
        control,
        emit,
    };
    let results = runner.execute(&id, &mut record, Some(&cfst), index_nodes(nodes))?;
    let source = (record.nodes.is_empty() && results.iter().any(|group| !group.cfst.is_empty()))
        .then_some(AppendSource {
            history_id: id.clone(),
            inherited_port,
        });
    Ok((
        Report {
            id,
            groups: results,
        },
        source,
    ))
}

fn node_refs(nodes: &[xray::Node], default_port: impl Fn(&xray::Node) -> u16) -> Vec<NodeRef> {
    nodes
        .iter()
        .map(|node| NodeRef {
            default_port: default_port(node),
            fingerprint: node.fingerprint(),
        })
        .collect()
}

fn index_nodes(nodes: impl IntoIterator<Item = xray::Node>) -> HashMap<String, xray::Node> {
    nodes
        .into_iter()
        .map(|node| (node.fingerprint(), node))
        .collect()
}

fn local_nodes(text: &str) -> HashMap<String, xray::Node> {
    index_nodes(
        text.lines()
            .filter_map(|link| xray::Node::parse(link.trim())),
    )
}

fn history_node<'a>(
    nodes: &'a HashMap<String, xray::Node>,
    reference: &NodeRef,
    idx: usize,
) -> Result<&'a xray::Node> {
    nodes.get(&reference.fingerprint).ok_or_else(|| {
        format!(
            "历史节点 #{} 已缺失或配置已变更，请在候选 IP 中恢复原 VLESS 节点后重试",
            idx + 1
        )
        .into()
    })
}

pub fn append_xray(
    source: &AppendSource,
    node_text: &str,
    engine: &Path,
    root: &Path,
    control: &Control,
    emit: impl Fn(Event) + Sync,
) -> Result<Vec<Group>> {
    let _lock = history::lock(root)?;
    let nodes = parse_nodes(node_text, engine)?;
    if nodes.is_empty() {
        return Err("请填写 VLESS 节点".into());
    }
    if let Some(port) = source.inherited_port
        && let Some((index, node)) = nodes.iter().enumerate().find(|(_, node)| node.port != port)
    {
        return Err(format!(
            "VLESS 第 {} 个节点的入口端口为 {}，原 CFST 默认端口为 {port}，请重新测速",
            index + 1,
            node.port
        )
        .into());
    }
    control.check()?;
    let mut record = history::load(root, &source.history_id)
        .map_err(|_| "原 CFST 历史已删除或无法读取，请重新测速")?;
    if record.status != Status::Completed
        || !record.nodes.is_empty()
        || !record.groups().iter().any(|group| !group.cfst.is_empty())
    {
        return Err("只能追加有结果且已完成的纯 CFST 测速".into());
    }
    let default = record.scans[0].default_port;
    record.nodes = node_refs(&nodes, |_| default);
    record.appended_xray = true;
    prepare_xray(&mut record);
    confirm_pending_routes(&record, control, &emit)?;
    Runner {
        root,
        xray: engine,
        control,
        emit,
    }
    .execute(&source.history_id, &mut record, None, index_nodes(nodes))
}

pub fn resume(
    id: &str,
    node_text: &str,
    engines: (&Path, &Path),
    root: &Path,
    control: &Control,
    emit: impl Fn(Event) + Sync,
) -> Result<Vec<Group>> {
    let _lock = history::lock(root)?;
    let mut record = history::load(root, id)?;
    if !matches!(
        record.status,
        Status::Running | Status::Paused | Status::Interrupted | Status::Failed
    ) {
        return Err("只有未完成的测速才能继续".into());
    }
    confirm_pending_routes(&record, control, &emit)?;
    Runner {
        root,
        xray: engines.1,
        control,
        emit,
    }
    .execute(id, &mut record, Some(engines.0), local_nodes(node_text))
}

pub fn retest_history(
    root: &Path,
    id: &str,
    endpoint: SocketAddr,
    node: Option<usize>,
    kind: RetestKind,
    node_text: &str,
) -> Result<SingleRequest> {
    let record = history::load(root, id)?;
    if record.status != Status::Completed
        || !record.groups().iter().any(|group| {
            group.node == node && group.cfst.iter().any(|row| row.endpoint == endpoint)
        })
    {
        return Err("只能重测已完成的历史结果".into());
    }
    let test = if matches!(kind, RetestKind::Xray) {
        let index = node
            .and_then(|node| node.checked_sub(1))
            .ok_or("历史记录缺少节点")?;
        let reference = record.nodes.get(index).ok_or("历史记录缺少节点")?;
        let link = node_text
            .lines()
            .map(str::trim)
            .find(|link| {
                xray::Node::parse(link)
                    .is_some_and(|node| node.fingerprint() == reference.fingerprint)
            })
            .ok_or_else(|| {
                format!(
                    "历史节点 #{} 已缺失或配置已变更，请恢复原 VLESS 节点后重试",
                    index + 1
                )
            })?;
        SingleTest::Xray { node: link.into() }
    } else {
        SingleTest::Cfst {
            settings: record.settings,
        }
    };
    Ok(SingleRequest { endpoint, test })
}

fn confirm_pending_routes(record: &Record, control: &Control, emit: &impl Fn(Event)) -> Result<()> {
    let mut ips: BTreeSet<_> = record
        .scans
        .iter()
        .flat_map(|scan| &scan.items[scan.done_end()..])
        .map(|item| item.endpoint.ip())
        .collect();
    if let Some(tests) = &record.retests {
        ips.extend(
            tests
                .iter()
                .flatten()
                .filter(|test| test.row.is_none())
                .map(|test| test.endpoint.ip()),
        );
    } else if !record.nodes.is_empty() {
        // Saved CFST rows can still become Xray targets after a resume.
        ips.extend(
            record
                .scans_by_default()
                .values()
                .flat_map(|rows| rows.iter().take(xray::TOP))
                .map(|row| row.endpoint.ip()),
        );
    }
    confirm_network(&ips, false, control, emit)
}

fn prepare_xray(record: &mut Record) {
    let scans = record.scans_by_default();
    record.retests = Some(
        record
            .nodes
            .iter()
            .map(|node| {
                scans[&node.default_port]
                    .iter()
                    .take(xray::TOP)
                    .map(|row| Retest {
                        endpoint: row.endpoint,
                        row: None,
                    })
                    .collect()
            })
            .collect(),
    );
}

fn batch_size(previous: usize, elapsed: Duration) -> usize {
    const MAX_BATCH: usize = 16_384;
    let desired = (previous as f64 * 10.0 / elapsed.as_secs_f64().max(0.001)).round() as usize;
    desired
        .clamp(previous.div_ceil(2), previous.saturating_mul(2))
        .clamp(1, MAX_BATCH)
}

struct Runner<'a, E> {
    root: &'a Path,
    xray: &'a Path,
    control: &'a Control,
    emit: E,
}

impl<E: Fn(Event) + Sync> Runner<'_, E> {
    fn execute(
        &self,
        id: &str,
        record: &mut Record,
        cfst: Option<&Path>,
        nodes: HashMap<String, xray::Node>,
    ) -> Result<Vec<Group>> {
        self.control.check()?;
        record.status = Status::Running;
        history::save(self.root, id, record, Some((self.control, false)))?;
        let result = self.run_tasks(id, record, cfst, &nodes).and_then(|()| {
            (self.emit)(stage(Stage::SavingResults));
            record.status = Status::Completed;
            history::save(self.root, id, record, Some((self.control, true)))?;
            (self.emit)(stage(Stage::Completed));
            Ok(record.groups())
        });
        if result.is_err() {
            let stopped = self.control.cancelled();
            record.status = if stopped {
                Status::Paused
            } else {
                Status::Failed
            };
            history::save(self.root, id, record, None).map_err(history::WriteError)?;
            (self.emit)(stage(if stopped {
                Stage::Paused
            } else {
                Stage::Failed
            }));
        }
        result
    }

    fn run_tasks(
        &self,
        id: &str,
        record: &mut Record,
        cfst: Option<&Path>,
        nodes: &HashMap<String, xray::Node>,
    ) -> Result<()> {
        let emit = &self.emit;
        let options = Options::parse(&record.settings)?;
        let mut done: usize = record.scans.iter().map(Scan::done_end).sum();
        let total = record.scans.iter().map(|scan| scan.items.len()).sum();
        if done < total {
            emit(Event::Stage {
                stage: Stage::Cfst,
                progress: Some([done, total]),
            });
        } else if record.nodes.is_empty() {
            emit(stage(Stage::SavingResults));
        } else {
            emit(stage(Stage::PreparingXray));
        }
        emit(Event::Candidates {
            history_id: id.into(),
            groups: record.candidates(),
        });
        for scan in &record.scans {
            let done = scan.done_end();
            if done > 0 {
                emit(Event::CfstBatchDone {
                    default_port: scan.default_port,
                    endpoints: scan.items[..done]
                        .iter()
                        .map(|item| item.endpoint)
                        .collect(),
                    rows: scan
                        .batches
                        .iter()
                        .filter_map(|batch| batch.rows.as_ref())
                        .flatten()
                        .cloned()
                        .collect(),
                });
            }
        }

        self.control.check()?;
        let work =
            PrivateDir::new(std::env::temp_dir().join(format!("cftool-{}", Uuid::new_v4())))?;
        for index in 0..record.scans.len() {
            loop {
                self.control.check()?;
                let scan = &mut record.scans[index];
                let start = scan.done_end();
                if start == scan.items.len() {
                    break;
                }
                let Some(cfst) = cfst else {
                    return Err("原 CFST 批次尚未完成".into());
                };
                let (default_port, port) = (scan.default_port, scan.port);
                let end = match scan.batches.last() {
                    Some(batch) if batch.rows.is_none() => batch.end,
                    _ => {
                        let end = if options.download {
                            scan.items.len()
                        } else {
                            start.saturating_add(scan.next_size).min(scan.items.len())
                        };
                        scan.batches.push(Batch { end, rows: None });
                        history::save(self.root, id, record, None)?;
                        end
                    }
                };
                let items = &record.scans[index].items[start..end];
                let endpoints: Vec<_> = items.iter().map(|item| item.endpoint).collect();
                emit(Event::CfstBatchStart {
                    default_port,
                    endpoints: endpoints.clone(),
                });
                let path = work.path().join("cfst.csv");
                if path.try_exists()? {
                    fs::remove_file(&path)?;
                }
                let started = Instant::now();
                self::scan(
                    cfst,
                    work.path(),
                    &path,
                    &options,
                    port,
                    endpoints.iter().map(SocketAddr::ip),
                    self.control,
                )?;
                let elapsed = started.elapsed();
                let mut rows = parse_cfst(&fs::read_to_string(&path)?, port, options.download)?;
                let mut expected: BTreeMap<_, _> = items
                    .iter()
                    .map(|item| (item.endpoint, &item.notes))
                    .collect();
                for row in &mut rows {
                    row.notes = expected
                        .remove(&row.endpoint)
                        .ok_or("CFST 返回未知或重复的入口")?
                        .clone();
                }
                let scan = &mut record.scans[index];
                scan.next_size = batch_size(end - start, elapsed);
                scan.batches.last_mut().unwrap().rows = Some(rows.clone());
                history::save(self.root, id, record, None)?;
                emit(Event::CfstBatchDone {
                    default_port,
                    endpoints,
                    rows,
                });
                done += end - start;
                emit(Event::Stage {
                    stage: Stage::Cfst,
                    progress: Some([done, total]),
                });
            }
        }

        if record.retests.is_none() {
            if !record.nodes.is_empty() {
                emit(stage(Stage::PreparingXray));
            }
            prepare_xray(record);
            history::save(self.root, id, record, None)?;
        }
        self.run_retests(id, record, nodes, work.path())
    }

    fn run_retests(
        &self,
        id: &str,
        record: &mut Record,
        nodes: &HashMap<String, xray::Node>,
        work: &Path,
    ) -> Result<()> {
        let emit = &self.emit;
        let tests = record.retests.as_ref().unwrap();
        emit(Event::XrayTargets {
            targets: tests
                .iter()
                .enumerate()
                .map(|(idx, tests)| XrayNodeTargets {
                    node: idx + 1,
                    endpoints: tests.iter().map(|test| test.endpoint).collect(),
                })
                .collect(),
        });
        let mut remaining = Vec::new();
        for (idx, tests) in tests.iter().enumerate() {
            let mut endpoints = Vec::new();
            for test in tests {
                if let Some(row) = &test.row {
                    emit(Event::XrayUpdate {
                        node: idx + 1,
                        row: row.clone(),
                    });
                } else {
                    endpoints.push(test.endpoint);
                }
            }
            if !endpoints.is_empty() {
                remaining.push((
                    idx,
                    history_node(nodes, &record.nodes[idx], idx)?,
                    endpoints,
                ));
            }
        }
        if remaining.is_empty() {
            return Ok(());
        }
        if !self.xray.is_file() {
            return Err("缺少 Xray，无法复测".into());
        }
        let scans = record.scans_by_default();
        let ports: Vec<_> = record.nodes.iter().map(|node| node.default_port).collect();
        let targets: Vec<_> = remaining
            .iter()
            .map(|(_, node, endpoints)| (*node, endpoints.as_slice()))
            .collect();
        let total: usize = tests.iter().map(Vec::len).sum();
        let done = total
            - remaining
                .iter()
                .map(|(_, _, targets)| targets.len())
                .sum::<usize>();
        emit(Event::Stage {
            stage: Stage::Xray,
            progress: Some([done, total]),
        });
        let saved = Mutex::new((record, done));
        xray::retest(
            self.xray,
            work,
            &targets,
            self.control,
            |_| (),
            |index, mut row| {
                let idx = remaining[index].0;
                if let Some(cfst) = scans[&ports[idx]]
                    .iter()
                    .find(|cfst| cfst.endpoint == row.endpoint)
                {
                    row.notes.clone_from(&cfst.notes);
                }
                if row.sent == xray::ROUNDS {
                    let (record, done) = &mut *saved.lock().unwrap();
                    let test = record.retests.as_mut().unwrap()[idx]
                        .iter_mut()
                        .find(|test| test.endpoint == row.endpoint)
                        .ok_or("Xray 返回未知的入口")?;
                    test.row = Some(row.clone());
                    history::save(self.root, id, record, None)?;
                    *done += 1;
                    emit(Event::Stage {
                        stage: Stage::Xray,
                        progress: Some([*done, total]),
                    });
                }
                emit(Event::XrayUpdate { node: idx + 1, row });
                Ok(())
            },
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancelling_network_preflight_for_subscription_only_does_not_create_a_task() {
        let temp = PrivateDir::new(std::env::temp_dir().join(Uuid::new_v4().to_string())).unwrap();
        let root = temp.path().join("history");
        let control = Control::default();
        let Err(error) = run(
            Request {
                ips: "https://example.invalid/ips".into(),
                nodes: String::new(),
                settings: HashMap::new(),
            },
            || panic!("preflight must run before selecting engines"),
            &root,
            &control,
            |event| match event {
                Event::Stage {
                    stage: Stage::CheckingNetwork,
                    ..
                } => {
                    control.cancel();
                }
                _ => panic!("unexpected event before preflight completed: {event:?}"),
            },
        ) else {
            panic!("cancelled preflight started a probe");
        };
        assert_eq!(error.to_string(), "测速已取消");
        assert!(!root.exists());
    }

    #[test]
    fn checks_form_before_preflight_and_creates_task_before_fetching() {
        let temp = PrivateDir::new(std::env::temp_dir().join(Uuid::new_v4().to_string())).unwrap();
        let root = temp.path().join("history");
        let control = Control::default();
        let request = |ips: &str| Request {
            ips: ips.into(),
            nodes: String::new(),
            settings: HashMap::new(),
        };
        let Err(error) = run(
            request("https://example.invalid/ips\nnot an IP"),
            || panic!("invalid input started a task"),
            &root,
            &control,
            |_| panic!("preflight must not run for invalid input"),
        ) else {
            panic!("invalid input was accepted");
        };
        assert!(error.to_string().contains("第 2 行"));
        assert!(!root.exists());

        let events = Mutex::new(Vec::new());
        assert!(
            run(
                request("https://example.invalid/ips"),
                || (PathBuf::new(), PathBuf::new()),
                &root,
                &control,
                |event| match event {
                    Event::RouteWarning { .. } => assert!(control.approve()),
                    Event::Stage { .. } => events.lock().unwrap().push(event),
                    Event::Subscription { line: 1 } => {
                        assert!(root.join(".lock").exists());
                        control.cancel(); // Stop before invoking curl.
                        events.lock().unwrap().push(event);
                    }
                    _ => panic!("started CFST before fetching and validating subscriptions"),
                },
            )
            .is_err()
        );
        assert!(control.cancelled());
        assert_eq!(
            events.into_inner().unwrap(),
            [
                stage(Stage::CheckingNetwork),
                stage(Stage::PreparingTask),
                Event::Subscription { line: 1 },
            ]
        );
        assert!(history::list(&root).unwrap().is_empty());
    }

    #[test]
    fn detects_enabled_proxies_and_non_scoped_tunnel_routes() {
        assert!(system_proxy_enabled("  HTTPEnable : 0\n SOCKSEnable : 1"));
        assert!(system_proxy_enabled("  ProxyAutoConfigEnable : 1"));
        assert!(!system_proxy_enabled(" HTTPEnable : 0\n HTTPPort : 1"));
        let idle = "default fe80::%utun0 UGcIg utun0\nfe80::%utun0/64 fe80::1 UCI utun0\nff00::/8 fe80::1 UmCI utun0\ndefault 192.168.1.1 UGScg en0";
        assert_eq!(subscription_tunnel(idle), None);
        assert_eq!(
            subscription_tunnel(&format!("{idle}\n 128.0/1 198.18.0.1 UGSc utun4")),
            Some("utun4")
        );
        assert_eq!(
            subscription_tunnel("2000::/3 fe80::1 UGSc utun5"),
            Some("utun5")
        );
    }

    #[test]
    fn resumes_the_same_adaptive_cfst_batch_without_repeating_an_empty_batch() {
        use std::{
            os::unix::fs::PermissionsExt,
            sync::atomic::{AtomicUsize, Ordering},
        };

        let temp = PrivateDir::new(std::env::temp_dir().join(Uuid::new_v4().to_string())).unwrap();
        let root = temp.path().join("history");
        let cfst = temp.path().join("cfst");
        fs::write(
            &cfst,
            format!(
                r#"#!/bin/sh
while [ "$#" -gt 0 ]; do
  case "$1" in
    -f) input="$2" ;;
    -o) output="$2" ;;
  esac
  shift
done
printf '%s\n' '{CFST_HEADER}' > "$output"
while read -r ip; do
  case "$ip" in 127.0.0.1|127.0.0.2) continue ;; esac
  printf '%s,3,2,0.33333333333333337,2.15,1.25,HKG\n' "$ip" >> "$output"
done < "$input"
"#
            ),
        )
        .unwrap();
        fs::set_permissions(&cfst, fs::Permissions::from_mode(0o700)).unwrap();

        let ips = (1..=7)
            .map(|n| format!("127.0.0.{n} # 备注 {n}"))
            .collect::<Vec<_>>()
            .join("\n");
        let settings = HashMap::from([("n".into(), "2".into())]);
        let stopped = Control::default();
        let started = AtomicUsize::new(0);
        assert!(
            run(
                Request {
                    ips: ips.clone(),
                    nodes: String::new(),
                    settings: settings.clone()
                },
                || (cfst.clone(), cfst.clone()),
                &root,
                &stopped,
                |event| {
                    if matches!(event, Event::RouteWarning { .. }) {
                        stopped.approve();
                    }
                    if matches!(event, Event::CfstBatchStart { .. })
                        && started.fetch_add(1, Ordering::Relaxed) == 1
                    {
                        stopped.cancel();
                    }
                }
            )
            .is_err()
        );
        let entries = history::list(&root).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].status, Status::Paused);
        let id = &entries[0].id;
        let mut record = history::load(&root, id).unwrap();
        let scan = &record.scans[0];
        assert_eq!(scan.done_end(), 2);
        assert_eq!(scan.batches[0].rows.as_deref(), Some(&[][..]));
        assert_eq!(scan.batches[1].end, 6);
        assert!(scan.batches[1].rows.is_none());
        record.status = Status::Running; // The last durable state after a hard exit.
        history::save(&root, id, &record, None).unwrap();
        assert_eq!(history::list(&root).unwrap()[0].status, Status::Interrupted);

        let resumed = Control::default();
        let starts = Mutex::new(Vec::new());
        let result = resume(
            id,
            "",
            (&cfst, &cfst),
            &root,
            &resumed,
            |event| match event {
                Event::RouteWarning { .. } => {
                    resumed.approve();
                }
                Event::CfstBatchStart { endpoints, .. } => starts.lock().unwrap().push(endpoints),
                _ => (),
            },
        )
        .unwrap();
        let starts = starts.into_inner().unwrap();
        let replayed: Vec<_> = starts
            .iter()
            .flatten()
            .map(|endpoint| endpoint.ip().to_string())
            .collect();
        assert_eq!(starts.iter().map(Vec::len).collect::<Vec<_>>(), [4, 1]);
        assert_eq!(
            replayed,
            [
                "127.0.0.3",
                "127.0.0.4",
                "127.0.0.5",
                "127.0.0.6",
                "127.0.0.7"
            ]
        );
        assert_eq!(history::list(&root).unwrap()[0].id, *id);
        assert_eq!(history::list(&root).unwrap()[0].status, Status::Completed);
        assert_eq!(history::load(&root, id).unwrap().groups(), result);
        assert_eq!(fs::read_dir(&root).unwrap().count(), 2); // History JSON and writer lock only.
        let first = &result[0].cfst[0];
        assert_eq!(first.colo.as_deref(), Some("HKG"));
        assert_eq!(first.notes, ["备注 3"]);
        let single = retest_history(&root, id, first.endpoint, None, RetestKind::Cfst, "").unwrap();
        let SingleTest::Cfst { settings: saved } = &single.test else {
            panic!("expected CFST")
        };
        assert_eq!(saved["n"], "2");
        let retest = Control::default();
        retest_one(single, &cfst, &retest, |message| {
            if message.starts_with("测速前提示：") {
                retest.approve();
            }
        })
        .unwrap();
        assert_eq!(history::load(&root, id).unwrap().groups(), result);
        assert_eq!(batch_size(200, Duration::from_secs(40)), 100);
        assert_eq!(batch_size(200, Duration::from_secs(8)), 250);

        let download = Control::default();
        let starts = AtomicUsize::new(0);
        run(
            Request {
                ips,
                nodes: String::new(),
                settings: HashMap::from_iter(
                    settings
                        .into_iter()
                        .chain([("download".into(), "yes".into())]),
                ),
            },
            || (cfst.clone(), cfst.clone()),
            &root,
            &download,
            |event| match event {
                Event::RouteWarning { .. } => {
                    download.approve();
                }
                Event::CfstBatchStart { endpoints, .. } => {
                    assert_eq!(endpoints.len(), 7);
                    starts.fetch_add(1, Ordering::Relaxed);
                }
                _ => (),
            },
        )
        .unwrap();
        assert_eq!(starts.load(Ordering::Relaxed), 1);
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn resumes_appended_xray_without_rerunning_cfst_or_storing_node_credentials() {
        use std::net::TcpListener;

        let tmp = PrivateDir::new(std::env::temp_dir().join(Uuid::new_v4().to_string())).unwrap();
        let root = tmp.path();
        let first = TcpListener::bind("127.0.0.1:0").unwrap();
        let second = TcpListener::bind("127.0.0.1:0").unwrap();
        let a = first.local_addr().unwrap();
        let b = second.local_addr().unwrap();
        let binaries = Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/bin");
        let cfst = binaries.join("cfst");
        let xray = binaries.join("xray");
        let control = Control::default();
        let (report, source) = run(
            Request {
                ips: format!("{a} # 一\n{b} # 二"),
                nodes: String::new(),
                settings: HashMap::from([("n".into(), "1".into()), ("t".into(), "1".into())]),
            },
            || (cfst.clone(), xray.clone()),
            root,
            &control,
            |event| {
                if matches!(event, Event::RouteWarning { .. }) {
                    control.approve();
                }
            },
        )
        .unwrap();
        let source = source.unwrap();
        let original = report.groups;
        assert_eq!(report.id, source.history_id);
        assert_eq!(original[0].cfst.len(), 2);
        drop((first, second)); // A finished 0/3 result is still a checkpoint.
        let credential = "98765432-0000-4000-8000-000000000001";
        let node = format!("vless://{credential}@example.invalid:8443?type=ws&security=tls");
        let path = root.join(format!("{}.json", report.id));
        let original_json = fs::read(&path).unwrap();
        let cancelled = Control::default();
        assert!(
            append_xray(&source, &node, &xray, root, &cancelled, |event| {
                assert_eq!(event, stage(Stage::CheckingNetwork));
                cancelled.cancel();
            })
            .is_err()
        );
        assert_eq!(fs::read(&path).unwrap(), original_json);
        let stopping = Control::default();
        assert!(
            append_xray(
                &source,
                &node,
                &xray,
                root,
                &stopping,
                |event| match event {
                    Event::RouteWarning { .. } => {
                        stopping.approve();
                    }
                    Event::XrayUpdate { row, .. } if row.sent == xray::ROUNDS => stopping.cancel(),
                    Event::CfstBatchStart { .. } => panic!("append must not run CFST"),
                    _ => (),
                }
            )
            .is_err()
        );
        let entries = history::list(root).unwrap();
        assert_eq!(entries.len(), 1);
        let id = &entries[0].id;
        assert_eq!(id, &source.history_id);
        let partial = history::load(root, id).unwrap();
        assert_eq!(partial.status, Status::Paused);
        assert!(partial.appended_xray && partial.source_id.is_none());
        let partial_groups = partial.groups();
        assert_eq!(partial_groups[0], original[0]);
        let completed = &partial_groups[1].xray[0];
        assert_eq!((completed.sent, completed.received), (3, 0));
        assert_eq!(partial_groups[1].xray.len(), 1);
        let missing = Control::default();
        assert!(
            resume(id, "", (&cfst, &xray), root, &missing, |event| {
                if matches!(event, Event::RouteWarning { .. }) {
                    missing.approve();
                }
            })
            .unwrap_err()
            .to_string()
            .contains("已缺失")
        );

        let control = Control::default();
        let stages = Mutex::new(Vec::new());
        let combined = resume(
            id,
            &node,
            (&cfst, &xray),
            root,
            &control,
            |event| match event {
                Event::RouteWarning { .. } => {
                    control.approve();
                }
                Event::CfstBatchStart { .. } => panic!("resume must not rerun CFST"),
                Event::Stage { .. } => stages.lock().unwrap().push(event),
                Event::XrayUpdate { row, .. } if row.endpoint == completed.endpoint => {
                    assert_eq!(row, *completed)
                }
                _ => (),
            },
        )
        .unwrap();
        assert_eq!(
            stages.into_inner().unwrap(),
            [
                stage(Stage::CheckingNetwork),
                stage(Stage::PreparingXray),
                Event::Stage {
                    stage: Stage::Xray,
                    progress: Some([1, 2])
                },
                Event::Stage {
                    stage: Stage::Xray,
                    progress: Some([2, 2])
                },
                stage(Stage::SavingResults),
                stage(Stage::Completed),
            ]
        );
        assert_eq!(combined[0], original[0]);
        assert_eq!(combined[1].xray.len(), 2);
        assert_eq!(history::load(root, id).unwrap().groups(), combined);
        let duplicate = Control::default();
        assert!(
            append_xray(&source, &node, &xray, root, &duplicate, |_| panic!(
                "already appended"
            ))
            .is_err()
        );
        let json = fs::read_to_string(root.join(format!("{id}.json"))).unwrap();
        assert!(!json.contains(credential) && !json.contains("vless://"));
        assert!(retest_history(root, id, a, Some(1), RetestKind::Xray, &node).is_ok());
    }

    #[test]
    fn validates_candidate_addresses_and_cfst_settings() {
        let ips = candidates::parse("127.0.0.1 # note\n[2001:db8::1]:8443\n2001:db8::2")
            .unwrap()
            .ips;
        assert_eq!(ips[0].note, "note");
        assert_eq!(ips[1].ip, "2001:db8::1".parse::<IpAddr>().unwrap());
        assert_eq!(ips[1].port, Some(8443));
        assert_eq!(ips[2].port, None);
        for input in [
            "1.1.1.1:0",
            "[fe80::1%0]:443",
            "http://example.com",
            "# empty",
        ] {
            assert!(candidates::parse(input).is_err());
        }
        assert!(Options::parse(&HashMap::from([("tlr".into(), "NaN".into())])).is_err());
        assert!(Options::parse(&HashMap::from([("tll".into(), "10000".into())])).is_err());
    }

    #[test]
    fn resolves_https_subscriptions_without_trusting_their_contents() {
        let url = "https://example.invalid/ips?key=secret";
        let body = "\u{feff}104.25.245.128:443#微测优选 | 移动 | HKG | 104.25.245.128\r\n[2606:4700::1111]:8443#CFYes优选 | 电信 | [2606:4700::1111]\r\n";
        let input = candidates::parse(&format!("127.0.0.1\n{url} # note\n{url}")).unwrap();
        assert_eq!(input.ips.len(), 1);
        assert!(input.has_subscriptions());
        let mut downloads = 0;
        let ips = input
            .resolve(|line, received| {
                downloads += 1;
                assert_eq!(line, 2);
                assert_eq!(received.as_str(), url);
                Ok(body.into())
            })
            .unwrap();
        assert_eq!(downloads, 1);
        assert_eq!(ips.len(), 3);
        assert_eq!(ips[0].ip, "127.0.0.1".parse::<IpAddr>().unwrap());
        assert_eq!(ips[1].port, Some(443));
        assert_eq!(ips[1].note, "微测优选 | 移动 | HKG | 104.25.245.128");
        assert_eq!(ips[2].port, Some(8443));
        assert_eq!(
            candidates::import_ip_subscription(body).unwrap().join("\n"),
            body.trim_start_matches('\u{feff}')
                .trim_end()
                .replace("\r\n", "\n")
        );

        for entry in [
            "127.0.0.1",
            "192.168.1.1:443",
            "[::ffff:127.0.0.1]:443",
            "ff02::1",
            "https://example.invalid/other",
        ] {
            let err = candidates::parse(url)
                .unwrap()
                .resolve(|_, _| Ok(format!("1.1.1.1\n{entry}")))
                .unwrap_err()
                .to_string();
            assert!(err.contains("第 1 行：订阅内容第 2 行"), "{err}");
            assert!(!err.contains("secret"));
        }
        assert!(
            candidates::parse(url)
                .unwrap()
                .resolve(|_, _| Ok("# no candidates".into()))
                .is_err()
        );
        assert!(
            candidates::parse(url)
                .unwrap()
                .resolve(|_, _| Ok("1.1.1.1\n".repeat(1025)))
                .is_err()
        );
        assert!(candidates::parse("https://user:pass@example.invalid/ips").is_err());
    }
}
