use crate::{
    probe::{Row, finite},
    process::{Control, Result, Running, command},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs::OpenOptions,
    io,
    net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream},
    os::unix::fs::OpenOptionsExt,
    path::Path,
    process::Stdio,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
use url::Url;
use uuid::Uuid;

pub const ROUNDS: u32 = 3;
pub const TOP: usize = 10;
const MAX_PARALLEL: usize = 4;
const TARGETS_PER_LANE: usize = 3;
const URL: &str = "https://www.gstatic.com/generate_204";

pub struct Node {
    pub port: u16,
    id: Uuid,
    stream: Value,
}

impl Node {
    pub fn parse(link: &str) -> Option<Self> {
        if link.chars().any(char::is_whitespace) {
            return None;
        }
        let url = Url::parse(link).ok()?;
        if url.scheme() != "vless" || url.password().is_some() || !url.path().is_empty() {
            return None;
        }
        let id = Uuid::parse_str(url.username()).ok()?;
        let host = url.host_str()?.trim_matches(['[', ']']);
        let port = url.port().unwrap_or(443);
        if port == 0 || host.is_empty() || url.query()?.split('&').any(|part| !part.contains('=')) {
            return None;
        }
        let mut qs = HashMap::new();
        for (key, value) in url.query_pairs() {
            if !matches!(
                key.as_ref(),
                "type"
                    | "security"
                    | "encryption"
                    | "flow"
                    | "sni"
                    | "host"
                    | "path"
                    | "alpn"
                    | "fp"
                    | "ech"
            ) || qs.insert(key, value).is_some()
            {
                return None;
            }
        }
        let get = |key: &str| qs.get(key).map(|v| v.as_ref());
        if get("type") != Some("ws")
            || get("security") != Some("tls")
            || get("encryption").unwrap_or("none") != "none"
            || get("flow").is_some_and(|flow| !flow.is_empty())
        {
            return None;
        }
        let nonempty = |key: &str| get(key).filter(|v| !v.is_empty());
        let sni = nonempty("sni").unwrap_or(host);
        let mut tls = json!({"serverName": sni});
        if let Some(alpn) = nonempty("alpn") {
            tls["alpn"] = json!(alpn.split(',').collect::<Vec<_>>());
        }
        if let Some(fp) = nonempty("fp") {
            tls["fingerprint"] = json!(fp);
        }
        if let Some(ech) = get("ech") {
            if ech.is_empty() || ech.chars().any(|c| c.is_whitespace() || c.is_control()) {
                return None;
            }
            tls["echConfigList"] = json!(ech);
        }
        let stream = json!({
            "network": "ws", "security": "tls", "tlsSettings": tls,
            "wsSettings": {"host": nonempty("host").unwrap_or(sni), "path": nonempty("path").unwrap_or("/")},
        });
        Some(Self { port, id, stream })
    }

    pub fn fingerprint(&self) -> String {
        let mut hash = Sha256::new();
        hash.update(self.id.as_bytes());
        hash.update(self.port.to_be_bytes());
        hash.update(self.stream.to_string());
        format!("{:x}", hash.finalize())
    }

    fn outbound(&self, endpoint: SocketAddr, tag: &str) -> Value {
        json!({
            "tag": tag, "protocol": "vless", "settings": {"vnext": [{
                "address": endpoint.ip(), "port": endpoint.port(),
                "users": [{"id": self.id.to_string(), "encryption": "none"}],
            }]}, "streamSettings": self.stream,
        })
    }
}

struct Target {
    node: usize,
    endpoint: SocketAddr,
}

fn group_targets(nodes: &[(&Node, &[SocketAddr])]) -> Vec<Vec<Target>> {
    let count: usize = nodes
        .iter()
        .map(|(_, endpoints)| endpoints.len().min(TOP))
        .sum();
    if count == 0 {
        return Vec::new();
    }
    let parallel = (count / TARGETS_PER_LANE).clamp(1, MAX_PARALLEL);
    let mut lanes: Vec<_> = (0..parallel).map(|_| Vec::new()).collect();
    let targets = nodes.iter().enumerate().flat_map(|(node, (_, endpoints))| {
        endpoints
            .iter()
            .take(TOP)
            .map(move |&endpoint| Target { node, endpoint })
    });
    for (index, target) in targets.enumerate() {
        lanes[index % parallel].push(target);
    }
    lanes
}

