use clap::{Arg, ArgAction, ArgGroup, ArgMatches, Command};

use crate::api::schema::messages::{
    MessageListProjection, MessageSendProjection, MessageTargetProjection,
};
use crate::api::schema::{MessageKind, MessageListParams, MessageSendParams, MessageTarget};
use crate::cli::projection::{choices, optional_text, parse_choice, Choice, Input, ParseError};

const KINDS: &[Choice<MessageKind>] = &[
    Choice::new("report", MessageKind::Report),
    Choice::new("reply", MessageKind::Reply),
    Choice::new("tell", MessageKind::Tell),
];

pub(crate) fn to_position() -> Input<String> {
    Input::positional("to", 0, |slot, value| {
        *slot = value.to_owned();
        Ok(())
    })
    .value_name("TO")
}

pub(crate) fn summary_position() -> Input<String> {
    Input::positional("summary", 1, |slot, value| {
        *slot = value.to_owned();
        Ok(())
    })
    .value_name("SUMMARY")
    .verbatim()
}

pub(crate) fn kind() -> Input<MessageKind> {
    Input::value("kind", "KIND", |slot, value| {
        *slot = parse_choice(KINDS, "message kind", value)?;
        Ok(())
    })
    .configure(|arg| choices(arg, KINDS))
    .help("report, reply or tell (default: tell)")
}

pub(crate) fn body() -> Input<Option<String>> {
    optional_text("body", "TEXT")
        .configure(|arg| arg.conflicts_with("body-file"))
        .help("The rest of the message, shown by `herdr message show`")
}

pub(crate) fn from() -> Input<Option<String>> {
    optional_text("from", "TARGET")
        .configure(|arg| arg.conflicts_with("user"))
        .help("Sender agent id, agent name or pane id (default: this pane)")
}

pub(crate) fn user() -> Input<Option<String>> {
    Input::flag("user", |slot, _| {
        *slot = None;
        Ok(())
    })
    .help("Send as the user")
}

pub(crate) fn reply_to() -> Input<Option<u64>> {
    Input::value("reply-to", "MESSAGE_ID", |slot, value| {
        *slot = Some(parse_message_id(value)?);
        Ok(())
    })
    .help("The message this one answers")
}

pub(crate) fn to_filter() -> Input<Option<String>> {
    optional_text("to", "TARGET").help("Only messages to this agent id, agent name or pane id")
}

pub(crate) fn from_filter() -> Input<Option<String>> {
    optional_text("from", "TARGET").help("Only messages from this agent id, agent name or pane id")
}

pub(crate) fn pending() -> Input<bool> {
    Input::flag("pending", |slot, _| {
        *slot = true;
        Ok(())
    })
    .help("Only messages not yet delivered")
}

pub(crate) fn message_id() -> Input<u64> {
    Input::positional("message_id", 0, |slot, value| {
        *slot = parse_message_id(value)?;
        Ok(())
    })
    .value_name("MESSAGE_ID")
}

fn parse_message_id(value: &str) -> Result<u64, ParseError> {
    value
        .parse()
        .map_err(|_| ParseError::Usage(format!("invalid message id: {value}")))
}

pub(crate) enum Operation {
    /// The params, plus a file to read the body from and whether the sender was left to default.
    Send {
        params: MessageSendParams,
        body_file: Option<String>,
        default_sender: bool,
    },
    List(MessageListParams),
    Show {
        target: MessageTarget,
        json: bool,
    },
    Withdraw(MessageTarget),
}

fn leaf(name: &'static str, arguments: Vec<Arg>, about: &'static str) -> Command {
    crate::cli::projection::command(name, arguments).about(about)
}

pub(crate) fn command() -> Command {
    Command::new("message")
        .about("Send and read messages between agents")
        .subcommand_required(true)
        .subcommand(
            leaf("send", MessageSendProjection::arguments(), "Queue a message for an agent")
                .arg(
                    Arg::new("body-file")
                        .long("body-file")
                        .value_name("PATH")
                        .value_hint(clap::ValueHint::FilePath)
                        .help("Read the body from a file (- for stdin)"),
                )
                .group(ArgGroup::new("sender").args(["from", "user"]))
                .override_usage("herdr message send <TO> <SUMMARY> [--kind report|reply|tell] [--body TEXT|--body-file PATH] [--from TARGET|--user] [--reply-to MESSAGE_ID]")
                .after_help("TO and --from are agent ids, agent names or pane ids. The message waits until the recipient is idle, or blocked on a question with an empty composer, and is then typed into its pane as one line: [from <name>] <summary> (herdr message show <id>). Pending messages go oldest first, one per state change."),
        )
        .subcommand(leaf("list", MessageListProjection::arguments(), "List messages, oldest first"))
        .subcommand(
            leaf("show", MessageTargetProjection::arguments(), "Show a message").arg(
                Arg::new("json")
                    .long("json")
                    .action(ArgAction::SetTrue)
                    .help("Print the API response instead of text"),
            ),
        )
        .subcommand(leaf(
            "withdraw",
            MessageTargetProjection::arguments(),
            "Withdraw a message that is still pending",
        ))
}

