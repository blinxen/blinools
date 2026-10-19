use anyhow::Context;
use ipnet::IpNet;
use nftables::batch::Batch;
use nftables::expr::{
    CT, Expression, Meta, MetaKey, NamedExpression, Payload, PayloadField, Prefix, SetItem,
};
use nftables::helper;
use nftables::schema::{Chain, NfListObject, Rule, Table};
use nftables::stmt::{Match, Operator, Statement};
use nftables::types::{NfChainPolicy, NfChainType, NfFamily, NfHook};
use std::net::{IpAddr, ToSocketAddrs};

use crate::sandbox::config::IpOrRangeOrHostname;
use crate::sandbox::config::NetworkException;

const TABLE: &str = "ROOT_TABLE";

pub fn install(
    dns: Option<Vec<String>>,
    exceptions: Vec<NetworkException>,
) -> Result<(), anyhow::Error> {
    let batch = build_ruleset(dns, exceptions);
    helper::apply_ruleset(&batch.to_nftables()).context("installing firewall")
}

fn build_ruleset(dns: Option<Vec<String>>, exceptions: Vec<NetworkException>) -> Batch<'static> {
    let mut batch = Batch::new();

    recreate_table(&mut batch);
    // deny all incoming
    batch.add(chain("input", NfHook::Input, NfChainPolicy::Drop));
    // deny all forwarding, we don't do routing
    batch.add(chain("forward", NfHook::Forward, NfChainPolicy::Drop));
    // allow all outgoing, blocking this breaks the internet
    batch.add(chain("output", NfHook::Output, NfChainPolicy::Accept));

    // Make exception for input traffic -> stateful outgoing traffic
    batch.add(rule(
        "input",
        vec![ct_established(), Statement::Accept(None)],
    ));
    // Don't allow outgoing traffic to loopback address
    batch.add(rule(
        "output",
        vec![meta_eq(MetaKey::Oifname, "lo"), Statement::Drop(None)],
    ));

    // Collect all exceptions as (network, optional port)
    // A single IP is just a /32 or /128
    let mut allow: Vec<(IpNet, Option<u16>)> = Vec::new();

    for entry in dns.into_iter().flatten() {
        match entry.parse::<IpAddr>() {
            // Only allow DNS ports
            Ok(ip) => allow.push((ip.into(), Some(53))),
            Err(e) => eprintln!("ignoring invalid DNS address {entry:?}: {e}"),
        }
    }
    for exception in exceptions {
        let port = exception.port;
        match exception.ip_or_range_or_hostname {
            IpOrRangeOrHostname::Ip(ip) => allow.push((ip.into(), port)),
            IpOrRangeOrHostname::Range(net) => allow.push((net.trunc(), port)),
            IpOrRangeOrHostname::Hostname(hostname) => {
                if let Ok(addresses) = (hostname.as_str(), 0u16).to_socket_addrs() {
                    allow.extend(addresses.map(|a| (IpNet::from(a.ip()), port)));
                }
            }
        }
    }

    // Remove duplicates to avoid nft issues
    allow.sort();
    allow.dedup();
    for (net, port) in allow {
        batch.add(rule("output", build_exception(net, port)));
    }

    batch.add(rule(
        "output",
        vec![
            daddr_in(
                "ip",
                vec![
                    prefix("10.0.0.0", 8),     // RFC1918
                    prefix("172.16.0.0", 12),  // RFC1918
                    prefix("192.168.0.0", 16), // RFC1918
                    prefix("100.64.0.0", 10),  // CGNAT / Tailscale
                    prefix("169.254.0.0", 16), // link-local
                ],
            ),
            Statement::Drop(None),
        ],
    ));
    batch.add(rule(
        "output",
        vec![
            daddr_in(
                "ip6",
                vec![
                    prefix("fc00::", 7),  // unique local
                    prefix("fe80::", 10), // link-local
                ],
            ),
            Statement::Drop(None),
        ],
    ));

    batch
}

fn recreate_table(batch: &mut Batch) {
    let table = || {
        NfListObject::Table(Table {
            family: NfFamily::INet,
            name: TABLE.into(),
            ..Default::default()
        })
    };
    // First we make sure the table exists because deleting errors out if it does not
    // Then we actually delete it.
    batch.add(table());
    batch.delete(table());
    batch.add(table());
}

fn chain(name: &'static str, hook: NfHook, policy: NfChainPolicy) -> NfListObject<'static> {
    NfListObject::Chain(Chain {
        family: NfFamily::INet,
        table: TABLE.into(),
        name: name.into(),
        _type: Some(NfChainType::Filter),
        hook: Some(hook),
        prio: Some(0),
        policy: Some(policy),
        ..Default::default()
    })
}

fn rule(chain: &'static str, stmts: Vec<Statement<'static>>) -> NfListObject<'static> {
    NfListObject::Rule(Rule {
        family: NfFamily::INet,
        table: TABLE.into(),
        chain: chain.into(),
        expr: stmts.into(),
        ..Default::default()
    })
}

