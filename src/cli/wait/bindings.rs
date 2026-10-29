use clap::{Arg, ArgGroup, ArgMatches, Command};

use crate::api::schema::waits::{
    WaitCancelProjection, WaitDeclareProjection, WaitExtendProjection, WaitListProjection,
    WaitTargetProjection,
};
use crate::api::schema::{
    WaitCancelParams, WaitDecider, WaitDeclareParams, WaitExtendParams, WaitListParams,
    WaitListState, WaitSourceInfo, WaitTarget,
};
use crate::cli::projection::{choices, optional_text, parse_choice, Choice, Input, ParseError};

const DECIDERS: &[Choice<WaitDecider>] = &[
    Choice::new("self", WaitDecider::Waiter),
    Choice::new("parent", WaitDecider::Parent),
    Choice::new("user", WaitDecider::User),
];

const STATES: &[Choice<WaitListState>] = &[
    Choice::new("live", WaitListState::Live),
    Choice::new("fired", WaitListState::Fired),
    Choice::new("satisfied", WaitListState::Satisfied),
    Choice::new("cancelled", WaitListState::Cancelled),
    Choice::new("all", WaitListState::All),
];

pub(crate) fn waiter_position() -> Input<String> {
    Input::positional("waiter", 0, |slot, value| {
        *slot = value.to_owned();
        Ok(())
    })
    .value_name("WAITER")
}

pub(crate) fn summary() -> Input<String> {
    Input::value("summary", "TEXT", |slot, value| {
        *slot = value.to_owned();
        Ok(())
    })
    .required()
    .help("What it waits for, 1 to 200 characters")
}

pub(crate) fn then() -> Input<Option<WaitDecider>> {
    Input::value("then", "DECIDER", |slot, value| {
        *slot = Some(parse_choice(DECIDERS, "decider", value)?);
        Ok(())
    })
    .configure(|arg| choices(arg, DECIDERS))
    .help("Who decides when it fires: self, parent or user (required unless --rearm)")
}

pub(crate) fn on() -> Input<Option<WaitSourceInfo>> {
    Input::value("on", "KIND:TARGET", |slot, value| {
        let Some((kind, target)) = value.split_once(':') else {
            return Err(ParseError::Usage(format!(
                "invalid wait source {value}: expected KIND:TARGET"
            )));
        };
        *slot = Some(WaitSourceInfo {
            kind: kind.to_owned(),
            target: target.to_owned(),
        });
        Ok(())
    })
    .help("What it watches, such as node:<agent>; `herdr status server --json` lists the kinds as wait_sources")
}

pub(crate) fn until() -> Input<Option<String>> {
    optional_text("until", "STATE")
        .help("The source state that satisfies it; for node, a task status (default complete)")
}

pub(crate) fn rearm() -> Input<Option<u64>> {
    Input::value("rearm", "WAIT_ID", |slot, value| {
        *slot = Some(parse_wait_id(value)?);
        Ok(())
    })
    .help("A fired wait to re-arm, keeping its extensions and --then")
}

pub(crate) fn waiter_filter() -> Input<Option<String>> {
    optional_text("waiter", "TARGET").help("Only waits of this agent id, agent name or pane id")
}

pub(crate) fn state() -> Input<WaitListState> {
    Input::value("state", "STATE", |slot, value| {
        *slot = parse_choice(STATES, "wait state", value)?;
        Ok(())
    })
    .configure(|arg| choices(arg, STATES).conflicts_with("all"))
    .help("live, fired, satisfied, cancelled or all (default: live)")
}

pub(crate) fn all() -> Input<WaitListState> {
    Input::flag("all", |slot, _| {
        *slot = WaitListState::All;
        Ok(())
    })
    .help("Waits in every state")
}

pub(crate) fn by() -> Input<Option<String>> {
    optional_text("by", "TARGET")
        .configure(|arg| arg.conflicts_with("user"))
        .help("Ask as this agent id, agent name or pane id (default: this pane)")
}

pub(crate) fn user() -> Input<Option<String>> {
    Input::flag("user", |slot, _| {
        *slot = None;
        Ok(())
    })
    .help("Ask as the user")
}

pub(crate) fn wait_id() -> Input<u64> {
    Input::positional("wait_id", 0, |slot, value| {
        *slot = parse_wait_id(value)?;
        Ok(())
    })
    .value_name("WAIT_ID")
}

fn parse_wait_id(value: &str) -> Result<u64, ParseError> {
    value
        .parse()
        .map_err(|_| ParseError::Usage(format!("invalid wait id: {value}")))
}

/// When a deadline falls: an RFC 3339 time, or a duration from now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Deadline {
    AtMs(u64),
    InMs(u64),
}