fn route_user(lane: usize, index: usize) -> String {
    format!("target-{lane}-{index}")
}

fn batch_config(
    nodes: &[(&Node, &[SocketAddr])],
    lanes: &[Vec<Target>],
    locals: &[SocketAddr],
    password: &str,
) -> Value {
    let mut inbounds = Vec::new();
    let mut outbounds = vec![json!({"protocol": "blackhole", "tag": "reject"})];
    let mut rules = Vec::new();
    for (lane, (targets, local)) in lanes.iter().zip(locals).enumerate() {
        let inbound = format!("lane-{lane}");
        let mut accounts = Vec::new();
        for (index, target) in targets.iter().enumerate() {
            let user = route_user(lane, index);
            // SOCKS authentication selects an outbound without changing the HTTPS destination.
            accounts.push(json!({"user": user, "pass": password}));
            rules.push(json!({"inboundTag": [inbound], "user": [user], "outboundTag": user}));
            outbounds.push(nodes[target.node].0.outbound(target.endpoint, &user));
        }
        inbounds.push(json!({
            "tag": inbound, "listen": local.ip(), "port": local.port(), "protocol": "socks",
            "settings": {"auth": "password", "accounts": accounts},
        }));
    }
    json!({
        "log": {"loglevel": "none"}, "inbounds": inbounds, "outbounds": outbounds,
        "routing": {"rules": rules},
    })
}

fn measurement(output: &str) -> Option<f64> {
    let (status, seconds) = output.trim().split_once(' ')?;
    if status != "204" {
        return None;
    }
    let ms = finite(seconds).ok()? * 1000.0;
    ms.is_finite().then_some(ms)
}

