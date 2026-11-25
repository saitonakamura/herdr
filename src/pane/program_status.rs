//! OSC 7501 program status records (Program Status Protocol, rev 0.4).
//!
//! A program reports its state down its own PTY. This module keeps what one
//! pane's program reported. The root record, read through
//! [`ProgramStatusRecords::root_lifecycle`], is what the terminal state ranks
//! above hooks and screen rules.

use std::collections::BTreeMap;

use base64::Engine as _;

use crate::detect::BlockedReason;
use crate::terminal::{ProgramLifecycle, ProgramStatusReport};

/// What herdr writes back to `OSC 7501 ; ?`, without the terminator.
pub(crate) const QUERY_REPLY_BODY: &[u8] =
    b"\x1b]7501;?:states=idle,working,done,blocked,error:kinds=permission,question,auth";

/// The spec lets a terminal pick a lower cap than 256, but not below 64.
const MAX_RECORDS: usize = 64;
const MAX_KEY_BYTES: usize = 16;
const MAX_ID_BYTES: usize = 128;
const MAX_ID_SEGMENT_BYTES: usize = 32;
const MAX_ID_DEPTH: usize = 8;
const MAX_APP_BYTES: usize = 32;
const MAX_TITLE_ENCODED_BYTES: usize = 256;
const MAX_TITLE_DECODED_BYTES: usize = 192;
const MAX_MSG_ENCODED_BYTES: usize = 2732;
const MAX_MSG_DECODED_BYTES: usize = 2048;

// The spec makes padding optional.
const BASE64: base64::engine::GeneralPurpose = base64::engine::GeneralPurpose::new(
    &base64::alphabet::STANDARD,
    base64::engine::GeneralPurposeConfig::new()
        .with_decode_padding_mode(base64::engine::DecodePaddingMode::Indifferent),
);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProgramStatusState {
    Idle,
    Working,
    Done,
    Blocked,
    Error,
}

impl ProgramStatusState {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Working => "working",
            Self::Done => "done",
            Self::Blocked => "blocked",
            Self::Error => "error",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProgramStatusKind {
    Permission,
    Question,
    Auth,
}

impl ProgramStatusKind {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Permission => "permission",
            Self::Question => "question",
            Self::Auth => "auth",
        }
    }
}

/// One record as reported. `id` is empty for the root record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProgramStatusRecord {
    pub(crate) id: String,
    pub(crate) state: ProgramStatusState,
    pub(crate) kind: Option<ProgramStatusKind>,
    pub(crate) progress: Option<u8>,
    /// The record's own `app`, or its nearest ancestor's when read through
    /// [`ProgramStatusRecords::snapshot`].
    pub(crate) app: Option<String>,
    pub(crate) title: Option<String>,
    pub(crate) msg: Option<String>,
}

impl ProgramStatusRecord {
    /// What this record says about the lifecycle of the program that wrote it.
    pub(crate) fn lifecycle_report(&self) -> ProgramStatusReport {
        let lifecycle = match self.state {
            ProgramStatusState::Working => ProgramLifecycle::Working,
            ProgramStatusState::Blocked => ProgramLifecycle::Blocked(match self.kind {
                Some(ProgramStatusKind::Permission) => Some(BlockedReason::Permission),
                Some(ProgramStatusKind::Question) => Some(BlockedReason::Question),
                Some(ProgramStatusKind::Auth) => Some(BlockedReason::Auth),
                None => None,
            }),
            ProgramStatusState::Done => ProgramLifecycle::Done,
            ProgramStatusState::Idle => ProgramLifecycle::Idle,
            ProgramStatusState::Error => ProgramLifecycle::Error,
        };
        ProgramStatusReport {
            lifecycle,
            message: self.msg.clone(),
        }
    }
}

#[derive(Debug)]
struct StoredRecord {
    record: ProgramStatusRecord,
    updated: u64,
    /// Kept past the process that wrote it: shown, but no longer anyone's state.
    outlived: bool,
}