/// `90s`, `30m`, `2h`, `1d`, or an RFC 3339 time.
pub(crate) fn parse_deadline(value: &str) -> Result<Deadline, ParseError> {
    let invalid = || {
        ParseError::Usage(format!(
            "invalid deadline {value}: expected a duration like 90s, 30m, 2h or 1d, or an RFC 3339 time"
        ))
    };
    if let Ok(at) =
        time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
    {
        let ms = at.unix_timestamp_nanos() / 1_000_000;
        return u64::try_from(ms).map(Deadline::AtMs).map_err(|_| invalid());
    }
    let unit_at = value
        .find(|ch: char| !ch.is_ascii_digit())
        .ok_or_else(invalid)?;
    let (count, unit) = value.split_at(unit_at);
    let count: u64 = count.parse().map_err(|_| invalid())?;
    let unit_ms = match unit {
        "ms" => 1,
        "s" => 1_000,
        "m" => 60_000,
        "h" => 3_600_000,
        "d" => 86_400_000,
        _ => return Err(invalid()),
    };
    count
        .checked_mul(unit_ms)
        .map(Deadline::InMs)
        .ok_or_else(invalid)
}

fn deadline_arg(required: bool) -> Arg {
    Arg::new("deadline")
        .long("deadline")
        .value_name("WHEN")
        .required(required)
        .help("A duration from now (90s, 30m, 2h, 1d) or an RFC 3339 time")
}

pub(crate) enum Operation {
    Declare(WaitDeclareParams),
    List(WaitListParams),
    Show(WaitTarget),
    /// The params, plus whether the asker was left to default to this pane.
    Extend {
        params: WaitExtendParams,
        default_by: bool,
    },
    Cancel {
        params: WaitCancelParams,
        default_by: bool,
    },
}

fn leaf(name: &'static str, arguments: Vec<Arg>, about: &'static str) -> Command {
    crate::cli::projection::command(name, arguments).about(about)
}

pub(crate) fn command() -> Command {
    Command::new("wait")
        .about("Declare and manage agent waits: deadlines herdr keeps and fires as messages")
        .subcommand_required(true)
        .subcommand(
            leaf("declare", WaitDeclareProjection::arguments(), "Declare a wait for an agent")
                .arg(deadline_arg(true))
                .override_usage("herdr wait declare <WAITER> --summary TEXT --deadline WHEN [--then self|parent|user] [--on KIND:TARGET] [--until STATE] [--rearm WAIT_ID]")
                .after_help("WAITER is an agent id, agent name or pane id. When the deadline passes, herdr queues a message from `herdr wait` to the decider: the waiter (self), its parent (parent, or the user when it has none), or flags the user (user). A wait whose two extensions are spent also goes one level up. A deadline that passed while the server was down fires when it starts."),
        )
        .subcommand(leaf("list", WaitListProjection::arguments(), "List waits, oldest first"))
        .subcommand(leaf("show", WaitTargetProjection::arguments(), "Show a wait"))
        .subcommand(
            leaf("extend", WaitExtendProjection::arguments(), "Move a live wait's deadline; at most twice")
                .arg(deadline_arg(true))
                .group(ArgGroup::new("asker").args(["by", "user"])),
        )
        .subcommand(
            leaf("cancel", WaitCancelProjection::arguments(), "Cancel a live wait, or clear one that needs the user")
                .group(ArgGroup::new("asker").args(["by", "user"])),
        )
}

