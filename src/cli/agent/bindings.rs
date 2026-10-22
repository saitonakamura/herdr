use clap::{ArgGroup, ArgMatches, Command};

use crate::api::schema::agents::{
    AgentCloseProjection, AgentPromptProjection, AgentPromptWaitProjection, AgentReadProjection,
    AgentRenameProjection, AgentSendKeysProjection, AgentSetArgsProjection,
    AgentSetParentProjection, AgentSetTaskStatusProjection, AgentStartProjection,
    AgentWaitProjection,
};
use crate::api::schema::{
    AgentCloseParams, AgentPromptParams, AgentReadParams, AgentRenameParams, AgentSendKeysParams,
    AgentSetArgsParams, AgentSetParentParams, AgentSetTaskStatusParams, AgentStartParams,
    AgentStatus, AgentTarget, AgentWaitParams, ReadFormat, ReadSource, TaskStatus, READ_FORMATS,
    READ_SOURCES,
};
pub(crate) use crate::cli::projection::optional_text;
use crate::cli::projection::{
    choices, cli_projection, parse_choice, Choice, Field, Input, ParseError,
};

const STATUSES: &[Choice<AgentStatus>] = &[
    Choice::new("idle", AgentStatus::Idle),
    Choice::new("working", AgentStatus::Working),
    Choice::new("awaiting", AgentStatus::Awaiting),
    Choice::new("blocked", AgentStatus::Blocked),
    Choice::new("done", AgentStatus::Done),
    Choice::new("unknown", AgentStatus::Unknown),
];
const TASK_STATUSES: &[Choice<TaskStatus>] = &[
    Choice::new("starting", TaskStatus::Starting),
    Choice::new("running", TaskStatus::Running),
    Choice::new("needs_parent", TaskStatus::NeedsParent),
    Choice::new("needs_user", TaskStatus::NeedsUser),
    Choice::new("complete", TaskStatus::Complete),
    Choice::new("failed", TaskStatus::Failed),
    Choice::new("closed", TaskStatus::Closed),
];
const EXPLAIN_FORMATS: &[Choice<bool>] = &[Choice::new("text", false), Choice::new("json", true)];

pub(crate) fn target() -> Input<String> {
    text_position("target", "TARGET", 0)
}

pub(crate) fn text_position(
    name: &'static str,
    value_name: &'static str,
    index: usize,
) -> Input<String> {
    Input::positional(name, index, |slot, value| {
        *slot = value.to_owned();
        Ok(())
    })
    .value_name(value_name)
}

pub(crate) fn until() -> Input<Vec<AgentStatus>> {
    Input::<Vec<AgentStatus>>::value("until", "STATUS", |slot, value| {
        slot.push(parse_choice(STATUSES, "agent status", value)?);
        Ok(())
    })
    .configure(|arg| choices(arg, STATUSES))
}

pub(crate) fn timeout() -> Input<Option<u64>> {
    Input::value("timeout", "MS", |slot, value| {
        *slot = Some(super::super::parse_u64_flag("--timeout", value)?);
        Ok(())
    })
    .configure(|arg| arg.value_parser(clap::builder::ValueParser::new(validate_timeout)))
}

fn validate_timeout(value: &str) -> Result<String, String> {
    super::super::parse_u64_flag("--timeout", value)
        .map(|_| value.to_owned())
        .map_err(|error| error.to_string())
}

pub(crate) fn source() -> Input<ReadSource> {
    Input::value("source", "SOURCE", |slot, value| {
        *slot = parse_choice(READ_SOURCES, "read source", value)?;
        Ok(())
    })
    .configure(|arg| choices(arg, READ_SOURCES))
}

pub(crate) fn format() -> Input<ReadFormat> {
    Input::value("format", "FORMAT", |slot, value| {
        *slot = parse_choice(READ_FORMATS, "read format", value)?;
        Ok(())
    })
    .configure(|arg| choices(arg, READ_FORMATS))
}

pub(crate) fn ansi() -> Input<ReadFormat> {
    Input::flag("ansi", |slot, _| {
        *slot = ReadFormat::Ansi;
        Ok(())
    })
}

pub(crate) fn lines() -> Input<Option<u32>> {
    Input::value("lines", "N", |slot, value| {
        *slot = Some(super::super::parse_u32_flag("--lines", value)?);
        Ok(())
    })
}

pub(crate) fn keys() -> Input<Vec<String>> {
    Input::<Vec<String>>::positional("key", 1, |slot, value| {
        slot.push(value.to_owned());
        Ok(())
    })
    .value_name("KEY")
    .configure(|arg| arg.num_args(1..))
    .verbatim()
}