/// The records of one pane, keyed by id so a parent sorts before its children.
#[derive(Debug, Default)]
pub(crate) struct ProgramStatusRecords {
    records: BTreeMap<String, StoredRecord>,
    updates: u64,
    /// Bumped by every report or drop that touches the root record.
    root_revision: u64,
    /// The current program has reported, so its OSC 9;4 progress says nothing more.
    reporting: bool,
}

enum ReportState {
    Set(ProgramStatusState),
    Clear,
}

impl ProgramStatusRecords {
    /// Applies one report payload (the OSC body after `7501;`). A report that
    /// breaks the grammar or a limit changes nothing.
    pub(crate) fn apply_report(&mut self, payload: &[u8]) {
        let Some((id, state, mut record)) = parse_report(payload) else {
            return;
        };
        self.reporting = true;
        if id.is_empty() {
            self.root_revision += 1;
        }
        let state = match state {
            ReportState::Clear => {
                self.clear(&id);
                return;
            }
            ReportState::Set(state) => state,
        };
        record.state = state;
        self.updates += 1;
        let stored = StoredRecord {
            record,
            updated: self.updates,
            outlived: false,
        };
        if self.records.insert(id, stored).is_none() && self.records.len() > MAX_RECORDS {
            self.evict_least_recently_updated();
        }
    }

    /// RIS: every record goes.
    pub(crate) fn clear_all(&mut self) {
        self.records.clear();
        self.root_revision += 1;
    }

    /// The reporting process is gone: only `done` and `error` outlive it, and
    /// they stop being the state of whatever runs in the pane next.
    pub(crate) fn drop_live_states(&mut self) {
        self.records.retain(|_, stored| {
            stored.outlived = true;
            matches!(
                stored.record.state,
                ProgramStatusState::Done | ProgramStatusState::Error
            )
        });
        self.root_revision += 1;
        self.reporting = false;
    }

    /// The root record as the running program's lifecycle; `None` once cleared
    /// or once its process is gone.
    pub(crate) fn root_lifecycle(&self) -> Option<ProgramStatusReport> {
        self.records
            .get("")
            .filter(|stored| !stored.outlived)
            .map(|stored| stored.record.lifecycle_report())
    }

    pub(crate) fn root_revision(&self) -> u64 {
        self.root_revision
    }

    pub(crate) fn reporting(&self) -> bool {
        self.reporting
    }

    /// Records in id order, each with `app` resolved through its ancestors.
    pub(crate) fn snapshot(&self) -> Vec<ProgramStatusRecord> {
        self.records
            .values()
            .map(|stored| {
                let mut record = stored.record.clone();
                if record.app.is_none() {
                    record.app = self.inherited_app(&record.id);
                }
                record
            })
            .collect()
    }

    fn inherited_app(&self, id: &str) -> Option<String> {
        let mut ancestor = id;
        while !ancestor.is_empty() {
            ancestor = ancestor.rsplit_once('/').map_or("", |(parent, _)| parent);
            if let Some(app) = self
                .records
                .get(ancestor)
                .and_then(|stored| stored.record.app.as_ref())
            {
                return Some(app.clone());
            }
        }
        None
    }

    fn clear(&mut self, id: &str) {
        if id.is_empty() {
            self.records.clear();
            return;
        }
        self.records.retain(|key, _| {
            key != id
                && !key
                    .strip_prefix(id)
                    .is_some_and(|rest| rest.starts_with('/'))
        });
    }

    fn evict_least_recently_updated(&mut self) {
        let oldest = self
            .records
            .iter()
            .min_by_key(|(_, stored)| stored.updated)
            .map(|(id, _)| id.clone());
        if let Some(oldest) = oldest {
            if oldest.is_empty() {
                self.root_revision += 1;
            }
            self.records.remove(&oldest);
        }
    }
}

