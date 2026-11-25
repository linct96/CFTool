use crate::process::{Control, Result, Running, command};
use std::{
    net::{IpAddr, SocketAddr},
    process::Stdio,
    time::Duration,
};
use url::Url;

#[derive(Debug)]
pub struct Candidate {
    pub ip: IpAddr,
    pub port: Option<u16>,
    pub note: String,
}
const MAX_SUBSCRIPTIONS: usize = 8;
const MAX_ENTRIES: usize = 1024;
const MAX_BODY: u64 = 128 * 1024;

fn entries(text: &str) -> impl Iterator<Item = (usize, &str, &str)> {
    text.trim_start_matches('\u{feff}')
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let (entry, note) = line.split_once('#').unwrap_or((line, ""));
            let entry = entry.trim();
            (!entry.is_empty()).then_some((index + 1, entry, note.trim()))
        })
}

fn endpoint(entry: &str) -> Option<(IpAddr, Option<u16>)> {
    if let Ok(ip) = entry.parse::<IpAddr>() {
        Some((ip, None))
    } else if let Ok(addr) = entry.parse::<SocketAddr>()
        && addr.port() != 0
        && !entry.contains('%')
    {
        Some((addr.ip(), Some(addr.port())))
    } else {
        let ip = entry.strip_prefix('[')?.strip_suffix(']')?.parse().ok()?;
        Some((IpAddr::V6(ip), None))
    }
}

fn subscription_url(entry: &str) -> Option<Url> {
    let (scheme, _) = entry.split_once("://")?;
    if !scheme.eq_ignore_ascii_case("https")
        || entry.len() > 4096
        || entry
            .chars()
            .any(|ch| ch.is_whitespace() || ch.is_control())
    {
        return None;
    }
    let url = Url::parse(entry).ok()?;
    (url.has_host()
        && url.username().is_empty()
        && url.password().is_none()
        && url.port() != Some(0))
    .then_some(url)
}

fn public_candidate(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => !matches!(
            ip.octets(),
            [0 | 10 | 127 | 224..=255, ..]
                | [100, 64..=127, ..]
                | [169, 254, ..]
                | [172, 16..=31, ..]
                | [192, 0, 0 | 2, _]
                | [192, 88, 99, _]
                | [192, 168, ..]
                | [198, 18 | 19, ..]
                | [198, 51, 100, _]
                | [203, 0, 113, _]
        ),
        // Accept global unicast, excluding special-use, transition and documentation ranges.
        IpAddr::V6(ip) => {
            let parts = ip.segments();
            (0x2000..=0x3fff).contains(&parts[0])
                && !matches!(
                    parts,
                    [0x2001, 0..=0x01ff | 0x0db8, ..]
                        | [0x2002 | 0x3ffe, ..]
                        | [0x3fff, 0..=0x0fff, ..]
                )
        }
    }
}

fn parse_subscription(body: &str) -> Result<Vec<Candidate>> {
    if body.len() as u64 > MAX_BODY {
        return Err("订阅响应超过大小限制".into());
    }
    let mut ips = Vec::new();
    for (line, entry, note) in entries(body) {
        if ips.len() == MAX_ENTRIES {
            return Err(format!("订阅超过 {MAX_ENTRIES} 个候选 IP").into());
        }
        let (ip, port) = endpoint(entry)
            .filter(|&(ip, _)| public_candidate(ip))
            .ok_or_else(|| format!("订阅内容第 {line} 行不是有效的公网 IP"))?;
        ips.push(Candidate {
            ip,
            port,
            note: note.into(),
        });
    }
    if ips.is_empty() {
        return Err("订阅不包含候选 IP".into());
    }
    Ok(ips)
}

#[tauri::command]
pub fn import_ip_subscription(body: &str) -> std::result::Result<Vec<String>, String> {
    let candidates = parse_subscription(body).map_err(|err| err.to_string())?;
    Ok(candidates
        .into_iter()
        .map(|Candidate { ip, port, note }| {
            let mut line = match port {
                Some(port) => SocketAddr::new(ip, port).to_string(),
                None => ip.to_string(),
            };
            if !note.is_empty() {
                line.push('#');
                line.push_str(&note);
            }
            line
        })
        .collect())
}

pub struct Input {
    pub ips: Vec<Candidate>,
    subscriptions: Vec<(usize, Url)>,
}

pub fn parse(input: &str) -> Result<Input> {
    let mut ips = Vec::new();
    let mut subscriptions = Vec::new();
    for (line, entry, note) in entries(input) {
        if let Some((ip, port)) = endpoint(entry) {
            ips.push(Candidate {
                ip,
                port,
                note: note.into(),
            });
        } else if let Some(url) = subscription_url(entry) {
            if subscriptions.iter().any(|(_, previous)| previous == &url) {
                continue;
            }
            if subscriptions.len() == MAX_SUBSCRIPTIONS {
                return Err(
                    format!("IP 列表最多可填写 {MAX_SUBSCRIPTIONS} 个不同的订阅链接").into(),
                );
            }
            subscriptions.push((line, url));
        } else {
            return Err(format!("IP 列表第 {line} 行不是有效的 IP 或 HTTPS 订阅链接").into());
        }
    }

    if ips.is_empty() && subscriptions.is_empty() {
        return Err("请先输入至少一个候选 IP 或订阅链接".into());
    }
    Ok(Input { ips, subscriptions })
}

impl Input {
    pub fn has_subscriptions(&self) -> bool {
        !self.subscriptions.is_empty()
    }

    pub fn resolve(
        self,
        mut download: impl FnMut(usize, &Url) -> Result<String>,
    ) -> Result<Vec<Candidate>> {
        let Self {
            mut ips,
            subscriptions,
        } = self;
        for (line, url) in subscriptions {
            let candidates = download(line, &url)
                .and_then(|body| parse_subscription(&body))
                .map_err(|err| format!("IP 列表第 {line} 行：{err}"))?;
            ips.extend(candidates);
        }
        Ok(ips)
    }
}

pub fn fetch(url: &Url, control: &Control) -> Result<String> {
    let max_body = MAX_BODY.to_string();
    let child = Running::start(
        command("/usr/bin/curl")
            .args([
                "-q",
                "--silent",
                "--fail",
                "--globoff",
                "--proto",
                "=https",
                "--connect-timeout",
                "5",
                "--max-time",
                "10",
                "--max-filesize",
                &max_body,
                "--write-out",
                "%{http_code}",
                "--config",
                "-",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped()),
        control,
    )?;
    // The query can contain a secret; pass the URL on stdin, not the process command line.
    let url = url.as_str().replace('\\', "\\\\").replace('"', "\\\"");
    let config = format!("url = \"{url}\"\n");
    let (status, mut bytes) = child.exchange(
        config.as_bytes(),
        MAX_BODY + 3,
        control,
        Duration::from_secs(11),
    )?;
    let code = bytes.split_off(bytes.len().checked_sub(3).ok_or("未获取到订阅响应")?);
    if code.first() == Some(&b'3') {
        return Err("订阅发生重定向，请填写最终的 HTTPS 链接".into());
    }
    if status.code() == Some(63) {
        return Err("订阅响应超过大小限制".into());
    }
    if !status.success() || code.first() != Some(&b'2') {
        return Err("下载失败，请检查链接和网络后重试".into());
    }
    String::from_utf8(bytes).map_err(|_| "订阅必须是 UTF-8 纯文本".into())
}