pub(crate) fn agent_args() -> Input<Vec<String>> {
    Input::<Vec<String>>::positional("agent_args", 1, |slot, value| {
        slot.push(value.to_owned());
        Ok(())
    })
    .value_name("AGENT_ARG")
    .configure(|arg| arg.required(false).num_args(0..).last(true))
}

pub(crate) fn kind() -> Input<String> {
    Input::value("kind", "KIND", |slot, value| {
        *slot = value.to_owned();
        Ok(())
    })
    .required()
    .configure(|arg| arg.value_parser(AgentKindCodec))
    .help("Supported agent kind and canonical executable")
}

/// Lists kinds for help and completion. Only the final `--kind` is validated, by
/// `agent_start`, so an overwritten earlier value is not rejected.
#[derive(Clone)]
struct AgentKindCodec;

impl clap::builder::TypedValueParser for AgentKindCodec {
    type Value = String;

    fn parse_ref(
        &self,
        command: &Command,
        argument: Option<&clap::Arg>,
        value: &std::ffi::OsStr,
    ) -> Result<String, clap::Error> {
        clap::builder::StringValueParser::new().parse_ref(command, argument, value)
    }

    fn possible_values(
        &self,
    ) -> Option<Box<dyn Iterator<Item = clap::builder::PossibleValue> + '_>> {
        Some(Box::new(crate::detect::Agent::ALL.into_iter().map(
            |agent| clap::builder::PossibleValue::new(crate::detect::agent_label(agent)),
        )))
    }
}

pub(crate) fn pane() -> Input<String> {
    Input::value("pane", "ID", |slot, value| {
        *slot = super::super::normalize_pane_id(value);
        Ok(())
    })
    .required()
    .help("Existing pane at an interactive shell prompt")
}

pub(crate) fn rename_name() -> Input<Option<String>> {
    Input::positional("name", 1, |slot, value| {
        *slot = Some(value.to_owned());
        Ok(())
    })
    .value_name("NAME")
    .verbatim()
    .configure(|arg| arg.required(false).conflicts_with("clear"))
}

pub(crate) fn clear_name() -> Input<Option<String>> {
    Input::flag("clear", |slot, _| {
        *slot = None;
        Ok(())
    })
}

pub(crate) fn parent_flag() -> Input<Option<String>> {
    optional_text("parent", "AGENT_ID").help("Agent id of the agent that starts this one")
}

pub(crate) fn parent_position() -> Input<Option<String>> {
    Input::positional("parent", 1, |slot, value| {
        *slot = Some(value.to_owned());
        Ok(())
    })
    .value_name("PARENT")
    .configure(|arg| arg.required(false).conflicts_with("clear"))
}

pub(crate) fn clear_parent() -> Input<Option<String>> {
    Input::flag("clear", |slot, _| {
        *slot = None;
        Ok(())
    })
    .help("Make the agent top-level")
}

pub(crate) fn task_status() -> Input<TaskStatus> {
    Input::positional("status", 1, |slot, value| {
        *slot = parse_choice(TASK_STATUSES, "task status", value)?;
        Ok(())
    })
    .value_name("STATUS")
    .configure(|arg| choices(arg, TASK_STATUSES))
}

pub(crate) fn close_targets() -> Input<Vec<String>> {
    Input::<Vec<String>>::positional("target", 0, |slot, value| {
        slot.push(value.to_owned());
        Ok(())
    })
    .value_name("TARGET")
    // Variadic, so a hyphen value would swallow --force and --dry-run after the targets.
    .configure(|arg| arg.num_args(1..).allow_hyphen_values(false))
}

pub(crate) fn wait_presence() -> Input<Option<crate::api::schema::AgentPromptWaitOptions>> {
    Input::flag("wait", |slot, _| {
        *slot = Some(crate::api::schema::AgentPromptWaitOptions {
            until: Vec::new(),
            timeout_ms: None,
            submission_deadline: None,
        });
        Ok(())
    })
    .help("Wait for the first matching state observed after submission")
}

cli_projection! {
    pub(crate) TargetProjection for AgentTarget {
        target => Field::new(String::new()).input(target()),
    }
}

pub(crate) struct Attach {
    pub(crate) target: String,
    pub(crate) takeover: bool,
}

cli_projection! {
    AttachProjection for Attach {
        target => Field::new(String::new()).input(target()),
        takeover => Field::new(false).input(Input::flag("takeover", |slot, _| { *slot = true; Ok(()) })),
    }
}

pub(crate) struct Explain {
    pub(crate) target: Option<String>,
    pub(crate) file: Option<String>,
    pub(crate) agent: Option<String>,
    pub(crate) json: bool,
    pub(crate) verbose: bool,
}

