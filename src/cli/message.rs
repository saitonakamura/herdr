use std::io::Read;

use crate::api::schema::{Method, Request};

pub(crate) mod bindings;

pub(crate) fn command() -> clap::Command {
    bindings::command()
}

pub(super) fn run_message_command(args: &[String]) -> std::io::Result<i32> {
    use super::projection::ParseError;
    use bindings::Operation;

    let operation = match super::projection::parse(
        command().bin_name("herdr message"),
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
    match operation {
        Operation::Send {
            mut params,
            body_file,
            default_sender,
        } => {
            if let Some(path) = body_file {
                params.body = Some(read_body(&path)?);
            }
            if default_sender {
                params.from = super::target::caller_pane_id();
            }
            super::print_response(&super::send_request(&Request {
                id: "cli:message:send".into(),
                method: Method::MessageSend(params),
            })?)
        }
        Operation::List(params) => super::print_response(&super::send_request(&Request {
            id: "cli:message:list".into(),
            method: Method::MessageList(params),
        })?),
        Operation::Show { target, json } => {
            let response = super::send_request(&Request {
                id: "cli:message:show".into(),
                method: Method::MessageGet(target),
            })?;
            if json || response.get("error").is_some() {
                return super::print_response(&response);
            }
            print!("{}", render_message(&response["result"]["message"]));
            Ok(0)
        }
        Operation::Withdraw(target) => super::print_response(&super::send_request(&Request {
            id: "cli:message:withdraw".into(),
            method: Method::MessageWithdraw(target),
        })?),
    }
}

fn read_body(path: &str) -> std::io::Result<String> {
    let mut body = String::new();
    if path == "-" {
        std::io::stdin().read_to_string(&mut body)?;
    } else {
        body = std::fs::read_to_string(path).map_err(|err| {
            std::io::Error::new(err.kind(), format!("cannot read --body-file {path}: {err}"))
        })?;
    }
    Ok(body.trim_end().to_owned())
}

/// A header an agent can read at a glance, then the summary and the body.
fn render_message(message: &serde_json::Value) -> String {
    let text = |key: &str| message[key].as_str().unwrap_or_default().to_owned();
    let from = match message["from_agent_id"].as_str() {
        Some(id) if id != text("from_name") => format!("{} ({id})", text("from_name")),
        _ => text("from_name"),
    };
    let mut out = format!(
        "message {}: {} from {from} to {}\n",
        message["message_id"],
        text("kind"),
        text("to_agent_id")
    );
    out.push_str(&format!("sent: {}\n", format_ms(&message["sent_at_ms"])));
    match message["delivered_at_ms"].as_u64() {
        Some(_) => out.push_str(&format!(
            "delivered: {}\n",
            format_ms(&message["delivered_at_ms"])
        )),
        None => out.push_str("delivered: pending\n"),
    }
    if let Some(reply_to) = message["reply_to"].as_u64() {
        out.push_str(&format!("reply to: {reply_to}\n"));
    }
    out.push('\n');
    out.push_str(&text("summary"));
    out.push('\n');
    if let Some(body) = message["body"].as_str().filter(|body| !body.is_empty()) {
        out.push('\n');
        out.push_str(body);
        out.push('\n');
    }
    out
}

fn format_ms(value: &serde_json::Value) -> String {
    let Some(ms) = value.as_u64() else {
        return "unknown".into();
    };
    time::OffsetDateTime::from_unix_timestamp_nanos(i128::from(ms) * 1_000_000)
        .ok()
        .and_then(|at| {
            at.format(&time::format_description::well_known::Rfc3339)
                .ok()
        })
        .unwrap_or_else(|| ms.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_renders_as_header_summary_and_body() {
        let message = serde_json::json!({
            "message_id": 3,
            "kind": "report",
            "state": "delivered",
            "from_agent_id": "1234abcd",
            "from_name": "worker",
            "to_agent_id": "2345bcde",
            "summary": "done: tests pass",
            "body": "details\nmore",
            "reply_to": 2,
            "sent_at_ms": 0,
            "delivered_at_ms": 1000,
        });
        assert_eq!(
            render_message(&message),
            "message 3: report from worker (1234abcd) to 2345bcde\n\
             sent: 1970-01-01T00:00:00Z\n\
             delivered: 1970-01-01T00:00:01Z\n\
             reply to: 2\n\
             \n\
             done: tests pass\n\
             \n\
             details\nmore\n"
        );
    }

    #[test]
    fn a_pending_message_from_the_user_renders_without_a_body() {
        let message = serde_json::json!({
            "message_id": 4,
            "kind": "tell",
            "state": "pending",
            "from_name": "user",
            "to_agent_id": "2345bcde",
            "summary": "heads up",
            "sent_at_ms": 0,
        });
        assert_eq!(
            render_message(&message),
            "message 4: tell from user to 2345bcde\n\
             sent: 1970-01-01T00:00:00Z\n\
             delivered: pending\n\
             \n\
             heads up\n"
        );
    }
}
