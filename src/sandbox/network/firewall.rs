use anyhow::Context;
use nftables::batch::Batch;
use nftables::expr::{
    CT, Expression, Meta, MetaKey, NamedExpression, Payload, PayloadField, Prefix, SetItem,
};
use nftables::helper;
use nftables::schema::{Chain, NfListObject, Rule, Table};
use nftables::stmt::{Match, Operator, Statement};
use nftables::types::{NfChainPolicy, NfChainType, NfFamily, NfHook};
use std::net::IpAddr;

const TABLE: &str = "ROOT_TABLE";

pub fn install(dns: Option<Vec<String>>) -> Result<(), anyhow::Error> {
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

    let mut dns_v4 = Vec::new();
    let mut dns_v6 = Vec::new();
    for entry in dns.into_iter().flatten() {
        match entry.parse::<IpAddr>() {
            Ok(ip @ IpAddr::V4(_)) => dns_v4.push(ip_expr(ip)),
            Ok(ip @ IpAddr::V6(_)) => dns_v6.push(ip_expr(ip)),
            Err(e) => eprintln!("ignoring invalid DNS address {entry:?}: {e}"),
        }
    }

    // Make exceptions for the defined DNS servers.
    if !dns_v4.is_empty() {
        batch.add(rule(
            "output",
            vec![daddr_in("ip", dns_v4), Statement::Accept(None)],
        ));
    }
    if !dns_v6.is_empty() {
        batch.add(rule(
            "output",
            vec![daddr_in("ip6", dns_v6), Statement::Accept(None)],
        ));
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

    helper::apply_ruleset(&batch.to_nftables()).context("installing firewall")
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

fn ip_expr(ip: IpAddr) -> Expression<'static> {
    Expression::String(ip.to_string().into())
}

fn prefix(addr: &'static str, len: u32) -> Expression<'static> {
    Expression::Named(NamedExpression::Prefix(Prefix {
        addr: Box::new(Expression::String(addr.into())),
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