cli_projection! {
    ExplainProjection for Explain {
        target => Field::new(None).input(Input::positional("target", 0, |slot, value| {
            *slot = Some(value.to_owned()); Ok(())
        }).value_name("TARGET").configure(|arg| arg.required(false).allow_hyphen_values(false).conflicts_with("file"))),
        file => Field::new(None).input(optional_text("file", "PATH").path().configure(|arg| arg.requires("agent"))),
        agent => Field::new(None).input(optional_text("agent", "LABEL").configure(|arg| arg.requires("file").conflicts_with("target"))),
        json => Field::new(false)
            .input(Input::flag("json", |slot, _| { *slot = true; Ok(()) }))
            .input(Input::value("format", "FORMAT", |slot, value| { *slot = parse_choice(EXPLAIN_FORMATS, "explain format", value)?; Ok(()) })
                .configure(|arg| choices(arg, EXPLAIN_FORMATS))),
        verbose => Field::new(false).input(Input::flag("verbose", |slot, _| { *slot = true; Ok(()) }).configure(|arg| arg.short('v'))),
    }
}

pub(crate) enum Operation {
    List,
    Get(AgentTarget),
    Focus(AgentTarget),
    Read(AgentReadParams),
    SendKeys(AgentSendKeysParams),
    Rename(AgentRenameParams),
    SetArgs(AgentSetArgsParams),
    SetParent(AgentSetParentParams),
    SetTaskStatus(AgentSetTaskStatusParams),
    Close(AgentCloseParams),
    Wait(AgentWaitParams),
    Prompt(AgentPromptParams),
    Start(AgentStartParams),
    Attach(Attach),
    Explain(Explain),
}

fn leaf(name: &'static str, arguments: Vec<clap::Arg>, about: &'static str) -> Command {
    crate::cli::projection::command(name, arguments).about(about)
}

pub(crate) fn command() -> Command {
    Command::new("agent")
        .about("Control and inspect agent panes")
        .subcommand_required(true)
        .subcommand(leaf("list", Vec::new(), "List agents"))
        .subcommand(leaf("get", TargetProjection::arguments(), "Show an agent"))
        .subcommand(leaf("focus", TargetProjection::arguments(), "Focus an agent"))
        .subcommand(leaf("read", AgentReadProjection::arguments(), "Read agent terminal output")
            .override_usage("herdr agent read <TARGET> [OPTIONS]"))
        .subcommand(leaf("send-keys", AgentSendKeysProjection::arguments(), "Send key presses to an agent")
            .after_help("Use esc as the canonical Escape key name; escape is also accepted."))
        .subcommand(leaf("rename", AgentRenameProjection::arguments(), "Rename an agent")
            .override_usage("herdr agent rename <TARGET> <NAME>|--clear")
            .group(ArgGroup::new("rename").args(["name", "clear"]).required(true)))
        .subcommand(leaf("set-args", AgentSetArgsProjection::arguments(), "Replace the args a resume starts an agent with")
            .override_usage("herdr agent set-args <TARGET> --kind <KIND> [-- <AGENT_ARG>...]")
            .after_help("TARGET is an agent id, agent name or pane id. A resume adds the args after the session it resumes, minus those that select a session or run headless. With no AGENT_ARG, the agent resumes with none."))
        .subcommand(leaf("set-parent", AgentSetParentProjection::arguments(), "Set or clear an agent's parent")
            .override_usage("herdr agent set-parent <TARGET> <PARENT>|--clear")
            .group(ArgGroup::new("set_parent").args(["parent", "clear"]).required(true))
            .after_help("TARGET is an agent id, agent name or pane id; a pane without an agent gets an agent id too. PARENT is an agent id. When the parent's pane closes, the agent becomes top-level."))
        .subcommand(leaf("set-task-status", AgentSetTaskStatusProjection::arguments(), "Record what an agent says about its brief")
            .override_usage("herdr agent set-task-status <TARGET> <STATUS>")
            .after_help("TARGET is an agent id, agent name or pane id; a pane without an agent gets an agent id too. The task status sits next to the state herdr detects and never changes it."))
        .subcommand(leaf("close", AgentCloseProjection::arguments(), "Close agents with their whole subtree, deepest first")
            .override_usage("herdr agent close <TARGET>... [--force] [--dry-run]")
            .after_help("Each TARGET closes with every descendant (by parent). Refused with agent_subtree_busy while any of them is working or blocked, unless --force; the pane running this command is never counted and closes last. Closing an agent closes its pane, and the tab or workspace too when that pane is the last one left in it."))
        .subcommand(leaf("wait", AgentWaitProjection::arguments(), "Wait until an agent reaches one of the requested states")
            .override_usage("herdr agent wait <TARGET> [OPTIONS]")
            .after_help("Without --until, matches idle, done, awaiting, or blocked. Use --until unknown explicitly when needed. Without --timeout, waits indefinitely."))
        .subcommand(leaf("prompt", AgentPromptProjection::arguments(), "Submit a prompt to an agent")
            .args(AgentPromptWaitProjection::arguments().into_iter().map(|arg| arg.requires("wait")))
            .override_usage("herdr agent prompt <TARGET> <TEXT> [OPTIONS]")
            .after_help("If the agent is already blocked, submission is rejected before any input is sent: agent_blocked_permission or agent_blocked_form when its integration or matched detection rule says what it waits on, otherwise agent_blocked. An agent blocked on a question takes the prompt as its answer when its composer is empty, and is refused with agent_composer_not_empty or agent_composer_not_found otherwise. When an accepted submission starts from another non-working state, --wait requires an observed working or blocked state within 5000ms; otherwise it returns agent_prompt_stalled. A caller timeout that expires first returns timeout. It then matches idle, done, or blocked by default, or any exact --until state. It does not track turns: if the agent is already working, that active turn's completion may match."))
        .subcommand(leaf("start", AgentStartProjection::arguments(), "Start a supported interactive agent in an existing pane")
            .override_usage("herdr agent start <NAME> --kind <KIND> --pane <ID> [OPTIONS] [-- [AGENT_ARG]...]")
            .after_help("The pane must be at its interactive shell prompt. Success means the expected agent was detected in the same terminal and is ready for input.\n\nnext: herdr agent prompt <TARGET> <TEXT> --wait"))
        .subcommand(leaf("attach", AttachProjection::arguments(), "Attach directly to an agent terminal")
            .override_usage("herdr agent attach <TARGET> [OPTIONS]"))
        .subcommand(leaf("explain", ExplainProjection::arguments(), "Explain agent detection state")
            .group(ArgGroup::new("explain_source").args(["target", "file"]).required(true)))
}