pub(crate) fn from_matches(matches: &ArgMatches) -> Result<Operation, ParseError> {
    let Some((name, matches)) = matches.subcommand() else {
        return Err(ParseError::Usage("missing wait command".into()));
    };
    let deadline = || -> Result<Option<Deadline>, ParseError> {
        matches
            .get_one::<String>("deadline")
            .map(|value| parse_deadline(value))
            .transpose()
    };
    let default_by = || !matches.contains_id("by") && !matches.get_flag("user");
    Ok(match name {
        "declare" => {
            let mut params = WaitDeclareProjection::from_matches(matches)?;
            match deadline()? {
                Some(Deadline::AtMs(at)) => params.deadline_at_ms = Some(at),
                Some(Deadline::InMs(ms)) => params.deadline_in_ms = Some(ms),
                None => {}
            }
            Operation::Declare(params)
        }
        "list" => Operation::List(WaitListProjection::from_matches(matches)?),
        "show" => Operation::Show(WaitTargetProjection::from_matches(matches)?),
        "extend" => {
            let mut params = WaitExtendProjection::from_matches(matches)?;
            match deadline()? {
                Some(Deadline::AtMs(at)) => params.deadline_at_ms = Some(at),
                Some(Deadline::InMs(ms)) => params.deadline_in_ms = Some(ms),
                None => {}
            }
            Operation::Extend {
                params,
                default_by: default_by(),
            }
        }
        "cancel" => Operation::Cancel {
            params: WaitCancelProjection::from_matches(matches)?,
            default_by: default_by(),
        },
        _ => return Err(ParseError::Usage(format!("unknown wait command {name}"))),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Operation, ParseError> {
        let args: Vec<String> = args.iter().map(|arg| (*arg).to_owned()).collect();
        crate::cli::projection::parse(command(), &args, from_matches)
    }

    #[test]
    fn choices_cover_the_api_enums() {
        crate::cli::projection::assert_choices_cover_api_enum(DECIDERS);
        crate::cli::projection::assert_choices_cover_api_enum(STATES);
        command().debug_assert();
    }

    #[test]
    fn deadlines_parse_as_durations_or_times() {
        assert_eq!(parse_deadline("90s").unwrap(), Deadline::InMs(90_000));
        assert_eq!(parse_deadline("30m").unwrap(), Deadline::InMs(1_800_000));
        assert_eq!(parse_deadline("2h").unwrap(), Deadline::InMs(7_200_000));
        assert_eq!(parse_deadline("1d").unwrap(), Deadline::InMs(86_400_000));
        assert_eq!(parse_deadline("250ms").unwrap(), Deadline::InMs(250));
        assert_eq!(
            parse_deadline("1970-01-01T00:00:01Z").unwrap(),
            Deadline::AtMs(1_000)
        );
        for bad in ["", "30", "m", "5w", "-5m", "1970-01-01"] {
            assert!(parse_deadline(bad).is_err(), "{bad} is refused");
        }
    }

    #[test]
    fn declare_parses_every_flag() {
        let Ok(Operation::Declare(params)) = parse(&[
            "declare",
            "worker",
            "--summary",
            "CI on !12",
            "--deadline",
            "30m",
            "--then",
            "parent",
            "--on",
            "mr:group/project!12",
            "--until",
            "merged",
            "--rearm",
            "4",
        ]) else {
            panic!("declare parses");
        };
        assert_eq!(params.waiter, "worker");
        assert_eq!(params.summary, "CI on !12");
        assert_eq!(params.deadline_in_ms, Some(1_800_000));
        assert_eq!(params.deadline_at_ms, None);
        assert_eq!(params.then, Some(WaitDecider::Parent));
        assert_eq!(
            params.on,
            Some(WaitSourceInfo {
                kind: "mr".into(),
                target: "group/project!12".into()
            })
        );
        assert_eq!(params.until.as_deref(), Some("merged"));
        assert_eq!(params.rearm, Some(4));
    }

    #[test]
    fn declare_needs_a_summary_and_a_deadline() {
        for args in [
            &["declare", "worker", "--deadline", "5m"][..],
            &["declare", "worker", "--summary", "x"],
            &[
                "declare",
                "worker",
                "--summary",
                "x",
                "--deadline",
                "5m",
                "--then",
                "boss",
            ],
        ] {
            assert!(
                matches!(parse(args), Err(ParseError::Clap(_))),
                "{args:?} is refused"
            );
        }
        assert!(matches!(
            parse(&["declare", "worker", "--summary", "x", "--deadline", "soon"]),
            Err(ParseError::Usage(_))
        ));
        assert!(matches!(
            parse(&[
                "declare",
                "worker",
                "--summary",
                "x",
                "--deadline",
                "5m",
                "--on",
                "node"
            ]),
            Err(ParseError::Usage(_))
        ));
    }

    #[test]
    fn list_defaults_to_live_and_all_takes_every_state() {
        let Ok(Operation::List(list)) = parse(&["list"]) else {
            panic!("list parses");
        };
        assert_eq!(list.state, WaitListState::Live);
        let Ok(Operation::List(list)) = parse(&["list", "--waiter", "worker", "--all"]) else {
            panic!("list parses");
        };
        assert_eq!(list.waiter.as_deref(), Some("worker"));
        assert_eq!(list.state, WaitListState::All);
        let Ok(Operation::List(list)) = parse(&["list", "--state", "fired"]) else {
            panic!("list parses");
        };
        assert_eq!(list.state, WaitListState::Fired);
        assert!(matches!(
            parse(&["list", "--state", "fired", "--all"]),
            Err(ParseError::Clap(_))
        ));
    }

    #[test]
    fn extend_and_cancel_ask_as_this_pane_unless_told_otherwise() {
        let Ok(Operation::Extend { params, default_by }) =
            parse(&["extend", "3", "--deadline", "10m"])
        else {
            panic!("extend parses");
        };
        assert_eq!(params.wait_id, 3);
        assert_eq!(params.deadline_in_ms, Some(600_000));
        assert!(default_by);

        let Ok(Operation::Cancel { params, default_by }) = parse(&["cancel", "3", "--user"]) else {
            panic!("cancel parses");
        };
        assert_eq!(params.by, None);
        assert!(!default_by);

        let Ok(Operation::Cancel { params, default_by }) = parse(&["cancel", "3", "--by", "lead"])
        else {
            panic!("cancel parses");
        };
        assert_eq!(params.by.as_deref(), Some("lead"));
        assert!(!default_by);

        assert!(matches!(parse(&["extend", "3"]), Err(ParseError::Clap(_))));
        assert!(matches!(
            parse(&["cancel", "3", "--by", "lead", "--user"]),
            Err(ParseError::Clap(_))
        ));
    }
}