pub(crate) fn from_matches(matches: &ArgMatches) -> Result<Operation, ParseError> {
    let Some((name, matches)) = matches.subcommand() else {
        return Err(ParseError::Usage("missing message command".into()));
    };
    Ok(match name {
        "send" => Operation::Send {
            params: MessageSendProjection::from_matches(matches)?,
            body_file: matches.get_one::<String>("body-file").cloned(),
            default_sender: !matches.contains_id("from") && !matches.get_flag("user"),
        },
        "list" => Operation::List(MessageListProjection::from_matches(matches)?),
        "show" => Operation::Show {
            target: MessageTargetProjection::from_matches(matches)?,
            json: matches.get_flag("json"),
        },
        "withdraw" => Operation::Withdraw(MessageTargetProjection::from_matches(matches)?),
        _ => return Err(ParseError::Usage(format!("unknown message command {name}"))),
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
    fn kinds_cover_the_api_enum() {
        crate::cli::projection::assert_choices_cover_api_enum(KINDS);
        command().debug_assert();
    }

    #[test]
    fn send_parses_flags_and_keeps_a_flag_shaped_summary() {
        let Ok(Operation::Send {
            params,
            body_file,
            default_sender,
        }) = parse(&[
            "send",
            "1234abcd",
            "--not-a-flag",
            "--kind",
            "report",
            "--from",
            "w1:p2",
            "--reply-to",
            "7",
            "--body-file",
            "notes.md",
        ])
        else {
            panic!("send parses");
        };
        assert_eq!(params.to, "1234abcd");
        assert_eq!(params.summary, "--not-a-flag");
        assert_eq!(params.kind, MessageKind::Report);
        assert_eq!(params.from.as_deref(), Some("w1:p2"));
        assert_eq!(params.reply_to, Some(7));
        assert_eq!(body_file.as_deref(), Some("notes.md"));
        assert!(!default_sender);
    }

    #[test]
    fn send_defaults_to_a_tell_from_this_pane_unless_sent_as_the_user() {
        let Ok(Operation::Send {
            params,
            default_sender,
            ..
        }) = parse(&["send", "worker", "hello"])
        else {
            panic!("send parses");
        };
        assert_eq!(params.kind, MessageKind::Tell);
        assert!(params.from.is_none());
        assert!(default_sender);

        let Ok(Operation::Send {
            params,
            default_sender,
            ..
        }) = parse(&["send", "worker", "hello", "--user"])
        else {
            panic!("send parses");
        };
        assert!(params.from.is_none());
        assert!(!default_sender);
    }

    #[test]
    fn send_rejects_conflicting_and_malformed_flags() {
        for args in [
            &["send", "worker", "hi", "--from", "w1:p1", "--user"][..],
            &["send", "worker", "hi", "--body", "x", "--body-file", "f"],
            &["send", "worker", "hi", "--kind", "question"],
            &["send", "worker"],
        ] {
            assert!(
                matches!(parse(args), Err(ParseError::Clap(_))),
                "{args:?} is refused"
            );
        }
        assert!(matches!(
            parse(&["send", "worker", "hi", "--reply-to", "seven"]),
            Err(ParseError::Usage(_))
        ));
    }

    #[test]
    fn list_and_show_parse() {
        let Ok(Operation::List(list)) = parse(&["list", "--to", "worker", "--pending"]) else {
            panic!("list parses");
        };
        assert_eq!(list.to.as_deref(), Some("worker"));
        assert!(list.pending);
        let Ok(Operation::Show { target, json }) = parse(&["show", "12", "--json"]) else {
            panic!("show parses");
        };
        assert_eq!(target.message_id, 12);
        assert!(json);
    }
}
