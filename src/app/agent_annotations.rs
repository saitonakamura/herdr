use std::collections::BTreeMap;

use super::App;
use crate::api::schema::{ErrorBody, PaneInfo};

pub(crate) const MAX_ANNOTATIONS_PER_AGENT: usize = 32;
const MAX_ANNOTATION_KEY_LEN: usize = 32;
const MAX_ANNOTATION_VALUE_CHARS: usize = 512;

impl App {
    /// Applies an annotation patch to the target's agent: a `None` or empty
    /// value clears its key, and keys not named stay as they are.
    pub(super) fn annotate_agent(
        &mut self,
        target: &str,
        patch: BTreeMap<String, Option<String>>,
    ) -> Result<PaneInfo, ErrorBody> {
        let patch = normalize_annotations(patch)?;
        let (ws_idx, pane_id, terminal_id) = self
            .resolve_agent_identity_target(target)
            .map_err(|err| self.agent_parent_error_body(err))?;
        let current = self
            .state
            .terminals
            .get(&terminal_id)
            .map(|terminal| terminal.annotations.clone())
            .unwrap_or_default();
        let next = apply_patch(current.clone(), patch)?;
        let mut changed = self.assign_agent_identity(&terminal_id, None);
        if next != current {
            if let Some(terminal) = self.state.terminals.get_mut(&terminal_id) {
                terminal.annotations = next;
                changed = true;
            }
        }
        if changed {
            self.state.mark_session_dirty();
            self.schedule_session_save();
            self.emit_pane_updated(ws_idx, pane_id);
        }
        self.pane_info(ws_idx, pane_id).ok_or_else(|| ErrorBody {
            code: "agent_not_found".into(),
            message: format!("agent {target} not found"),
        })
    }
}

/// Checks a patch against the limits without altering any value: annotations
/// are stored exactly as given.
fn normalize_annotations(
    patch: BTreeMap<String, Option<String>>,
) -> Result<BTreeMap<String, Option<String>>, ErrorBody> {
    if patch.is_empty() {
        return Err(invalid("missing annotation to set or clear".into()));
    }
    patch
        .into_iter()
        .map(|(key, value)| {
            if key.is_empty()
                || key.len() > MAX_ANNOTATION_KEY_LEN
                || !key
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-'))
            {
                return Err(invalid(format!(
                    "invalid annotation key `{key}`: use 1-{MAX_ANNOTATION_KEY_LEN} of A-Z a-z 0-9 _ -"
                )));
            }
            let value = value.filter(|value| !value.is_empty());
            if let Some(value) = &value {
                if value.chars().count() > MAX_ANNOTATION_VALUE_CHARS {
                    return Err(invalid(format!(
                        "annotation `{key}` is longer than {MAX_ANNOTATION_VALUE_CHARS} characters"
                    )));
                }
                if value.chars().any(char::is_control) {
                    return Err(invalid(format!(
                        "annotation `{key}` contains a control character"
                    )));
                }
            }
            Ok((key, value))
        })
        .collect()
}

fn apply_patch(
    mut annotations: BTreeMap<String, String>,
    patch: BTreeMap<String, Option<String>>,
) -> Result<BTreeMap<String, String>, ErrorBody> {
    for (key, value) in patch {
        match value {
            Some(value) => {
                annotations.insert(key, value);
            }
            None => {
                annotations.remove(&key);
            }
        }
    }
    if annotations.len() > MAX_ANNOTATIONS_PER_AGENT {
        return Err(ErrorBody {
            code: "annotation_limit".into(),
            message: format!(
                "an agent keeps at most {MAX_ANNOTATIONS_PER_AGENT} annotations; clear some first"
            ),
        });
    }
    Ok(annotations)
}