/// Validates a whole report before anything is stored. The returned record's
/// state is a placeholder the caller overwrites.
fn parse_report(payload: &[u8]) -> Option<(String, ReportState, ProgramStatusRecord)> {
    let mut state = None;
    let mut id = None;
    let mut kind: &[u8] = b"";
    let mut progress: &[u8] = b"";
    let mut app: &[u8] = b"";
    let mut title: &[u8] = b"";
    let mut msg: &[u8] = b"";
    for pair in payload.split(|&byte| byte == b':') {
        let Some(separator) = pair.iter().position(|&byte| byte == b'=') else {
            continue;
        };
        let key = pair[..separator].trim_ascii();
        let value = pair[separator + 1..].trim_ascii();
        if key.is_empty() || !key.iter().all(u8::is_ascii_lowercase) {
            continue;
        }
        if key.len() > MAX_KEY_BYTES {
            return None;
        }
        if !value.iter().copied().all(is_value_byte) {
            // Skipping a bad `id` would address the root record instead.
            if key == b"id" {
                return None;
            }
            continue;
        }
        // A repeated key keeps its last value.
        match key {
            b"state" => state = Some(value),
            b"id" => id = Some(value),
            b"kind" => kind = value,
            b"progress" => progress = value,
            b"app" => app = value,
            b"title" => title = value,
            b"msg" => msg = value,
            _ => {}
        }
    }

    let state = match state? {
        b"idle" => ReportState::Set(ProgramStatusState::Idle),
        b"working" => ReportState::Set(ProgramStatusState::Working),
        b"done" => ReportState::Set(ProgramStatusState::Done),
        b"blocked" => ReportState::Set(ProgramStatusState::Blocked),
        b"error" => ReportState::Set(ProgramStatusState::Error),
        b"clear" => ReportState::Clear,
        _ => return None,
    };
    let id = parse_id(id)?;
    let title = decode_text(title, MAX_TITLE_ENCODED_BYTES, MAX_TITLE_DECODED_BYTES)?;
    let msg = decode_text(msg, MAX_MSG_ENCODED_BYTES, MAX_MSG_DECODED_BYTES)?;

    let blocked = matches!(state, ReportState::Set(ProgramStatusState::Blocked));
    let working = matches!(state, ReportState::Set(ProgramStatusState::Working));
    let kind = match kind {
        b"permission" if blocked => Some(ProgramStatusKind::Permission),
        b"question" if blocked => Some(ProgramStatusKind::Question),
        b"auth" if blocked => Some(ProgramStatusKind::Auth),
        _ => None,
    };
    let progress = (blocked || working)
        .then(|| parse_progress(progress))
        .flatten();
    let app = is_name(app, MAX_APP_BYTES).then(|| String::from_utf8_lossy(app).into_owned());

    let record = ProgramStatusRecord {
        id: id.clone(),
        state: ProgramStatusState::Idle,
        kind,
        progress,
        app,
        title,
        msg,
    };
    Some((id, state, record))
}

fn is_value_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b',' | b'+' | b'/' | b'=' | b'-')
}

/// `[A-Za-z0-9_.+-]{1,max}`: the shape of `app` and of one `id` segment.
fn is_name(value: &[u8], max: usize) -> bool {
    !value.is_empty()
        && value.len() <= max
        && value
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'+' | b'-'))
}

/// An absent id is the root record; one that breaks the grammar voids the report.
fn parse_id(id: Option<&[u8]>) -> Option<String> {
    let Some(id) = id else {
        return Some(String::new());
    };
    if id.len() > MAX_ID_BYTES {
        return None;
    }
    let mut depth = 0;
    for segment in id.split(|&byte| byte == b'/') {
        depth += 1;
        if depth > MAX_ID_DEPTH || !is_name(segment, MAX_ID_SEGMENT_BYTES) {
            return None;
        }
    }
    Some(String::from_utf8_lossy(id).into_owned())
}