fn probe_204(
    work: &Path,
    proxy: &str,
    auth: &str,
    url: &str,
    control: &Control,
) -> Result<Option<f64>> {
    let output = Running::start(
        command("/usr/bin/curl")
            .current_dir(work)
            .args([
                "-q",
                "-s",
                "--config",
                "-",
                "--noproxy",
                "",
                "--proxy",
                proxy,
                "--socks5-basic",
                "--max-time",
                "8",
                "-o",
                "/dev/null",
                "-w",
                "%{http_code} %{time_total}",
                url,
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped()),
        control,
    )?
    .exchange(auth.as_bytes(), 128, control, Duration::from_secs(9));
    match output {
        Ok((status, stdout)) if status.success() => Ok(measurement(std::str::from_utf8(&stdout)?)),
        Ok(_) => Ok(None),
        Err(e)
            if e.downcast_ref::<io::Error>()
                .is_some_and(|e| e.kind() == io::ErrorKind::TimedOut) =>
        {
            Ok(None)
        }
        Err(e) => Err(e),
    }
}

fn test_lane(
    targets: &[Target],
    control: &Control,
    failed: &AtomicBool,
    mut probe: impl FnMut(usize) -> Result<Option<f64>>,
    mut on_update: impl FnMut(&Target, Row) -> Result<()>,
) -> Result<Vec<Row>> {
    let mut samples = vec![Vec::new(); targets.len()];
    for round in 0..ROUNDS {
        if round > 0 && targets.len() == 1 {
            control.sleep(Duration::from_secs(1))?;
        }
        for (index, samples) in samples.iter_mut().enumerate() {
            control.check()?;
            if failed.load(Ordering::Relaxed) {
                return Ok(Vec::new());
            }
            samples.extend(probe(index)?);
            on_update(
                &targets[index],
                summarize_samples(targets[index].endpoint, round + 1, samples),
            )?;
        }
    }
    Ok(targets
        .iter()
        .zip(samples)
        .map(|(target, samples)| summarize(target.endpoint, samples))
        .collect())
}

fn summarize_samples(endpoint: SocketAddr, sent: u32, samples: &[f64]) -> Row {
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    let received = sorted.len() as u32;
    let mid = sorted.len() / 2;
    let latency = if sorted.is_empty() {
        None
    } else if sorted.len().is_multiple_of(2) {
        Some(f64::midpoint(sorted[mid - 1], sorted[mid]))
    } else {
        Some(sorted[mid])
    };
    Row {
        endpoint,
        sent,
        received,
        loss: 1.0 - f64::from(received) / f64::from(sent),
        latency,
        latency_min: sorted.first().copied(),
        latency_max: sorted.last().copied(),
        speed: None,
        colo: None,
        notes: Vec::new(),
    }
}

fn summarize(endpoint: SocketAddr, samples: Vec<f64>) -> Row {
    summarize_samples(endpoint, ROUNDS, &samples)
}

fn start_batch(
    xray: &Path,
    work: &Path,
    nodes: &[(&Node, &[SocketAddr])],
    lanes: &[Vec<Target>],
    password: &str,
    control: &Control,
) -> Result<(Running, Vec<SocketAddr>)> {
    control.check()?;
    let listeners = (0..lanes.len())
        .map(|_| TcpListener::bind((Ipv4Addr::LOCALHOST, 0)))
        .collect::<io::Result<Vec<_>>>()?;
    let locals = listeners
        .iter()
        .map(TcpListener::local_addr)
        .collect::<io::Result<Vec<_>>>()?;
    let config_path = work.join("xray.json");
    let config = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&config_path)?;
    serde_json::to_writer(config, &batch_config(nodes, lanes, &locals, password))?;
    // shortcut: Xray binds its own sockets; retry with fresh ports if local bind races become common.
    drop(listeners);
    let mut engine = Running::start(
        command(xray)
            .current_dir(work)
            .args(["run", "-c"])
            .arg(config_path),
        control,
    )
    .map_err(|e| format!("无法启动 Xray：{e}"))?;
    let started = Instant::now();
    for local in &locals {
        while TcpStream::connect_timeout(local, Duration::from_millis(100)).is_err() {
            if engine.exited()? || started.elapsed() >= Duration::from_secs(5) {
                return Err("Xray 启动失败，请检查端口、版本和节点参数".into());
            }
            control.sleep(Duration::from_millis(40))?;
        }
    }
    if engine.exited()? {
        return Err("Xray 启动失败，请检查端口、版本和节点参数".into());
    }
    Ok((engine, locals))
}

pub fn retest(
    xray: &Path,
    work: &Path,
    nodes: &[(&Node, &[SocketAddr])],
    control: &Control,
    progress: impl Fn(String) + Sync,
    on_update: impl Fn(usize, Row) -> Result<()> + Sync,
) -> Result<Vec<Vec<Row>>> {
    let mut results = vec![Vec::new(); nodes.len()];
    let lanes = group_targets(nodes);
    if lanes.is_empty() {
        return Ok(results);
    }
    let password = Uuid::new_v4().simple().to_string();
    let total = lanes.iter().map(Vec::len).sum::<usize>() * ROUNDS as usize;
    let parallel = lanes.len();
    let report = |done| progress(format!("Xray 复测中…（{parallel} 路，{done}/{total} 次）"));
    report(0);
    let (engine, locals) = start_batch(xray, work, nodes, &lanes, &password, control)?;
    let state = Mutex::new((engine, 0));
    let failed = AtomicBool::new(false);
    thread::scope(|scope| -> Result<()> {
        let workers: Vec<_> = lanes
            .iter()
            .zip(locals)
            .enumerate()
            .map(|(lane, (targets, local))| {
                let (state, failed, password, report, on_update) =
                    (&state, &failed, &password, &report, &on_update);
                scope.spawn(move || {
                    let proxy = format!("socks5h://{local}");
                    let rows = test_lane(
                        targets,
                        control,
                        failed,
                        |index| {
                            let auth = format!(
                                "proxy-user = \"{}:{password}\"\n",
                                route_user(lane, index)
                            );
                            let sample = probe_204(work, &proxy, &auth, URL, control)?;
                            let (engine, done) = &mut *state.lock().unwrap();
                            if engine.exited()? {
                                return Err("测试中 Xray 意外退出".into());
                            }
                            *done += 1;
                            report(*done);
                            Ok(sample)
                        },
                        |target, row| on_update(target.node, row),
                    );
                    if rows.is_err() {
                        failed.store(true, Ordering::Relaxed);
                    }
                    rows
                })
            })
            .collect();
        for (targets, worker) in lanes.iter().zip(workers) {
            for (target, row) in targets.iter().zip(worker.join().unwrap()?) {
                results[target.node].push(row);
            }
        }
        Ok(())
    })?;
    drop(state);
    for rows in &mut results {
        sort_results(rows);
    }
    Ok(results)
}

