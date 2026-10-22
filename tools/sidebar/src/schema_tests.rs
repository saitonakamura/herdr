//! The sidebar's API types against herdr's generated schema
//! (`docs/next/api/herdr-api.schema.json`, kept current by herdr's own tests).
//! A field the sidebar reads that herdr renames or drops fails here.

use serde_json::{json, Value};

use crate::api;

const SCHEMA: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/next/api/herdr-api.schema.json"
));

fn herdr() -> Value {
    serde_json::from_str(SCHEMA).unwrap()
}

fn definition<'a>(schema: &'a Value, section: &str, name: &str) -> &'a Value {
    let def = &schema["schemas"][section]["$defs"][name];
    assert!(def.is_object(), "herdr's {section} schema has no {name}");
    def
}

fn keys(value: &Value) -> Vec<String> {
    value
        .as_object()
        .map(|map| map.keys().cloned().collect())
        .unwrap_or_default()
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn assert_struct_matches<T: schemars::JsonSchema>(herdr_name: &str) {
    let schema = herdr();
    let theirs = definition(&schema, "success_response", herdr_name);
    let ours = serde_json::to_value(schemars::schema_for!(T)).unwrap();
    let their_props = keys(&theirs["properties"]);
    let their_required = strings(&theirs["required"]);
    for field in keys(&ours["properties"]) {
        assert!(
            their_props.contains(&field),
            "{herdr_name}.{field}: herdr no longer has this field"
        );
    }
    for field in strings(&ours["required"]) {
        assert!(
            their_required.contains(&field),
            "{herdr_name}.{field}: the sidebar requires it but herdr may omit it"
        );
    }
}

#[test]
fn read_types_are_a_subset_of_herdrs() {
    assert_struct_matches::<api::SessionSnapshot>("SessionSnapshot");
    assert_struct_matches::<api::WorkspaceInfo>("WorkspaceInfo");
    assert_struct_matches::<api::TabInfo>("TabInfo");
    assert_struct_matches::<api::PaneInfo>("PaneInfo");
    assert_struct_matches::<api::PaneRepoInfo>("PaneRepoInfo");
    assert_struct_matches::<api::MessageInfo>("MessageInfo");
}

fn assert_enum_covers<T: serde::de::DeserializeOwned + PartialEq + std::fmt::Debug>(
    herdr_name: &str,
    unknown: T,
) {
    let schema = herdr();
    let values = strings(&definition(&schema, "success_response", herdr_name)["enum"]);
    assert!(!values.is_empty(), "{herdr_name} has no values");
    for value in values {
        let decoded: T = serde_json::from_value(json!(value)).unwrap();
        if value != "unknown" {
            assert_ne!(
                decoded, unknown,
                "{herdr_name} {value:?} decodes as unknown"
            );
        }
    }
}

#[test]
fn every_herdr_enum_value_has_a_meaning() {
    assert_enum_covers("AgentStatus", api::AgentStatus::Unknown);
    assert_enum_covers("AgentBlockedReason", api::BlockedReason::Unknown);
    assert_enum_covers("TaskStatus", api::TaskStatus::Unknown);
    assert_enum_covers("MessageKind", api::MessageKind::Unknown);
}

#[test]
fn subscriptions_are_events_herdr_accepts() {
    let schema = herdr();
    let accepted: Vec<String> = schema["schemas"]["request"]["$defs"]["Subscription"]["oneOf"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|variant| {
            variant["properties"]["type"]["const"]
                .as_str()
                .map(str::to_string)
        })
        .collect();
    for kind in api::SUBSCRIPTIONS {
        assert!(
            accepted.contains(&kind.to_string()),
            "{kind} is not a subscription"
        );
    }
}

#[test]
fn subscription_entries_carry_what_herdr_requires() {
    let schema = herdr();
    let variants = schema["schemas"]["request"]["$defs"]["Subscription"]["oneOf"]
        .as_array()
        .unwrap()
        .clone();
    let entries = api::lifecycle_subscriptions()
        .into_iter()
        .chain(api::status_subscriptions(&["w1:p1".to_string()]));
    for entry in entries {
        let kind = entry["type"].as_str().unwrap();
        let variant = variants
            .iter()
            .find(|v| v["properties"]["type"]["const"] == kind)
            .unwrap_or_else(|| panic!("{kind} is not a subscription"));
        for key in strings(&variant["required"]) {
            assert!(entry.get(&key).is_some(), "{kind}: herdr requires {key}");
        }
        for key in keys(&entry) {
            assert!(
                keys(&variant["properties"]).contains(&key),
                "{kind}: herdr has no {key}"
            );
        }
    }
}

fn assert_request(schema: &Value, method: &str, params: &Value) {
    let variant = schema["schemas"]["request"]["oneOf"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["properties"]["method"]["const"] == method)
        .unwrap_or_else(|| panic!("herdr has no method {method}"));
    let reference = variant["properties"]["params"]["$ref"].as_str().unwrap();
    let name = reference.rsplit('/').next().unwrap();
    let def = definition(schema, "request", name);
    let props = keys(&def["properties"]);
    for key in keys(params) {
        assert!(
            props.contains(&key),
            "{method}: herdr's {name} has no {key}"
        );
    }
    for key in strings(&def["required"]) {
        assert!(params.get(&key).is_some(), "{method}: {key} is required");
    }
}

/// Every request the sidebar sends: those the app builds, driven through it,
/// and the runner's own.
#[test]
fn requests_match_herdrs_methods_and_params() {
    use crate::app::{App, Effect};
    use crate::config::from_table;
    use crate::model::{fixtures::snapshot, UiState};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let mut sent: Vec<(String, Value)> = vec![
        ("session.snapshot".into(), json!({})),
        ("message.list".into(), json!({ "pending": true })),
        ("message.list".into(), json!({ "from": "a" })),
        (
            "message.send".into(),
            json!({ "to": "a", "summary": "s", "kind": "reply", "reply_to": 1 }),
        ),
        ("session_pane.unfocus".into(), json!({})),
        (
            "agent.close".into(),
            json!({ "targets": ["a"], "dry_run": true }),
        ),
        (
            "agent.close".into(),
            json!({ "targets": ["a"], "force": true }),
        ),
        ("popup.open".into(), json!({ "argv": ["loom"] })),
        ("events.subscribe".into(), json!({ "subscriptions": [] })),
    ];
    let mut app = App::new(from_table(&toml::Table::new()), UiState::default());
    app.set_snapshot(snapshot());
    let press = |app: &mut App, code: KeyCode| app.key(KeyEvent::new(code, KeyModifiers::NONE));
    let mut collect = |effects: Vec<Effect>| {
        for effect in effects {
            if let Effect::Call(method, params) = effect {
                sent.push((method.to_string(), params));
            }
        }
    };
    for key in ["ws:w1", "pane:w1:p3", "pane:w1:p2", "new"] {
        app.selected = Some(key.into());
        collect(press(&mut app, KeyCode::Enter));
    }
    app.selected = Some("pane:w1:p3".into());
    collect(press(&mut app, KeyCode::Char('p')));
    press(&mut app, KeyCode::Char('t'));
    press(&mut app, KeyCode::Char('x'));
    collect(press(&mut app, KeyCode::Enter));
    // the header menu: rename, then new worktree
    for down in [0, 2] {
        app.selected = Some("ws:w1".into());
        press(&mut app, KeyCode::Char('m'));
        for _ in 0..down {
            press(&mut app, KeyCode::Char('j'));
        }
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('x'));
        collect(press(&mut app, KeyCode::Enter));
    }
    app.selected = Some("ws:w1".into());
    press(&mut app, KeyCode::Char('m'));
    press(&mut app, KeyCode::Char('j'));
    press(&mut app, KeyCode::Enter);
    let effects = press(&mut app, KeyCode::Char('y'));
    collect(effects);

    let methods: Vec<&str> = sent.iter().map(|(m, _)| m.as_str()).collect();
    for expected in [
        "workspace.focus",
        "pane.focus",
        "workspace.create",
        "plugin.pane.open",
        "workspace.rename",
        "worktree.create",
        "workspace.close",
    ] {
        assert!(
            methods.contains(&expected),
            "{expected} was not exercised: {methods:?}"
        );
    }
    let schema = herdr();
    for (method, params) in &sent {
        assert_request(&schema, method, params);
    }
}