fn meta_eq(key: MetaKey, val: &'static str) -> Statement<'static> {
    Statement::Match(Match {
        left: Expression::Named(NamedExpression::Meta(Meta { key })),
        right: Expression::String(val.into()),
        op: Operator::EQ,
    })
}

fn ct_established() -> Statement<'static> {
    Statement::Match(Match {
        left: Expression::Named(NamedExpression::CT(CT {
            key: "state".into(),
            family: None,
            dir: None,
        })),
        right: Expression::List(vec![
            Expression::String("established".into()),
            Expression::String("related".into()),
        ]),
        op: Operator::IN,
    })
}

fn build_exception(net: IpNet, port: Option<u16>) -> Vec<Statement<'static>> {
    let family = if net.addr().is_ipv4() { "ip" } else { "ip6" };
    let mut stmts = vec![daddr_in(
        family,
        vec![prefix(&net.addr().to_string(), u32::from(net.prefix_len()))],
    )];

    if let Some(port) = port {
        stmts.push(Statement::Match(Match {
            left: Expression::Named(NamedExpression::Payload(Payload::PayloadField(
                PayloadField {
                    protocol: "th".into(),
                    field: "dport".into(),
                },
            ))),
            right: Expression::Number(u32::from(port)),
            op: Operator::EQ,
        }));
    }

    stmts.push(Statement::Accept(None));
    stmts
}

fn prefix(addr: &str, len: u32) -> Expression<'static> {
    Expression::Named(NamedExpression::Prefix(Prefix {
        addr: Box::new(Expression::String(addr.to_string().into())),
        len,
    }))
}