fn invalid(message: String) -> ErrorBody {
    ErrorBody {
        code: "invalid_annotation".into(),
        message,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::api::schema::{
        AgentAnnotateParams, AgentTarget, EventData, EventKind, Method, PaneTarget, Request,
    };
    use crate::app::App;
    use crate::detect::{Agent, AgentState};
    use crate::layout::PaneId;

    struct Fixture {
        app: App,
        events: crate::api::EventHub,
        root: PaneId,
    }

    fn fixture() -> Fixture {
        let events = crate::api::EventHub::default();
        let (_api_tx, api_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(
            &crate::config::Config::default(),
            crate::app::AppPolicy::TEST,
            None,
            api_rx,
            events.clone(),
        );
        let workspace = crate::workspace::Workspace::test_new("annotations");
        let root = workspace.tabs[0].root_pane;
        app.state.workspaces = vec![workspace];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        let terminal_id = app.state.terminal_id_for_pane(0, root).unwrap();
        app.state
            .terminals
            .get_mut(&terminal_id)
            .unwrap()
            .set_detected_state(Some(Agent::Pi), AgentState::Idle);
        Fixture { app, events, root }
    }

    impl Fixture {
        fn call(&mut self, method: Method) -> serde_json::Value {
            let response = self.app.handle_api_request(Request {
                id: "t".into(),
                method,
            });
            serde_json::from_str(&response).unwrap()
        }

        fn target(&self) -> String {
            self.app.public_pane_id(0, self.root).unwrap()
        }

        fn annotate(&mut self, patch: &[(&str, Option<&str>)]) -> serde_json::Value {
            let target = self.target();
            self.call(Method::AgentAnnotate(AgentAnnotateParams {
                target,
                annotations: patch
                    .iter()
                    .map(|(key, value)| (key.to_string(), value.map(str::to_string)))
                    .collect(),
            }))
        }

        fn last_seq(&self) -> u64 {
            self.events
                .events_after(0)
                .last()
                .map(|(seq, _)| *seq)
                .unwrap_or(0)
        }
    }

    #[test]
    fn annotations_merge_clear_and_show_on_pane_and_agent() {
        let mut f = fixture();
        let after = f.last_seq();

        let set = f.annotate(&[
            ("role", Some("worker")),
            ("why", Some("a long brief, kept")),
        ]);
        assert_eq!(set["result"]["pane"]["annotations"]["role"], "worker");
        assert!(set["result"]["pane"]["agent_id"].is_string());
        assert!(f.events.events_after(after).iter().any(|(_, event)| {
            event.event == EventKind::PaneUpdated
                && matches!(&event.data, EventData::PaneUpdated { pane }
                    if pane.annotations.get("role").map(String::as_str) == Some("worker"))
        }));

        let patched = f.annotate(&[("why", Some("")), ("effort", Some("pt:x")), ("kind", None)]);
        assert_eq!(
            patched["result"]["pane"]["annotations"],
            serde_json::json!({"effort": "pt:x", "role": "worker"})
        );
        let target = f.target();
        let pane = f.call(Method::PaneGet(PaneTarget {
            pane_id: target.clone(),
        }));
        assert_eq!(pane["result"]["pane"]["annotations"]["effort"], "pt:x");
        let agent = f.call(Method::AgentGet(AgentTarget { target }));
        assert_eq!(agent["result"]["agent"]["annotations"]["role"], "worker");

        let cleared = f.annotate(&[("role", None), ("effort", Some(""))]);
        assert!(cleared["result"]["pane"].get("annotations").is_none());
    }

    #[test]
    fn an_unchanged_patch_publishes_nothing() {
        let mut f = fixture();
        f.annotate(&[("role", Some("worker"))]);
        let after = f.last_seq();

        f.annotate(&[("role", Some("worker")), ("gone", None)]);

        assert!(f.events.events_after(after).is_empty());
    }

    #[test]
    fn values_are_stored_verbatim_and_bad_input_changes_nothing() {
        let mut f = fixture();
        let spaced = "  keeps its blanks ";
        let set = f.annotate(&[("why", Some(spaced))]);
        assert_eq!(set["result"]["pane"]["annotations"]["why"], spaced);

        for (patch, code) in [
            (vec![("bad key", Some("v"))], "invalid_annotation"),
            (vec![("$ticket", Some("v"))], "invalid_annotation"),
            (vec![(&*"k".repeat(33), Some("v"))], "invalid_annotation"),
            (vec![("why", Some("two\nlines"))], "invalid_annotation"),
            (vec![], "invalid_annotation"),
        ] {
            let response = f.annotate(&patch);
            assert_eq!(response["error"]["code"], code, "{patch:?}");
        }
        let long = "x".repeat(513);
        let response = f.annotate(&[("why", Some(&long))]);
        assert_eq!(response["error"]["code"], "invalid_annotation");
        let max = "é".repeat(512);
        let response = f.annotate(&[("why", Some(&max))]);
        assert_eq!(
            response["result"]["pane"]["annotations"]["why"],
            max.as_str()
        );
    }

    #[test]
    fn the_count_limit_refuses_the_whole_patch() {
        let mut f = fixture();
        let keys: Vec<String> = (0..super::MAX_ANNOTATIONS_PER_AGENT)
            .map(|i| format!("k{i}"))
            .collect();
        let full: Vec<(&str, Option<&str>)> =
            keys.iter().map(|key| (key.as_str(), Some("v"))).collect();
        assert!(f.annotate(&full)["result"].is_object());

        let over = f.annotate(&[("one-more", Some("v")), ("k0", Some("changed"))]);
        assert_eq!(over["error"]["code"], "annotation_limit");
        let swap = f.annotate(&[("one-more", Some("v")), ("k0", None)]);
        let annotations = &swap["result"]["pane"]["annotations"];
        assert_eq!(annotations["k1"], "v");
        assert!(annotations.get("k0").is_none());
        assert_eq!(
            annotations.as_object().map(|map| map.len()),
            Some(super::MAX_ANNOTATIONS_PER_AGENT)
        );
    }

    #[test]
    fn an_unknown_target_is_agent_not_found() {
        let mut f = fixture();
        let response = f.call(Method::AgentAnnotate(AgentAnnotateParams {
            target: "nobody".into(),
            annotations: BTreeMap::from([("role".into(), Some("x".into()))]),
        }));
        assert_eq!(response["error"]["code"], "agent_not_found");
    }
}
