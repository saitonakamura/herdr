use clap::{value_parser, Arg, ArgAction, ArgMatches, Command};
use serde_json::{json, Value};

pub struct Request {
    pub method: &'static str,
    pub params: Value,
    pub json: bool,
}

fn deadline() -> Arg {
    Arg::new("deadline")
        .long("deadline")
        .required(true)
        .value_parser(duration_ms)
        .help("Positive duration: ms, s, m, h or d (e.g. 2h)")
}

fn declaration(name: &'static str) -> Command {
    let command = Command::new(name)
        .arg(deadline())
        .arg(
            Arg::new("then")
                .long("then")
                .required(true)
                .value_parser(["self", "parent", "user"]),
        )
        .arg(Arg::new("until").long("until").default_value("done"))
        .arg(Arg::new("summary").long("summary"));
    if name == "watch" {
        command.arg(
            Arg::new("node")
                .required(true)
                .help("Herdr agent identity (resolved by Bash loom)"),
        )
    } else {
        command.arg(
            Arg::new("on")
                .long("on")
                .required(true)
                .help("node:<agent identity>; other kinds follow in loom ticket 100"),
        )
    }
}

pub fn command() -> Command {
    Command::new("loom-rust")
        .version(env!("CARGO_PKG_VERSION"))
        .about("Declare waits; Herdr owns monitoring, deadlines and bounded escalation")
        .subcommand_required(true)
        .arg(
            Arg::new("json")
                .long("json")
                .global(true)
                .action(ArgAction::SetTrue),
        )
        .subcommand(declaration("watch"))
        .subcommand(
            declaration("wait")
                .arg_required_else_help(true)
                .args_conflicts_with_subcommands(true)
                .subcommand(
                    Command::new("list")
                        .arg(
                            Arg::new("all")
                                .long("all")
                                .action(ArgAction::SetTrue)
                                .conflicts_with("state"),
                        )
                        .arg(Arg::new("state").long("state").value_parser([
                            "live",
                            "fired",
                            "satisfied",
                            "cancelled",
                            "all",
                        ])),
                )
                .subcommand(
                    Command::new("extend")
                        .arg(
                            Arg::new("id")
                                .required(true)
                                .value_parser(value_parser!(u64)),
                        )
                        .arg(deadline()),
                )
                .subcommand(
                    Command::new("cancel").arg(
                        Arg::new("id")
                            .required(true)
                            .value_parser(value_parser!(u64)),
                    ),
                ),
        )
}

pub fn duration_ms(input: &str) -> Result<u64, String> {
    let split = input
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(input.len());
    let (number, unit) = input.split_at(split);
    let multiplier = match unit {
        "ms" => 1,
        "s" => 1_000,
        "m" => 60_000,
        "h" => 3_600_000,
        "d" => 86_400_000,
        _ => return Err("deadline needs a unit: ms, s, m, h or d".into()),
    };
    number
        .parse::<u64>()
        .ok()
        .and_then(|n| n.checked_mul(multiplier))
        .filter(|n| *n > 0)
        .ok_or_else(|| "deadline must be positive and fit in milliseconds".into())
}

pub fn request(matches: &ArgMatches, caller: Option<&str>) -> Result<Request, String> {
    let caller = caller
        .filter(|id| !id.is_empty())
        .ok_or("LOOM_NODE is required; invoke this through loom in a registered node")?;
    let (verb, args) = matches.subcommand().ok_or("expected wait or watch")?;
    let json_output = matches.get_flag("json");
    if verb == "wait" {
        if let Some((action, args)) = args.subcommand() {
            let (method, params) = match action {
                "list" => (
                    "wait.list",
                    json!({"waiter": caller, "state": if args.get_flag("all") { "all" } else { args.get_one::<String>("state").map(String::as_str).unwrap_or("live") }}),
                ),
                "extend" => (
                    "wait.extend",
                    json!({"wait_id": args.get_one::<u64>("id"), "deadline_in_ms": args.get_one::<u64>("deadline"), "by": caller}),
                ),
                "cancel" => (
                    "wait.cancel",
                    json!({"wait_id": args.get_one::<u64>("id"), "by": caller}),
                ),
                _ => return Err("unknown wait action".into()),
            };
            return Ok(Request {
                method,
                params,
                json: json_output,
            });
        }
    }
    let source = if verb == "watch" {
        format!(
            "node:{}",
            args.get_one::<String>("node").ok_or("missing node")?
        )
    } else {
        args.get_one::<String>("on").ok_or("missing --on")?.clone()
    };
    let (kind, target) = source
        .split_once(':')
        .filter(|(k, t)| !k.is_empty() && !t.is_empty())
        .ok_or("--on must be kind:target")?;
    let until = args
        .get_one::<String>("until")
        .map(String::as_str)
        .unwrap_or("done");
    let until = if until == "done" { "complete" } else { until };
    if kind == "node"
        && !["complete", "failed", "closed", "needs_parent", "needs_user"].contains(&until)
    {
        return Err(
            "node --until must be done, complete, failed, closed, needs_parent or needs_user"
                .into(),
        );
    }
    let summary = args
        .get_one::<String>("summary")
        .cloned()
        .unwrap_or_else(|| format!("{source} reaches {until}"));
    if summary.is_empty() || summary.chars().count() > 200 {
        return Err("summary must be 1 to 200 characters".into());
    }
    let params = json!({"waiter": caller, "summary": summary, "on": {"kind": kind, "target": target}, "until": until,
        "deadline_in_ms": args.get_one::<u64>("deadline"), "then": args.get_one::<String>("then")});
    Ok(Request {
        method: "wait.declare",
        params,
        json: json_output,
    })
}