fn daddr_in(proto: &'static str, nets: Vec<Expression<'static>>) -> Statement<'static> {
    Statement::Match(Match {
        left: Expression::Named(NamedExpression::Payload(Payload::PayloadField(
            PayloadField {
                protocol: proto.into(),
                field: "daddr".into(),
            },
        ))),
        right: Expression::Named(NamedExpression::Set(
            nets.into_iter().map(SetItem::Element).collect(),
        )),
        op: Operator::IN,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn exception(target: IpOrRangeOrHostname, port: Option<u16>) -> NetworkException {
        NetworkException {
            ip_or_range_or_hostname: target,
            port,
        }
    }

    fn net(s: &str) -> IpNet {
        s.parse().unwrap()
    }

    fn objects(batch: &Batch) -> Vec<Value> {
        let json = serde_json::to_value(batch.clone().to_nftables()).unwrap();
        json["nftables"].as_array().unwrap().clone()
    }

    fn expression_str(v: &Value) -> String {
        match v {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            Value::Array(a) => format!(
                "[{}]",
                a.iter().map(expression_str).collect::<Vec<_>>().join(", ")
            ),
            Value::Object(o) => {
                if let Some(p) = o.get("prefix") {
                    format!("{}/{}", expression_str(&p["addr"]), p["len"])
                } else if let Some(s) = o.get("set") {
                    format!(
                        "{{{}}}",
                        s.as_array()
                            .unwrap()
                            .iter()
                            .map(expression_str)
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                } else if let Some(p) = o.get("payload") {
                    format!(
                        "{} {}",
                        expression_str(&p["protocol"]),
                        expression_str(&p["field"])
                    )
                } else if let Some(m) = o.get("meta") {
                    format!("meta {}", expression_str(&m["key"]))
                } else if let Some(c) = o.get("ct") {
                    format!("ct {}", expression_str(&c["key"]))
                } else {
                    v.to_string()
                }
            }
            other => other.to_string(),
        }
    }

    fn statement_str(v: &Value) -> String {
        if let Some(m) = v.get("match") {
            let op = m["op"].as_str().unwrap();
            format!(
                "{} {} {}",
                expression_str(&m["left"]),
                op,
                expression_str(&m["right"])
            )
        } else if v.get("accept").is_some() {
            "accept".into()
        } else if v.get("drop").is_some() {
            "drop".into()
        } else {
            v.to_string()
        }
    }

    fn rules(batch: &Batch, chain: &str) -> Vec<String> {
        objects(batch)
            .iter()
            .filter_map(|o| o.get("add")?.get("rule"))
            .filter(|r| r["chain"] == chain && r["table"] == TABLE)
            .map(|r| {
                r["expr"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(statement_str)
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect()
    }

    fn accepts(batch: &Batch) -> Vec<String> {
        rules(batch, "output")
            .into_iter()
            .filter(|r| r.ends_with("accept"))
            .collect()
    }

    fn position(rules: &[String], needle: &str) -> usize {
        rules
            .iter()
            .position(|r| r == needle)
            .unwrap_or_else(|| panic!("rule {needle:?} not found in {rules:#?}"))
    }

    #[test]
    fn table_is_recreated_in_order() {
        let batch = build_ruleset(None, vec![]);
        let kinds: Vec<String> = objects(&batch)
            .iter()
            .take(3)
            .map(|o| {
                let (action, body) = o.as_object().unwrap().iter().next().unwrap();
                assert!(body.get("table").is_some(), "expected table object: {o}");
                action.clone()
            })
            .collect();
        assert_eq!(kinds, ["add", "delete", "add"]);
    }

    #[test]
    fn base_chain_policies() {
        let batch = build_ruleset(None, vec![]);
        let policy = |name: &str| -> String {
            objects(&batch)
                .iter()
                .filter_map(|o| o.get("add")?.get("chain"))
                .find(|c| c["name"] == name)
                .unwrap_or_else(|| panic!("chain {name} missing"))["policy"]
                .as_str()
                .unwrap()
                .to_string()
        };
        assert_eq!(policy("input"), "drop");
        assert_eq!(policy("forward"), "drop");
        assert_eq!(policy("output"), "accept");
    }

    #[test]
    fn input_allows_only_established() {
        let batch = build_ruleset(None, vec![]);
        assert_eq!(
            rules(&batch, "input"),
            ["ct state in [established, related] accept"]
        );
    }

    #[test]
    fn loopback_output_is_dropped_first() {
        let batch = build_ruleset(None, vec![]);
        assert_eq!(rules(&batch, "output")[0], "meta oifname == lo drop");
    }

    #[test]
    fn private_ranges_are_dropped() {
        let batch = build_ruleset(None, vec![]);
        let out = rules(&batch, "output");
        assert!(out.contains(
            &"ip daddr in {10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16, 100.64.0.0/10, 169.254.0.0/16} drop"
                .to_string()
        ));
        assert!(out.contains(&"ip6 daddr in {fc00::/7, fe80::/10} drop".to_string()));
    }

    #[test]
    fn single_ipv4_without_port() {
        let batch = build_ruleset(
            None,
            vec![exception(
                IpOrRangeOrHostname::Ip("203.0.113.7".parse().unwrap()),
                None,
            )],
        );
        assert_eq!(accepts(&batch), ["ip daddr in {203.0.113.7/32} accept"]);
    }

    #[test]
    fn single_ipv4_with_port() {
        let batch = build_ruleset(
            None,
            vec![exception(
                IpOrRangeOrHostname::Ip("203.0.113.7".parse().unwrap()),
                Some(443),
            )],
        );
        assert_eq!(
            accepts(&batch),
            ["ip daddr in {203.0.113.7/32} th dport == 443 accept"]
        );
    }

    #[test]
    fn single_ipv6_with_and_without_port() {
        let batch = build_ruleset(
            None,
            vec![
                exception(
                    IpOrRangeOrHostname::Ip("2001:db8::1".parse().unwrap()),
                    None,
                ),
                exception(
                    IpOrRangeOrHostname::Ip("2001:db8::2".parse().unwrap()),
                    Some(8080),
                ),
            ],
        );
        let acc = accepts(&batch);
        assert_eq!(acc.len(), 2);
        assert!(acc.contains(&"ip6 daddr in {2001:db8::1/128} accept".to_string()));
        assert!(
            acc.contains(&"ip6 daddr in {2001:db8::2/128} th dport == 8080 accept".to_string())
        );
    }

    #[test]
    fn cidr_v4_and_v6() {
        let batch = build_ruleset(
            None,
            vec![
                exception(IpOrRangeOrHostname::Range(net("198.51.100.0/24")), Some(22)),
                exception(IpOrRangeOrHostname::Range(net("2001:db8::/32")), None),
            ],
        );
        let acc = accepts(&batch);
        assert!(acc.contains(&"ip daddr in {198.51.100.0/24} th dport == 22 accept".to_string()));
        assert!(acc.contains(&"ip6 daddr in {2001:db8::/32} accept".to_string()));
    }

    #[test]
    fn cidr_host_bits_are_truncated() {
        let batch = build_ruleset(
            None,
            vec![exception(
                IpOrRangeOrHostname::Range(net("192.0.2.77/24")),
                None,
            )],
        );
        assert_eq!(accepts(&batch), ["ip daddr in {192.0.2.0/24} accept"]);
    }

    #[test]
    fn hostname_resolving_to_ip_literal() {
        let batch = build_ruleset(
            None,
            vec![exception(
                IpOrRangeOrHostname::Hostname("203.0.113.9".into()),
                Some(443),
            )],
        );
        assert_eq!(
            accepts(&batch),
            ["ip daddr in {203.0.113.9/32} th dport == 443 accept"]
        );
    }

    #[test]
    fn unresolvable_hostname_adds_no_rule() {
        let batch = build_ruleset(
            None,
            vec![exception(
                IpOrRangeOrHostname::Hostname("does-not-exist.invalid".into()),
                None,
            )],
        );
        assert!(accepts(&batch).is_empty());
    }

    #[test]
    fn dns_servers_restricted_to_port_53() {
        let batch = build_ruleset(Some(vec!["9.9.9.9".into(), "2620:fe::fe".into()]), vec![]);
        let acc = accepts(&batch);
        assert_eq!(acc.len(), 2);
        assert!(acc.contains(&"ip daddr in {9.9.9.9/32} th dport == 53 accept".to_string()));
        assert!(acc.contains(&"ip6 daddr in {2620:fe::fe/128} th dport == 53 accept".to_string()));
    }

    #[test]
    fn invalid_dns_entries_are_ignored() {
        let batch = build_ruleset(Some(vec!["not-an-ip".into(), "1.1.1.1".into()]), vec![]);
        assert_eq!(
            accepts(&batch),
            ["ip daddr in {1.1.1.1/32} th dport == 53 accept"]
        );
    }

    #[test]
    fn no_dns_means_no_dns_rules() {
        let batch = build_ruleset(None, vec![]);
        assert!(accepts(&batch).is_empty());
    }

    #[test]
    fn duplicate_dns_servers_emit_one_rule() {
        let batch = build_ruleset(Some(vec!["1.1.1.1".into(), "1.1.1.1".into()]), vec![]);
        assert_eq!(accepts(&batch).len(), 1);
    }

    #[test]
    fn duplicate_exceptions_emit_one_rule() {
        let e = || {
            exception(
                IpOrRangeOrHostname::Ip("203.0.113.7".parse().unwrap()),
                Some(80),
            )
        };
        let batch = build_ruleset(None, vec![e(), e()]);
        assert_eq!(accepts(&batch).len(), 1);
    }

    #[test]
    fn hostname_and_ip_to_same_target_dedup() {
        let batch = build_ruleset(
            None,
            vec![
                exception(
                    IpOrRangeOrHostname::Ip("203.0.113.9".parse().unwrap()),
                    None,
                ),
                exception(IpOrRangeOrHostname::Hostname("203.0.113.9".into()), None),
            ],
        );
        assert_eq!(accepts(&batch).len(), 1);
    }

    #[test]
    fn same_ip_different_ports_are_kept_separate() {
        let ip = || IpOrRangeOrHostname::Ip("203.0.113.7".parse().unwrap());
        let batch = build_ruleset(
            None,
            vec![exception(ip(), Some(80)), exception(ip(), Some(443))],
        );
        assert_eq!(accepts(&batch).len(), 2);
    }

    #[test]
    fn exceptions_precede_private_range_drops() {
        let batch = build_ruleset(
            Some(vec!["192.168.1.1".into()]),
            vec![exception(
                IpOrRangeOrHostname::Range(net("10.1.0.0/16")),
                Some(443),
            )],
        );
        let out = rules(&batch, "output");
        let drop_v4 = out
            .iter()
            .position(|r| r.starts_with("ip daddr in {10.0.0.0/8") && r.ends_with("drop"))
            .unwrap();
        assert!(position(&out, "ip daddr in {10.1.0.0/16} th dport == 443 accept") < drop_v4);
        assert!(position(&out, "ip daddr in {192.168.1.1/32} th dport == 53 accept") < drop_v4);
    }

    #[test]
    fn loopback_drop_precedes_exceptions() {
        let batch = build_ruleset(
            None,
            vec![exception(
                IpOrRangeOrHostname::Ip("127.0.0.1".parse().unwrap()),
                None,
            )],
        );
        let out = rules(&batch, "output");
        assert!(
            position(&out, "meta oifname == lo drop")
                < position(&out, "ip daddr in {127.0.0.1/32} accept")
        );
    }

    #[test]
    fn full_ruleset_serializes() {
        let batch = build_ruleset(
            Some(vec!["1.1.1.1".into()]),
            vec![
                exception(
                    IpOrRangeOrHostname::Ip("203.0.113.7".parse().unwrap()),
                    Some(443),
                ),
                exception(IpOrRangeOrHostname::Range(net("2001:db8::/32")), None),
            ],
        );
        let s = serde_json::to_string(&batch.to_nftables()).unwrap();
        assert!(s.starts_with("{\"nftables\":["));
    }
}