pub(crate) fn from_matches(matches: &ArgMatches) -> Result<Operation, ParseError> {
    let Some((name, matches)) = matches.subcommand() else {
        return Err(ParseError::Usage("missing agent command".into()));
    };
    Ok(match name {
        "list" => Operation::List,
        "get" => Operation::Get(TargetProjection::from_matches(matches)?),
        "focus" => Operation::Focus(TargetProjection::from_matches(matches)?),
        "read" => {
            let mut params = AgentReadProjection::from_matches(matches)?;
            params.strip_ansi = !matches!(params.format, ReadFormat::Ansi);
            Operation::Read(params)
        }
        "send-keys" => Operation::SendKeys(AgentSendKeysProjection::from_matches(matches)?),
        "rename" => Operation::Rename(AgentRenameProjection::from_matches(matches)?),
        "set-args" => Operation::SetArgs(AgentSetArgsProjection::from_matches(matches)?),
        "set-parent" => Operation::SetParent(AgentSetParentProjection::from_matches(matches)?),
        "set-task-status" => {
            Operation::SetTaskStatus(AgentSetTaskStatusProjection::from_matches(matches)?)
        }
        "close" => Operation::Close(AgentCloseProjection::from_matches(matches)?),
        "wait" => Operation::Wait(AgentWaitProjection::from_matches(matches)?),
        "prompt" => {
            let mut params = AgentPromptProjection::from_matches(matches)?;
            if params.wait.is_some() {
                params.wait = Some(AgentPromptWaitProjection::from_matches(matches)?);
            }
            Operation::Prompt(params)
        }
        "start" => Operation::Start(AgentStartProjection::from_matches(matches)?),
        "attach" => Operation::Attach(AttachProjection::from_matches(matches)?),
        "explain" => Operation::Explain(ExplainProjection::from_matches(matches)?),
        _ => return Err(ParseError::Usage(format!("unknown agent command: {name}"))),
    })
}

#[cfg(test)]
mod choice_tests {
    #[test]
    fn agent_statuses_cover_every_api_value() {
        crate::cli::projection::assert_choices_cover_api_enum(super::STATUSES);
    }

    #[test]
    fn task_statuses_cover_every_api_value() {
        crate::cli::projection::assert_choices_cover_api_enum(super::TASK_STATUSES);
    }

    #[test]
    fn close_takes_several_targets_with_flags_on_either_side() {
        let parse = |args: &[&str]| {
            let matches = super::command()
                .try_get_matches_from(["agent", "close"].iter().chain(args))
                .unwrap();
            match super::from_matches(&matches) {
                Ok(super::Operation::Close(params)) => params,
                _ => panic!("not a close: {args:?}"),
            }
        };

        let params = parse(&["1234abcd", "w1:p2", "--dry-run"]);
        assert_eq!(params.targets, ["1234abcd", "w1:p2"]);
        assert!(params.dry_run && !params.force);
        let params = parse(&["--force", "1234abcd"]);
        assert_eq!(params.targets, ["1234abcd"]);
        assert!(params.force && !params.dry_run);
    }
}