/// `None` voids the report; `Some(None)` is an absent or empty text.
fn decode_text(encoded: &[u8], max_encoded: usize, max_decoded: usize) -> Option<Option<String>> {
    if encoded.is_empty() {
        return Some(None);
    }
    if encoded.len() > max_encoded {
        return None;
    }
    let decoded = BASE64.decode(encoded).ok()?;
    if decoded.len() > max_decoded {
        return None;
    }
    let text = String::from_utf8(decoded).ok()?;
    if text.chars().any(char::is_control) {
        return None;
    }
    Some((!text.is_empty()).then_some(text))
}

fn parse_progress(value: &[u8]) -> Option<u8> {
    if value.is_empty() || value.len() > 3 || !value.iter().all(u8::is_ascii_digit) {
        return None;
    }
    let progress = value
        .iter()
        .fold(0u16, |acc, digit| acc * 10 + u16::from(digit - b'0'));
    u8::try_from(progress)
        .ok()
        .filter(|progress| *progress <= 100)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::ProgramLifecycle;

    fn b64(text: &str) -> String {
        base64::engine::general_purpose::STANDARD.encode(text)
    }

    fn records(reports: &[&str]) -> ProgramStatusRecords {
        let mut records = ProgramStatusRecords::default();
        for report in reports {
            records.apply_report(report.as_bytes());
        }
        records
    }

    fn ids(records: &ProgramStatusRecords) -> Vec<String> {
        records.snapshot().into_iter().map(|r| r.id).collect()
    }

    #[test]
    fn report_sets_the_root_record() {
        let report = format!(
            "state=blocked:kind=permission:progress=40:app=some-app:title={}:msg={}",
            b64("build"),
            b64("needs a yes")
        );
        let snapshot = records(&[&report]).snapshot();
        assert_eq!(
            snapshot,
            vec![ProgramStatusRecord {
                id: String::new(),
                state: ProgramStatusState::Blocked,
                kind: Some(ProgramStatusKind::Permission),
                progress: Some(40),
                app: Some("some-app".into()),
                title: Some("build".into()),
                msg: Some("needs a yes".into()),
            }]
        );
    }

    #[test]
    fn each_report_replaces_its_record() {
        let snapshot = records(&["state=working:progress=10:app=a", "state=idle"]).snapshot();
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].state, ProgramStatusState::Idle);
        assert_eq!(snapshot[0].progress, None);
        assert_eq!(snapshot[0].app, None);
    }

    #[test]
    fn missing_or_unknown_state_is_ignored() {
        assert!(ids(&records(&["app=a", "state=thinking", "state="])).is_empty());
        let kept = records(&["state=working", "state=paused"]).snapshot();
        assert_eq!(kept[0].state, ProgramStatusState::Working);
    }

    #[test]
    fn malformed_pairs_are_skipped_and_the_last_duplicate_wins() {
        let snapshot =
            records(&["junk: =x:Key=1:app=a b: state = working :state=done:future=1"]).snapshot();
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].state, ProgramStatusState::Done);
        assert_eq!(snapshot[0].app, None);
    }

    #[test]
    fn invalid_id_voids_the_report() {
        let long_segment = "s".repeat(33);
        let deep = ["a"; 9].join("/");
        let long = ["abcdefghijklmnopqrstuvwxyz"; 5].join("/");
        for id in ["", "a//b", "/a", "a/", "a*b", &long_segment, &deep, &long] {
            let report = format!("state=working:id={id}");
            assert!(ids(&records(&[&report])).is_empty(), "id {id:?}");
        }
        // An invalid id must not fall back to the root record.
        let kept = records(&["state=idle", "state=working:id=a//b"]).snapshot();
        assert_eq!(kept[0].state, ProgramStatusState::Idle);

        let deepest = ["a"; 8].join("/");
        let report = format!("state=working:id={deepest}");
        assert_eq!(ids(&records(&[&report])), vec![deepest]);
    }

    #[test]
    fn child_records_inherit_app_from_the_nearest_ancestor() {
        let snapshot = records(&[
            "state=working:app=root-app",
            "state=working:id=build:app=build-app",
            "state=working:id=build/test",
            "state=working:id=lint/fast",
        ])
        .snapshot();
        let app = |id: &str| {
            snapshot
                .iter()
                .find(|record| record.id == id)
                .and_then(|record| record.app.clone())
        };
        assert_eq!(app("build/test").as_deref(), Some("build-app"));
        assert_eq!(app("lint/fast").as_deref(), Some("root-app"));
    }

    #[test]
    fn clear_removes_the_record_and_its_children() {
        let mut records = records(&[
            "state=working",
            "state=working:id=build",
            "state=working:id=build/test",
            "state=working:id=build/test/unit",
            "state=working:id=builder",
        ]);
        records.apply_report(b"state=clear:id=build/test");
        assert_eq!(ids(&records), ["", "build", "builder"]);
        records.apply_report(b"state=clear:id=build");
        assert_eq!(ids(&records), ["", "builder"]);
        records.apply_report(b"state=clear");
        assert!(ids(&records).is_empty());
    }

    #[test]
    fn kind_and_progress_only_apply_to_their_states() {
        let snapshot = records(&[
            "state=working:kind=question:progress=100",
            "state=idle:id=a:progress=5",
            "state=blocked:id=b:kind=later:progress=101",
            "state=blocked:id=c:kind=auth:progress=+5",
        ])
        .snapshot();
        assert_eq!((snapshot[0].kind, snapshot[0].progress), (None, Some(100)));
        assert_eq!(snapshot[1].progress, None);
        assert_eq!((snapshot[2].kind, snapshot[2].progress), (None, None));
        assert_eq!(snapshot[3].kind, Some(ProgramStatusKind::Auth));
        assert_eq!(snapshot[3].progress, None);
    }

    #[test]
    fn text_limits_and_bad_text_void_the_report() {
        let title_over = b64(&"t".repeat(MAX_TITLE_DECODED_BYTES + 1));
        let msg_over = b64(&"m".repeat(MAX_MSG_DECODED_BYTES + 1));
        let control = b64("line\nbreak");
        let not_utf8 = base64::engine::general_purpose::STANDARD.encode([0xff, 0xfe]);
        for report in [
            format!("state=working:title={title_over}"),
            format!("state=working:msg={msg_over}"),
            format!("state=working:msg={control}"),
            format!("state=working:title={not_utf8}"),
            "state=working:msg=a".to_string(),
            format!("state=working:{}=1", "k".repeat(MAX_KEY_BYTES + 1)),
        ] {
            assert!(ids(&records(&[&report])).is_empty(), "report {report:?}");
        }

        let title_max = b64(&"t".repeat(MAX_TITLE_DECODED_BYTES));
        let msg_max = b64(&"m".repeat(MAX_MSG_DECODED_BYTES));
        let report = format!("state=working:title={title_max}:msg={msg_max}");
        let snapshot = records(&[&report]).snapshot();
        assert_eq!(
            snapshot[0].title.as_ref().map(String::len),
            Some(MAX_TITLE_DECODED_BYTES)
        );
        assert_eq!(
            snapshot[0].msg.as_ref().map(String::len),
            Some(MAX_MSG_DECODED_BYTES)
        );
    }

    #[test]
    fn base64_padding_is_optional() {
        let snapshot = records(&["state=working:title=YQ:msg=YQ=="]).snapshot();
        assert_eq!(snapshot[0].title.as_deref(), Some("a"));
        assert_eq!(snapshot[0].msg.as_deref(), Some("a"));
    }

    #[test]
    fn oversized_app_is_absent_not_fatal() {
        let report = format!("state=working:app={}", "a".repeat(MAX_APP_BYTES + 1));
        let snapshot = records(&[&report]).snapshot();
        assert_eq!(snapshot[0].app, None);
    }

    #[test]
    fn record_cap_evicts_the_least_recently_updated() {
        let mut records = ProgramStatusRecords::default();
        for index in 0..MAX_RECORDS {
            records.apply_report(format!("state=working:id=r{index}").as_bytes());
        }
        // Updating an existing record neither evicts nor grows the set.
        records.apply_report(b"state=idle:id=r0");
        assert_eq!(ids(&records).len(), MAX_RECORDS);

        records.apply_report(b"state=working:id=extra");
        let ids = ids(&records);
        assert_eq!(ids.len(), MAX_RECORDS);
        assert!(ids.contains(&"r0".to_string()));
        assert!(!ids.contains(&"r1".to_string()));
        assert!(ids.contains(&"extra".to_string()));
    }

    #[test]
    fn process_exit_keeps_only_done_and_error() {
        let mut records = records(&[
            "state=idle",
            "state=working:id=a",
            "state=blocked:id=b",
            "state=done:id=c",
            "state=error:id=d",
        ]);
        records.drop_live_states();
        assert_eq!(ids(&records), ["c", "d"]);
        records.clear_all();
        assert!(ids(&records).is_empty());
    }

    fn root(report: &str) -> Option<ProgramStatusReport> {
        records(&[report]).root_lifecycle()
    }

    #[test]
    fn root_lifecycle_maps_every_state_and_kind() {
        let lifecycle = |report: &str| root(report).map(|report| report.lifecycle);
        assert_eq!(lifecycle("state=working"), Some(ProgramLifecycle::Working));
        assert_eq!(lifecycle("state=done"), Some(ProgramLifecycle::Done));
        assert_eq!(lifecycle("state=idle"), Some(ProgramLifecycle::Idle));
        assert_eq!(lifecycle("state=error"), Some(ProgramLifecycle::Error));
        assert_eq!(
            lifecycle("state=blocked"),
            Some(ProgramLifecycle::Blocked(None))
        );
        for (kind, reason) in [
            ("permission", BlockedReason::Permission),
            ("question", BlockedReason::Question),
            ("auth", BlockedReason::Auth),
        ] {
            assert_eq!(
                lifecycle(&format!("state=blocked:kind={kind}")),
                Some(ProgramLifecycle::Blocked(Some(reason)))
            );
        }
        assert_eq!(
            root(&format!("state=error:msg={}", b64("rate limited"))),
            Some(ProgramStatusReport {
                lifecycle: ProgramLifecycle::Error,
                message: Some("rate limited".into()),
            })
        );
        // Only the root record speaks for the pane.
        assert_eq!(lifecycle("state=working:id=job"), None);
    }

    #[test]
    fn root_lifecycle_ends_with_a_clear_or_with_its_process() {
        let mut records = records(&["state=working"]);
        let reported = records.root_revision();
        records.apply_report(b"state=clear");
        assert_eq!(records.root_lifecycle(), None);
        assert!(records.root_revision() > reported);

        // A done record outlives its process as a record, not as the next program's state.
        records.apply_report(b"state=done");
        let reported = records.root_revision();
        records.drop_live_states();
        assert_eq!(ids(&records), [""]);
        assert_eq!(records.root_lifecycle(), None);
        assert!(records.root_revision() > reported);

        records.apply_report(b"state=done");
        assert!(records.root_lifecycle().is_some());
    }

    #[test]
    fn root_revision_moves_on_every_root_report_only() {
        let mut records = records(&["state=working"]);
        let reported = records.root_revision();
        records.apply_report(b"state=working:id=job");
        records.apply_report(b"state=nonsense");
        assert_eq!(records.root_revision(), reported);
        // A repeat still counts: the state it reaches may have been dropped meanwhile.
        records.apply_report(b"state=working");
        assert!(records.root_revision() > reported);
    }

    #[test]
    fn reporting_lasts_until_the_process_is_gone() {
        let mut records = ProgramStatusRecords::default();
        assert!(!records.reporting());
        records.apply_report(b"state=working:id=job");
        records.apply_report(b"state=clear:id=job");
        assert!(records.reporting());
        records.drop_live_states();
        assert!(!records.reporting());
    }
}
