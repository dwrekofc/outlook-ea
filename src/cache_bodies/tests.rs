use super::*;
use libsql::params;

fn args(limit: usize, dry_run: bool) -> Args {
    Args {
        since: None,
        all: false,
        limit,
        dry_run,
        json: true,
        action: None,
    }
}
fn messages() -> Vec<Message> {
    vec![
        Message {
            id: 1,
            message_id: "older".into(),
            sent: 1,
        },
        Message {
            id: 2,
            message_id: "newest".into(),
            sent: 3,
        },
        Message {
            id: 3,
            message_id: "middle".into(),
            sent: 2,
        },
    ]
}
fn body() -> body::CachedBody {
    body::CachedBody {
        body_text: "content".into(),
        body_format: "plain".into(),
        to: vec!["a@test".into()],
        cc: vec!["b@test".into()],
    }
}
fn store() -> Store {
    let store = crate::db::test_store().unwrap();
    // Exact account column from vault migration 009; no MEA production migration.
    store
        .execute_batch(
            "ALTER TABLE mail_identities ADD COLUMN account TEXT NOT NULL DEFAULT 'work-outlook';",
        )
        .unwrap();
    store
}
#[test]
fn idempotent_and_preserves_account_and_headers() {
    let store = store();
    store
        .execute(
            "INSERT INTO mail_identities(message_id, account) VALUES ('newest', 'another-account')",
            (),
        )
        .unwrap();
    for expected in [3, 0] {
        let mut report = Report::default();
        capture(
            &mut messages(),
            &mut source::existing(&store).unwrap(),
            &args(200, false),
            &mut report,
            |_| Ok(body()),
            |m, b| store.insert_body_once(&m.message_id, b),
        );
        assert_eq!(report.cached, expected);
        assert_eq!(report.failed, 0);
    }
    assert!(
        !store
            .insert_body_once(
                "newest",
                &body::CachedBody {
                    body_text: "replacement".into(),
                    ..body()
                }
            )
            .unwrap()
    );
    let cached = body::get_cached_body(&store, 2, "newest").unwrap().unwrap();
    assert_eq!(cached.body_text, "content");
    assert_eq!(cached.to, vec!["a@test"]);
    assert_eq!(cached.cc, vec!["b@test"]);
    let account = store
        .one(
            "SELECT account FROM mail_identities WHERE message_id='newest'",
            (),
            |r| Ok(r.get::<String>(0)?),
        )
        .unwrap()
        .unwrap();
    assert_eq!(account, "another-account");
    let account = store
        .one(
            "SELECT account FROM mail_identities WHERE message_id='older'",
            (),
            |r| Ok(r.get::<String>(0)?),
        )
        .unwrap()
        .unwrap();
    assert_eq!(account, "work-outlook");
}
#[test]
fn limit_and_newest_first_skip_existing_without_using_limit() {
    let mut report = Report::default();
    let mut seen = vec![];
    capture(
        &mut messages(),
        &mut HashSet::from(["newest".into()]),
        &args(1, false),
        &mut report,
        |m| {
            seen.push(m.message_id.clone());
            Ok(body())
        },
        |_, _| Ok(true),
    );
    assert_eq!(seen, ["middle"]);
    assert_eq!(
        (report.cached, report.scanned, report.skipped_existing),
        (1, 2, 1)
    );
}
#[test]
fn preview_does_not_extract_or_write_and_zero_limit_is_zero() {
    for limit in [0, 2] {
        let mut report = Report::default();
        capture(
            &mut messages(),
            &mut HashSet::new(),
            &args(limit, true),
            &mut report,
            |_| panic!("preview extracted"),
            |_, _| panic!("preview wrote"),
        );
        assert_eq!(report.would_cache, limit);
        assert_eq!(report.cached, 0);
    }
}
#[test]
fn busy_stops_with_counts_and_success_envelope() {
    let mut report = Report::default();
    capture(
        &mut messages(),
        &mut HashSet::new(),
        &args(200, false),
        &mut report,
        |_| Ok(body()),
        |_, _| anyhow::bail!("replica_busy: fixture"),
    );
    assert_eq!((report.scanned, report.cached, report.failed), (1, 0, 1));
    assert_eq!(report.failures[0].message_id, "newest");
    assert!(report.stopped.is_some());
    let output: serde_json::Value = serde_json::from_str(&cli::success(report)).unwrap();
    assert_eq!(output["status"], "ok");
}
#[test]
fn envelope_filters_and_duplicate_message_ids() {
    let envelope = Store::open_in_memory().unwrap();
    envelope
        .execute_batch(
            "CREATE TABLE messages(mailbox INTEGER, global_message_id INTEGER, date_sent INTEGER);
        CREATE TABLE mailboxes(url TEXT); CREATE TABLE message_global_data(message_id_header TEXT);
        INSERT INTO mailboxes VALUES ('ews://test/Inbox'), ('ews://test/Sent');
        INSERT INTO message_global_data VALUES ('same'), ('other');",
        )
        .unwrap();
    for (mailbox, global, date) in [(1, 1, 100), (1, 1, 200), (2, 2, 300)] {
        envelope
            .execute(
                "INSERT INTO messages VALUES (?1, ?2, ?3)",
                params![mailbox, global, date],
            )
            .unwrap();
    }
    let mut options = args(200, true);
    let mut items = source::messages(&envelope, &options).unwrap();
    assert_eq!(items.len(), 2);
    let mut report = Report::default();
    capture(
        &mut items,
        &mut HashSet::new(),
        &options,
        &mut report,
        |_| unreachable!(),
        |_, _| unreachable!(),
    );
    assert_eq!((report.would_cache, report.skipped_existing), (1, 1));
    options.all = true;
    assert_eq!(source::messages(&envelope, &options).unwrap().len(), 3);
    options.since = Some(chrono::NaiveDate::from_ymd_opt(1970, 1, 2).unwrap());
    assert!(source::messages(&envelope, &options).unwrap().is_empty());
}
#[test]
fn timer_uses_capture_only_command_and_escapes_paths() {
    let xml = timer::plist("/a&b/mea", 900, "/Users/test");
    assert!(xml.contains("/a&amp;b/mea"));
    assert!(xml.contains("<string>cache-bodies</string><string>--limit</string><string>200</string><string>--json</string>"));
    assert!(xml.contains(".config/vault/logs/mea-cache-bodies.log"));
    assert_eq!(timer::seconds("15m").unwrap(), 900);
    assert!(timer::seconds("0").is_err());
}
