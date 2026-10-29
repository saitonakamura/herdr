use crate::api::schema::{Method, Request};

pub(crate) mod bindings;

pub(crate) fn command() -> clap::Command {
    bindings::command()
}

pub(super) fn run_wait_command(args: &[String]) -> std::io::Result<i32> {
    use super::projection::ParseError;
    use bindings::Operation;

    let operation = match super::projection::parse(
        command().bin_name("herdr wait"),
        args,
        bindings::from_matches,
    ) {
        Ok(operation) => operation,
        Err(ParseError::Clap(error)) => return super::print_clap_error(&error),
        Err(ParseError::Usage(message)) => {
            eprintln!("{message}");
            return Ok(2);
        }
        Err(ParseError::Io(error)) => return Err(error),
    };
    let (id, method) = match operation {
        Operation::Declare(params) => ("cli:wait:declare", Method::WaitDeclare(params)),
        Operation::List(params) => ("cli:wait:list", Method::WaitList(params)),
        Operation::Show(target) => ("cli:wait:show", Method::WaitGet(target)),
        Operation::Extend {
            mut params,
            default_by,
        } => {
            if default_by {
                params.by = super::target::caller_pane_id();
            }
            ("cli:wait:extend", Method::WaitExtend(params))
        }
        Operation::Cancel {
            mut params,
            default_by,
        } => {
            if default_by {
                params.by = super::target::caller_pane_id();
            }
            ("cli:wait:cancel", Method::WaitCancel(params))
        }
    };
    super::print_response(&super::send_request(&Request {
        id: id.into(),
        method,
    })?)
}
