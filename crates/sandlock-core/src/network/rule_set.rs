// Literal allow rules kept per protocol, so live policy can be granted,
// narrowed, and checked for containment without re-resolving hostnames.

use std::collections::BTreeSet;
use std::net::IpAddr;

use super::rules::{IpCidr, NetRule, NetTarget, Protocol, ResolvedNetAllow, ResolvedNetAllowSet};
use crate::policy_fn::PolicyFnError;
use crate::seccomp::notif::{NetworkPolicy, PortAllow};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Target {
    Any,
    Net(IpCidr),
}

impl Target {
    fn covers(self, other: Target) -> bool {
        match (self, other) {
            (Target::Any, _) => true,
            (Target::Net(_), Target::Any) => false,
            (Target::Net(a), Target::Net(b)) => cidr_covers(a, b),
        }
    }

    fn intersect(self, other: Target) -> Option<Target> {
        if self.covers(other) {
            Some(other)
        } else if other.covers(self) {
            Some(self)
        } else {
            None
        }
    }
}

fn cidr_covers(a: IpCidr, b: IpCidr) -> bool {
    a.addr.is_ipv4() == b.addr.is_ipv4() && a.prefix_len <= b.prefix_len && a.contains(b.addr)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Ports {
    Any,
    Only(BTreeSet<u16>),
}

impl Ports {
    fn covers(&self, other: &Ports) -> bool {
        match (self, other) {
            (Ports::Any, _) => true,
            (Ports::Only(_), Ports::Any) => false,
            (Ports::Only(a), Ports::Only(b)) => b.is_subset(a),
        }
    }

    fn intersect(&self, other: &Ports) -> Option<Ports> {
        let ports = match (self, other) {
            (Ports::Any, p) | (p, Ports::Any) => p.clone(),
            (Ports::Only(a), Ports::Only(b)) => Ports::Only(a.intersection(b).copied().collect()),
        };
        match &ports {
            Ports::Only(set) if set.is_empty() => None,
            _ => Some(ports),
        }
    }

    fn to_port_allow(&self) -> PortAllow {
        match self {
            Ports::Any => PortAllow::Any,
            Ports::Only(set) => PortAllow::Specific(set.iter().copied().collect()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Rule {
    target: Target,
    ports: Ports,
}

impl Rule {
    fn covers(&self, other: &Rule) -> bool {
        self.target.covers(other.target) && self.ports.covers(&other.ports)
    }
}

/// Literal network allow rules for TCP, UDP and ICMP.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NetRuleSet {
    tcp: Vec<Rule>,
    udp: Vec<Rule>,
    icmp: Vec<Rule>,
}

impl NetRuleSet {
    fn rules(&self, protocol: Protocol) -> &Vec<Rule> {
        match protocol {
            Protocol::Tcp => &self.tcp,
            Protocol::Udp => &self.udp,
            Protocol::Icmp => &self.icmp,
        }
    }

    fn rules_mut(&mut self, protocol: Protocol) -> &mut Vec<Rule> {
        match protocol {
            Protocol::Tcp => &mut self.tcp,
            Protocol::Udp => &mut self.udp,
            Protocol::Icmp => &mut self.icmp,
        }
    }

    fn push(&mut self, protocol: Protocol, rule: Rule) {
        let rules = self.rules_mut(protocol);
        if !rules.contains(&rule) {
            rules.push(rule);
        }
    }

    pub(crate) fn from_resolved(set: &ResolvedNetAllowSet) -> Self {
        let mut out = Self::default();
        for (protocol, resolved) in [
            (Protocol::Tcp, &set.tcp),
            (Protocol::Udp, &set.udp),
            (Protocol::Icmp, &set.icmp),
        ] {
            for rule in resolved_rules(resolved) {
                out.push(protocol, rule);
            }
        }
        out
    }

    /// Parse `net_allow` specs that name no hostname, so applying them
    /// never needs DNS.
    pub(crate) fn parse_literal(specs: &[impl AsRef<str>]) -> Result<Self, PolicyFnError> {
        let mut out = Self::default();
        for spec in specs {
            let spec = spec.as_ref();
            let rules =
                NetRule::parse_allow(spec).map_err(|e| PolicyFnError::Invalid(e.to_string()))?;
            for rule in rules {
                let target = match rule.target {
                    NetTarget::AnyIp => Target::Any,
                    NetTarget::Cidr(c) => Target::Net(c),
                    NetTarget::Host(_) => {
                        return Err(PolicyFnError::Invalid(format!(
                            "`{}` names a hostname; use an IP or CIDR",
                            spec
                        )))
                    }
                };
                let ports = if rule.all_ports || rule.protocol == Protocol::Icmp {
                    Ports::Any
                } else {
                    Ports::Only(rule.ports.iter().copied().collect())
                };
                out.push(rule.protocol, Rule { target, ports });
            }
        }
        Ok(out)
    }

    /// Whether each rule of `other` lies within a single rule of `self`.
    pub(crate) fn covers(&self, other: &NetRuleSet) -> bool {
        [Protocol::Tcp, Protocol::Udp, Protocol::Icmp]
            .into_iter()
            .all(|p| {
                other
                    .rules(p)
                    .iter()
                    .all(|r| self.rules(p).iter().any(|g| g.covers(r)))
            })
    }

    pub(crate) fn union(&mut self, other: &NetRuleSet) {
        for p in [Protocol::Tcp, Protocol::Udp, Protocol::Icmp] {
            for rule in other.rules(p) {
                self.push(p, rule.clone());
            }
        }
    }

    pub(crate) fn intersect(&self, other: &NetRuleSet) -> NetRuleSet {
        let mut out = Self::default();
        for p in [Protocol::Tcp, Protocol::Udp, Protocol::Icmp] {
            for a in self.rules(p) {
                for b in other.rules(p) {
                    if let (Some(target), Some(ports)) =
                        (a.target.intersect(b.target), a.ports.intersect(&b.ports))
                    {
                        out.push(p, Rule { target, ports });
                    }
                }
            }
        }
        out
    }

    pub(crate) fn policy(&self, protocol: Protocol) -> NetworkPolicy {
        let rules = self.rules(protocol);
        if rules
            .iter()
            .any(|r| r.target == Target::Any && r.ports == Ports::Any)
        {
            return NetworkPolicy::Unrestricted;
        }
        let mut cidrs = Vec::new();
        let mut any_ip_ports = std::collections::HashSet::new();
        for rule in rules {
            match (rule.target, &rule.ports) {
                (Target::Net(c), ports) => cidrs.push((c, ports.to_port_allow())),
                (Target::Any, Ports::Only(set)) => any_ip_ports.extend(set.iter().copied()),
                (Target::Any, Ports::Any) => unreachable!(),
            }
        }
        NetworkPolicy::AllowList {
            per_ip: std::collections::HashMap::new(),
            cidrs,
            any_ip_ports,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.tcp.is_empty() && self.udp.is_empty() && self.icmp.is_empty()
    }
}

fn host(ip: IpAddr) -> IpCidr {
    let prefix_len = if ip.is_ipv4() { 32 } else { 128 };
    IpCidr {
        addr: ip,
        prefix_len,
    }
}

fn resolved_rules(r: &ResolvedNetAllow) -> Vec<Rule> {
    let mut out = Vec::new();
    if r.any_ip_all_ports {
        out.push(Rule {
            target: Target::Any,
            ports: Ports::Any,
        });
    }
    if !r.any_ip_ports.is_empty() {
        out.push(Rule {
            target: Target::Any,
            ports: Ports::Only(r.any_ip_ports.iter().copied().collect()),
        });
    }
    for (ip, ports) in &r.per_ip {
        let ports = if r.per_ip_all_ports.contains(ip) {
            Ports::Any
        } else if ports.is_empty() {
            continue;
        } else {
            Ports::Only(ports.iter().copied().collect())
        };
        out.push(Rule {
            target: Target::Net(host(*ip)),
            ports,
        });
    }
    for (net, allowed) in &r.cidrs {
        let ports = match allowed {
            PortAllow::Any => Ports::Any,
            PortAllow::Specific(set) => Ports::Only(set.iter().copied().collect()),
        };
        out.push(Rule {
            target: Target::Net(*net),
            ports,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(specs: &[&str]) -> NetRuleSet {
        NetRuleSet::parse_literal(specs).unwrap()
    }

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn covers_star_covers_everything() {
        assert!(set(&["*:*"]).covers(&set(&["10.0.0.5:22", "tcp://10.0.0.0/8:443", "udp://*:53"])));
    }

    #[test]
    fn covers_respects_ports() {
        let g = set(&["*:443"]);
        assert!(g.covers(&set(&["10.0.0.5:443"])));
        assert!(!g.covers(&set(&["10.0.0.5:22"])));
    }

    #[test]
    fn covers_respects_protocol() {
        assert!(!set(&["tcp://*:*"]).covers(&set(&["udp://10.0.0.5:53"])));
    }

    #[test]
    fn covers_needs_single_rule() {
        let g = set(&["10.0.0.0/25:*", "10.0.0.128/25:*"]);
        assert!(!g.covers(&set(&["10.0.0.0/24:*"])));
    }

    #[test]
    fn intersect_narrows() {
        assert_eq!(
            set(&["*:*"]).intersect(&set(&["10.0.0.5:443"])),
            set(&["10.0.0.5:443"])
        );
        assert!(set(&["10.0.0.0/24:*"])
            .intersect(&set(&["192.168.0.1:*"]))
            .is_empty());
    }

    #[test]
    fn parse_literal_rejects_hostnames() {
        assert!(matches!(
            NetRuleSet::parse_literal(&["example.com:443"]),
            Err(PolicyFnError::Invalid(_))
        ));
    }

    #[test]
    fn policy_star_is_unrestricted() {
        assert!(matches!(
            set(&["*:*"]).policy(Protocol::Tcp),
            NetworkPolicy::Unrestricted
        ));
    }

    #[test]
    fn policy_allows_granted_port() {
        let p = set(&["10.0.0.5:443"]).policy(Protocol::Tcp);
        assert!(p.allows(ip("10.0.0.5"), 443));
        assert!(!p.allows(ip("10.0.0.5"), 22));
    }

    #[test]
    fn v4_mapped_destination_matches_ipv4_rule() {
        let p = set(&["10.0.0.5:443"]).policy(Protocol::Tcp);
        assert!(p.allows(ip("::ffff:10.0.0.5"), 443));
    }

    #[test]
    fn empty_set_denies_everything() {
        assert!(NetRuleSet::default()
            .policy(Protocol::Udp)
            .denies_everything());
    }
}