pub fn sort_results(rows: &mut [Row]) {
    rows.sort_by(|a, b| {
        b.received
            .cmp(&a.received)
            .then_with(|| a.latency.partial_cmp(&b.latency).unwrap())
            .then_with(|| a.endpoint.to_string().cmp(&b.endpoint.to_string()))
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    use {
        crate::process::PrivateDir,
        std::{fs, io::Write},
    };

    #[test]
    fn balances_unique_targets_and_rotates_three_times_per_lane() {
        let node = Node::parse(
            "vless://00000000-0000-4000-8000-000000000001@edge.example?type=ws&security=tls",
        )
        .unwrap();
        let rows: Vec<_> = (1..=14)
            .map(|port| SocketAddr::from(([127, 0, 0, 1], port)))
            .collect();
        for (count, parallel) in [(0, 0), (1, 1), (5, 1), (6, 2), (9, 3), (12, 4), (20, 4)] {
            let nodes = [
                (&node, &rows[..count.min(10)]),
                (&node, &rows[..count.saturating_sub(10)]),
            ];
            assert_eq!(group_targets(&nodes).len(), parallel);
        }
        let lanes = group_targets(&[(&node, &rows), (&node, &rows[..3])]);
        assert_eq!(lanes.iter().map(Vec::len).collect::<Vec<_>>(), [4, 3, 3, 3]);
        let mut assigned = std::collections::BTreeSet::new();
        for targets in lanes {
            for target in &targets {
                assert!(assigned.insert((target.node, target.endpoint)));
            }
            let mut order = Vec::new();
            let mut updates = Vec::new();
            let result = test_lane(
                &targets,
                &Control::default(),
                &AtomicBool::new(false),
                |index| {
                    order.push(index);
                    let round = (order.len() - 1) / targets.len();
                    Ok([Some(100.0), None, Some(20.0)][round])
                },
                |target, row| {
                    updates.push((target.endpoint, row.sent, row.received));
                    Ok(())
                },
            )
            .unwrap();
            let one_round: Vec<_> = (0..targets.len()).collect();
            assert_eq!(order, one_round.repeat(3));
            assert_eq!(updates.len(), targets.len() * 3);
            assert_eq!(updates[0].1, 1);
            assert_eq!(updates[targets.len()].1, 2);
            assert_eq!(updates[targets.len() * 2].1, 3);
            assert!(result.iter().all(|row| row.sent == 3
                && row.received == 2
                && [row.latency_min, row.latency, row.latency_max]
                    == [Some(20.0), Some(60.0), Some(100.0)]));
        }
        assert_eq!(assigned.len(), 13);
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn bundled_xray_switches_outbounds_on_one_port() {
        let work = PrivateDir::new(std::env::temp_dir().join(Uuid::new_v4().to_string())).unwrap();
        let server = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let upstream = server.local_addr().unwrap();
        let port = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let local = port.local_addr().unwrap();
        let node = Node::parse(
            "vless://00000000-0000-4000-8000-000000000001@edge.example?type=ws&security=tls",
        )
        .unwrap();
        let rows = [upstream, local];
        let nodes = [(&node, rows.as_slice())];
        let mut config = batch_config(&nodes, &group_targets(&nodes), &[local], "test-password");
        config["outbounds"][1] = json!({
            "tag": route_user(0, 0), "protocol": "freedom", "settings": {"redirect": upstream.to_string()},
        });
        config["outbounds"][2] = json!({"tag": route_user(0, 1), "protocol": "blackhole"});
        let config_path = work.path().join("test.json");
        fs::write(&config_path, config.to_string()).unwrap();
        let binary = Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/bin/xray");
        let control = Control::default();
        drop(port);
        let mut engine = Running::start(
            command(binary).args(["run", "-c"]).arg(config_path),
            &control,
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while TcpStream::connect(local).is_err() {
            assert!(!engine.exited().unwrap() && Instant::now() < deadline);
            thread::sleep(Duration::from_millis(20));
        }
        let server = thread::spawn(move || {
            use std::io::{BufRead, BufReader};
            server.set_nonblocking(true).unwrap();
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut client = loop {
                match server.accept() {
                    Ok((stream, _)) => break stream,
                    Err(e)
                        if e.kind() == io::ErrorKind::WouldBlock && Instant::now() < deadline =>
                    {
                        thread::sleep(Duration::from_millis(20))
                    }
                    Err(e) => panic!("HTTP probe did not arrive: {e}"),
                }
            };
            client
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            for line in BufReader::new(&client).lines() {
                if line.unwrap().is_empty() {
                    break;
                }
            }
            client
                .write_all(b"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n")
                .unwrap();
        });
        let probe = |index| {
            probe_204(
                work.path(),
                &format!("socks5h://{local}"),
                &format!("proxy-user = \"{}:test-password\"\n", route_user(0, index)),
                "http://probe.invalid/generate_204",
                &control,
            )
            .unwrap()
        };
        assert_eq!(probe(1), None);
        assert!(probe(0).is_some());
        server.join().unwrap();
    }

    #[test]
    fn parses_node_and_builds_config_for_candidate() {
        let link = "vless://00000000-0000-4000-8000-000000000001@edge.example:8443?type=ws&security=tls&sni=tls.example&host=ws.example&path=%2Fa%3Fb%3Dc#demo";
        let node = Node::parse(link).unwrap();
        assert!(node.stream["tlsSettings"].get("echConfigList").is_none());
        assert!(Node::parse(&link.replace("type=ws", "type=grpc")).is_none());
        assert!(Node::parse(&link.replace("#", "&allowInsecure=1#")).is_none());
        for query in ["&ech=", "&ech=%00", "&ech=a&ech=b"] {
            assert!(Node::parse(&link.replace('#', &format!("{query}#"))).is_none());
        }
        let ech_link = link.replace(
            "#demo",
            "&fp=chrome&ech=cloudflare-ech.com%2Bhttps%3A%2F%2Fdns.alidns.com%2Fdns-query#demo",
        );
        let node = Node::parse(&ech_link).unwrap();
        assert_eq!(node.port, 8443);
        let rows = ["[2001:db8::1]:2053".parse().unwrap()];
        let nodes = [(&node, rows.as_slice())];
        let config = batch_config(
            &nodes,
            &group_targets(&nodes),
            &["127.0.0.1:12345".parse().unwrap()],
            "test-password",
        );
        let outbound = &config["outbounds"][1];
        assert_eq!(outbound["settings"]["vnext"][0]["address"], "2001:db8::1");
        let tls = &outbound["streamSettings"]["tlsSettings"];
        assert_eq!(tls["serverName"], "tls.example");
        assert_eq!(
            tls["echConfigList"],
            "cloudflare-ech.com+https://dns.alidns.com/dns-query"
        );
        assert_eq!(outbound["streamSettings"]["wsSettings"]["path"], "/a?b=c");
        assert_eq!(measurement("204 0.100"), Some(100.0));
        assert_eq!(measurement("302 0.100"), None);

        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        {
            let work =
                PrivateDir::new(std::env::temp_dir().join(Uuid::new_v4().to_string())).unwrap();
            let binary = Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/bin/xray");
            let config_path = work.path().join("test.json");
            fs::write(&config_path, config.to_string()).unwrap();
            assert!(
                command(binary)
                    .args(["run", "-test", "-c"])
                    .arg(config_path)
                    .status()
                    .unwrap()
                    .success()
            );
        }
    }
}
