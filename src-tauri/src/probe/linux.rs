use super::{Control, IpAddr, Result, network_output, tunnel_interface};
use serde::Deserialize;
use std::collections::BTreeSet;

#[derive(Deserialize)]
struct Link {
    ifname: String,
    linkinfo: Option<LinkInfo>,
}

#[derive(Deserialize)]
struct LinkInfo {
    info_kind: Option<String>,
}

fn tunnel_links(links: Vec<Link>) -> BTreeSet<String> {
    links
        .into_iter()
        .filter(|link| {
            tunnel_interface(&link.ifname)
                || matches!(
                    link.linkinfo
                        .as_ref()
                        .and_then(|info| info.info_kind.as_deref()),
                    Some(
                        "tun"
                            | "tap"
                            | "wireguard"
                            | "ppp"
                            | "xfrm"
                            | "vti"
                            | "vti6"
                            | "ovpn"
                            | "ovpn-dco"
                    )
                )
        })
        .map(|link| link.ifname)
        .collect()
}

#[derive(Deserialize)]
struct Route {
    dst: String,
    #[serde(rename = "type")]
    kind: Option<String>,
    dev: Option<String>,
    #[serde(default)]
    nexthops: Vec<NextHop>,
}

#[derive(Deserialize)]
struct NextHop {
    dev: Option<String>,
}

impl Route {
    fn devices(&self) -> impl Iterator<Item = &str> {
        self.dev
            .as_deref()
            .into_iter()
            .chain(self.nexthops.iter().filter_map(|hop| hop.dev.as_deref()))
    }
}

fn subscription_tunnel<'a>(routes: &'a [Route], tunnels: &BTreeSet<String>) -> Option<&'a str> {
    routes
        .iter()
        .filter(|route| {
            matches!(route.kind.as_deref(), None | Some("unicast"))
                && !["169.254.", "fe80:", "ff"]
                    .iter()
                    .any(|prefix| route.dst.starts_with(prefix))
        })
        .flat_map(Route::devices)
        .find(|&dev| tunnels.contains(dev))
}

fn ip<T: serde::de::DeserializeOwned>(args: &[&str], control: &Control) -> Result<Option<T>> {
    Ok(network_output("ip", args, control)?.and_then(|output| serde_json::from_str(&output).ok()))
}

fn read_tunnels(control: &Control) -> Result<Option<BTreeSet<String>>> {
    Ok(ip(&["-j", "-details", "link", "show"], control)?.map(tunnel_links))
}

pub(super) fn route_warning(ips: &BTreeSet<IpAddr>, control: &Control) -> Result<Option<String>> {
    if ips.is_empty() {
        return Ok(None);
    }
    const UNKNOWN: &str = "部分候选 IP 的系统路由无法确认，测速结果不一定代表直连。";
    let Some(tunnels) = read_tunnels(control)? else {
        return Ok(Some(UNKNOWN.into()));
    };
    let mut incomplete = false;
    for address in ips {
        let family = if address.is_ipv4() { "-4" } else { "-6" };
        let routes: Option<Vec<Route>> = ip(
            &["-j", family, "route", "get", &address.to_string()],
            control,
        )?;
        if let Some(routes) = routes {
            if let Some(dev) = routes
                .iter()
                .flat_map(Route::devices)
                .find(|&dev| tunnels.contains(dev))
            {
                return Ok(Some(format!(
                    "{address} 的系统路由经过隧道接口 {dev}（TUN/VPN），测速结果不一定代表直连。"
                )));
            }
            incomplete |= !routes.iter().any(|route| route.devices().next().is_some());
        } else {
            incomplete = true;
        }
    }
    Ok(incomplete.then(|| UNKNOWN.into()))
}

pub(super) fn subscription_route_warning(control: &Control) -> Result<Option<String>> {
    const UNKNOWN: &str = "无法确认订阅候选 IP 是否会经过 TUN/VPN。";
    let Some(tunnels) = read_tunnels(control)? else {
        return Ok(Some(UNKNOWN.into()));
    };
    let mut incomplete = false;
    // Policy-routed VPNs often put the tunnel default in a non-main table.
    for family in ["-4", "-6"] {
        let routes: Option<Vec<Route>> =
            ip(&["-j", family, "route", "show", "table", "all"], control)?;
        if let Some(routes) = routes {
            if let Some(dev) = subscription_tunnel(&routes, &tunnels) {
                return Ok(Some(format!(
                    "系统存在经过 {dev} 的 TUN/VPN 路由，订阅中的候选 IP 可能经过隧道。"
                )));
            }
        } else {
            incomplete = true;
        }
    }
    Ok(incomplete.then(|| UNKNOWN.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_linux_tunnels_by_link_kind_and_ignores_link_local_routes() {
        let tunnels = tunnel_links(
            serde_json::from_str(
                r#"[
            {"ifname":"eth0","linkinfo":{"info_slave_kind":"bridge"}},
            {"ifname":"privacy","linkinfo":{"info_kind":"wireguard"}},
            {"ifname":"tun0"}
        ]"#,
            )
            .unwrap(),
        );
        assert_eq!(tunnels, BTreeSet::from(["privacy".into(), "tun0".into()]));
        let mut routes: Vec<Route> = serde_json::from_str(
            r#"[
            {"dst":"default","dev":"eth0"},
            {"dst":"fe80::/64","dev":"tun0"},
            {"type":"local","dst":"10.0.0.1","dev":"privacy","table":"local"}
        ]"#,
        )
        .unwrap();
        assert_eq!(subscription_tunnel(&routes, &tunnels), None);
        routes.push(serde_json::from_str(r#"{"dst":"0.0.0.0/1","table":51820,"nexthops":[{"dev":"eth0"},{"dev":"privacy"}]}"#).unwrap());
        assert_eq!(subscription_tunnel(&routes, &tunnels), Some("privacy"));

        let control = Control::default();
        control.cancel();
        assert!(route_warning(&BTreeSet::from(["127.0.0.1".parse().unwrap()]), &control).is_err());
        assert!(subscription_route_warning(&control).is_err());
    }
}