pub fn check_capabilities(capabilities: &Value, request: &Request) -> Result<(), String> {
    if capabilities["agent_waits"].as_bool() != Some(true) {
        return Err(
            "this Herdr server does not advertise agent_waits; upgrade the fork server".into(),
        );
    }
    if let Some(source) = request.params.get("on") {
        let kind = source["kind"].as_str().ok_or("missing source kind")?;
        let advertised = capabilities["wait_sources"]
            .as_array()
            .is_some_and(|sources| sources.iter().any(|s| s.as_str() == Some(kind)));
        if kind != "node" {
            return Err(format!("source kind {kind:?} is not implemented by this client (server advertises it: {advertised}); see loom ticket 100, Rust sources and run"));
        }
        if !advertised {
            return Err("this Herdr server does not advertise node in wait_sources; upgrade the fork server".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Request {
        request(
            &command().try_get_matches_from(args).unwrap(),
            Some("caller"),
        )
        .unwrap()
    }

    #[test]
    fn declaration_and_watch_are_the_same_request() {
        let wait = parse(&[
            "loom-rust",
            "wait",
            "--on",
            "node:child",
            "--deadline",
            "2h",
            "--then",
            "parent",
        ]);
        let watch = parse(&[
            "loom-rust",
            "watch",
            "child",
            "--deadline",
            "2h",
            "--then",
            "parent",
        ]);
        assert_eq!(wait.params, watch.params);
        assert_eq!(wait.params["until"], "complete");
        assert_eq!(wait.params["deadline_in_ms"], 7_200_000);
    }

    #[test]
    fn declarations_require_deadline_and_decider() {
        for args in [
            vec!["loom-rust", "wait", "--on", "node:x", "--then", "self"],
            vec!["loom-rust", "watch", "x", "--deadline", "1h"],
        ] {
            assert!(command().try_get_matches_from(args).is_err());
        }
    }

    #[test]
    fn duration_is_positive_explicit_and_overflow_checked() {
        for value in [
            "0s",
            "-1h",
            "2",
            "1.5h",
            "18446744073709551615d",
            "ms",
            "1w",
        ] {
            assert!(duration_ms(value).is_err(), "{value}");
        }
        assert_eq!(duration_ms("1ms"), Ok(1));
        assert_eq!(duration_ms("1d"), Ok(86_400_000));
    }

    #[test]
    fn management_is_scoped_to_the_caller() {
        assert_eq!(
            parse(&["loom-rust", "wait", "list", "--all"]).params,
            json!({"waiter":"caller", "state":"all"})
        );
        assert_eq!(
            parse(&["loom-rust", "wait", "extend", "7", "--deadline", "1m"]).params,
            json!({"wait_id":7, "deadline_in_ms":60000, "by":"caller"})
        );
        assert_eq!(
            parse(&["loom-rust", "wait", "cancel", "7"]).params,
            json!({"wait_id":7, "by":"caller"})
        );
        let args = command()
            .try_get_matches_from(["loom-rust", "wait", "list"])
            .unwrap();
        assert!(request(&args, None).is_err());
    }

    #[test]
    fn requests_match_herdrs_published_schema() {
        let schema: Value =
            serde_json::from_str(include_str!("../../../docs/next/api/herdr-api.schema.json"))
                .unwrap();
        let requests = [
            parse(&[
                "loom-rust",
                "wait",
                "--on",
                "node:child",
                "--deadline",
                "1h",
                "--then",
                "self",
            ]),
            parse(&["loom-rust", "wait", "list", "--all"]),
            parse(&["loom-rust", "wait", "extend", "1", "--deadline", "1h"]),
            parse(&["loom-rust", "wait", "cancel", "1"]),
            Request {
                method: "ping",
                params: json!({}),
                json: false,
            },
        ];
        for request in requests {
            let variant = schema["schemas"]["request"]["oneOf"]
                .as_array()
                .unwrap()
                .iter()
                .find(|v| v["properties"]["method"]["const"] == request.method)
                .unwrap();
            let reference = variant["properties"]["params"]["$ref"].as_str().unwrap();
            let name = reference.rsplit('/').next().unwrap();
            let def = &schema["schemas"]["request"]["$defs"][name];
            for key in request.params.as_object().unwrap().keys() {
                assert!(
                    def["properties"].get(key).is_some(),
                    "{}: {key}",
                    request.method
                );
            }
            for key in def["required"].as_array().into_iter().flatten() {
                assert!(
                    request.params.get(key.as_str().unwrap()).is_some(),
                    "{}: {key}",
                    request.method
                );
            }
        }
    }

    #[test]
    fn sources_are_probed_not_assumed() {
        let wait = parse(&[
            "loom-rust",
            "watch",
            "child",
            "--deadline",
            "1s",
            "--then",
            "self",
        ]);
        assert!(check_capabilities(&json!({"agent_waits":true}), &wait).is_err());
        assert!(check_capabilities(
            &json!({"agent_waits":true,"wait_sources":["node","pr"]}),
            &wait
        )
        .is_ok());
        let other = parse(&[
            "loom-rust",
            "wait",
            "--on",
            "pr:owner/repo#1",
            "--until",
            "merged",
            "--deadline",
            "1s",
            "--then",
            "self",
        ]);
        assert!(check_capabilities(
            &json!({"agent_waits":true,"wait_sources":["node","pr"]}),
            &other
        )
        .unwrap_err()
        .contains("ticket 100"));
    }
}
