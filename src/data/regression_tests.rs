use super::thread::{is_automated_sender, is_sent_url};
use super::*;
use crate::db::test_store;

/// Create a mock Envelope Index matching Apple Mail V10's normalized schema.
fn mock_envelope(n: usize) -> Store {
    let conn = Store::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE mailboxes (ROWID INTEGER PRIMARY KEY, url TEXT COLLATE BINARY);
         CREATE TABLE subjects (ROWID INTEGER PRIMARY KEY, subject TEXT);
         CREATE TABLE addresses (ROWID INTEGER PRIMARY KEY, address TEXT, comment TEXT);
         CREATE TABLE message_global_data (ROWID INTEGER PRIMARY KEY, message_id INTEGER, message_id_header TEXT);
         CREATE TABLE messages (
            ROWID INTEGER PRIMARY KEY,
            message_id INTEGER DEFAULT 0,
            global_message_id INTEGER,
            subject_prefix TEXT,
            sender INTEGER,
            subject INTEGER,
            date_sent INTEGER,
            read INTEGER DEFAULT 0,
            flagged INTEGER DEFAULT 0,
            deleted INTEGER DEFAULT 0,
            mailbox INTEGER,
            conversation_id INTEGER DEFAULT 0
         );
         INSERT INTO mailboxes VALUES (1, 'ews://test-uuid/Inbox');",
    )
    .unwrap();
    for i in 1..=n {
        // Insert address
        conn.execute(
            "INSERT INTO addresses VALUES (?1, ?2, ?3)",
            libsql::params![i as i64, format!("user{i}@test.com"), format!("User {i}")],
        )
        .unwrap();
        // Insert subject
        conn.execute(
            "INSERT INTO subjects VALUES (?1, ?2)",
            libsql::params![i as i64, format!("Subject {i}")],
        )
        .unwrap();
        // Insert message_global_data
        conn.execute(
            "INSERT INTO message_global_data VALUES (?1, ?2, ?3)",
            libsql::params![i as i64, i as i64, format!("msg{i}@test")],
        )
        .unwrap();
        // Insert message
        conn.execute(
            "INSERT INTO messages (ROWID, message_id, global_message_id, subject_prefix, sender, subject, date_sent, read, flagged, deleted, mailbox)
             VALUES (?1, 0, ?2, '', ?3, ?4, ?5, 0, 0, 0, 1)",
            libsql::params![
                i as i64,
                i as i64,
                i as i64,
                i as i64,
                (i as i64) * 100,
            ],
        )
        .unwrap();
    }
    conn
}

#[test]
fn test_label_filter_finds_emails_beyond_first_page() {
    let envelope = mock_envelope(10);
    let overlay = test_store().unwrap();

    labels::assign_label(&overlay, 3, "msg3@test", 1).unwrap();

    let result =
        list_emails_filtered(&envelope, &overlay, None, 0, 5, Some(1), false, false).unwrap();
    assert_eq!(result.total_count, 1);
    assert_eq!(result.emails.len(), 1);
    assert_eq!(result.emails[0].id, 3);
}

#[test]
fn test_untriaged_filter_correct_pagination() {
    let envelope = mock_envelope(5);
    let overlay = test_store().unwrap();

    labels::assign_label(&overlay, 5, "msg5@test", 1).unwrap();
    labels::assign_label(&overlay, 3, "msg3@test", 2).unwrap();

    let page0 = list_emails_filtered(&envelope, &overlay, None, 0, 2, None, true, false).unwrap();
    assert_eq!(page0.total_count, 3);
    assert_eq!(page0.emails.len(), 2);

    let page1 = list_emails_filtered(&envelope, &overlay, None, 1, 2, None, true, false).unwrap();
    assert_eq!(page1.total_count, 3);
    assert_eq!(page1.emails.len(), 1);
}

#[test]
fn test_no_filter_uses_sql_pagination() {
    let envelope = mock_envelope(5);
    let overlay = test_store().unwrap();

    let result = list_emails_filtered(&envelope, &overlay, None, 0, 3, None, false, false).unwrap();
    assert_eq!(result.total_count, 5);
    assert_eq!(result.emails.len(), 3);
}

#[test]
fn test_is_sent_url() {
    assert!(is_sent_url("ews://uuid/Sent%20Items"));
    assert!(is_sent_url("ews://uuid/Deleted%20Items/Sent%20Items"));
    assert!(is_sent_url("imap://acct/Sent Messages"));
    assert!(!is_sent_url("ews://uuid/Inbox"));
    assert!(!is_sent_url("ews://uuid/Archive"));
}

#[test]
fn test_is_automated_sender() {
    assert!(is_automated_sender("no-reply@sharepointonline.com"));
    assert!(is_automated_sender("donotreply_signup@sap.com"));
    assert!(is_automated_sender("do_not_reply_learning@sap.com"));
    assert!(is_automated_sender("notification@emoneyadvisor.com"));
    assert!(is_automated_sender("mailer@workato.com"));
    assert!(is_automated_sender("someone@equateplus.com"));
    // Real people are not automated.
    assert!(!is_automated_sender("jason.cook@sap.com"));
    assert!(!is_automated_sender("d.skinnell@sap.com"));
    assert!(!is_automated_sender("benjamin.smokovich@sap.com"));
}

#[test]
fn test_parse_sender_with_name() {
    let (name, addr) = parse_sender("John Doe <john@example.com>");
    assert_eq!(name, "John Doe");
    assert_eq!(addr, "john@example.com");
}

#[test]
fn test_parse_sender_quoted_name() {
    let (name, addr) = parse_sender("\"Jane Doe\" <jane@example.com>");
    assert_eq!(name, "Jane Doe");
    assert_eq!(addr, "jane@example.com");
}

#[test]
fn test_parse_sender_bare_address() {
    let (name, addr) = parse_sender("user@example.com");
    assert_eq!(name, "");
    assert_eq!(addr, "user@example.com");
}

#[test]
fn test_unix_to_iso8601() {
    // 2024-01-01 00:00:00 UTC = Unix 1704067200
    let result = unix_to_iso8601(1_704_067_200);
    assert!(result.starts_with("2024-01-01T00:00:00"));
}

#[test]
fn test_unix_to_iso8601_zero() {
    let result = unix_to_iso8601(0);
    assert!(result.starts_with("1970-01-01T00:00:00"));
}

#[test]
fn test_list_emails_on_mock_db() {
    let conn = mock_envelope(2);

    let result = list_emails(&conn, None, 0, 10).unwrap();
    assert_eq!(result.total_count, 2);
    assert_eq!(result.emails.len(), 2);
    // Sorted by date desc — email 2 has date_sent=200, email 1 has date_sent=100
    assert_eq!(result.emails[0].sender_name, "User 2");
    assert_eq!(result.emails[1].sender_name, "User 1");
}

#[test]
fn test_list_emails_pagination() {
    let conn = mock_envelope(3);

    let page0 = list_emails(&conn, None, 0, 2).unwrap();
    assert_eq!(page0.total_count, 3);
    assert_eq!(page0.emails.len(), 2);
    assert_eq!(page0.emails[0].subject, "Subject 3");

    let page1 = list_emails(&conn, None, 1, 2).unwrap();
    assert_eq!(page1.emails.len(), 1);
    assert_eq!(page1.emails[0].subject, "Subject 1");
}

#[test]
fn test_list_emails_folder_filter() {
    let conn = Store::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE mailboxes (ROWID INTEGER PRIMARY KEY, url TEXT COLLATE BINARY);
         CREATE TABLE subjects (ROWID INTEGER PRIMARY KEY, subject TEXT);
         CREATE TABLE addresses (ROWID INTEGER PRIMARY KEY, address TEXT, comment TEXT);
         CREATE TABLE message_global_data (ROWID INTEGER PRIMARY KEY, message_id INTEGER, message_id_header TEXT);
         CREATE TABLE messages (
            ROWID INTEGER PRIMARY KEY, message_id INTEGER DEFAULT 0, global_message_id INTEGER,
            subject_prefix TEXT, sender INTEGER, subject INTEGER,
            date_sent INTEGER, read INTEGER DEFAULT 0, flagged INTEGER DEFAULT 0,
            deleted INTEGER DEFAULT 0, mailbox INTEGER, conversation_id INTEGER DEFAULT 0
         );
         INSERT INTO mailboxes VALUES (1, 'ews://test-uuid/Inbox');
         INSERT INTO mailboxes VALUES (2, 'ews://test-uuid/Sent');
         INSERT INTO subjects VALUES (1, 'Inbox msg');
         INSERT INTO subjects VALUES (2, 'Sent msg');
         INSERT INTO addresses VALUES (1, 'a@t', 'A');
         INSERT INTO addresses VALUES (2, 'b@t', 'B');
         INSERT INTO message_global_data VALUES (1, 1, 'a@test');
         INSERT INTO message_global_data VALUES (2, 2, 'b@test');
         INSERT INTO messages VALUES (1, 0, 1, '', 1, 1, 100, 0, 0, 0, 1, 0);
         INSERT INTO messages VALUES (2, 0, 2, '', 2, 2, 200, 0, 0, 0, 2, 0);",
    )
    .unwrap();

    let inbox = list_emails(&conn, None, 0, 10).unwrap();
    assert_eq!(inbox.total_count, 1);
    assert_eq!(inbox.emails[0].subject, "Inbox msg");

    let sent = list_emails(&conn, Some("Sent"), 0, 10).unwrap();
    assert_eq!(sent.total_count, 1);
    assert_eq!(sent.emails[0].subject, "Sent msg");
}

#[test]
fn test_list_emails_message_id_from_global_data() {
    let conn = mock_envelope(1);
    let result = list_emails(&conn, None, 0, 10).unwrap();
    assert_eq!(result.emails[0].message_id, "msg1@test");
}

#[test]
fn test_list_emails_subject_prefix() {
    let conn = Store::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE mailboxes (ROWID INTEGER PRIMARY KEY, url TEXT COLLATE BINARY);
         CREATE TABLE subjects (ROWID INTEGER PRIMARY KEY, subject TEXT);
         CREATE TABLE addresses (ROWID INTEGER PRIMARY KEY, address TEXT, comment TEXT);
         CREATE TABLE message_global_data (ROWID INTEGER PRIMARY KEY, message_id INTEGER, message_id_header TEXT);
         CREATE TABLE messages (
            ROWID INTEGER PRIMARY KEY, message_id INTEGER DEFAULT 0, global_message_id INTEGER,
            subject_prefix TEXT, sender INTEGER, subject INTEGER,
            date_sent INTEGER, read INTEGER DEFAULT 0, flagged INTEGER DEFAULT 0,
            deleted INTEGER DEFAULT 0, mailbox INTEGER, conversation_id INTEGER DEFAULT 0
         );
         INSERT INTO mailboxes VALUES (1, 'ews://test-uuid/Inbox');
         INSERT INTO subjects VALUES (1, 'Hello');
         INSERT INTO addresses VALUES (1, 'a@t', 'A');
         INSERT INTO message_global_data VALUES (1, 1, 'a@test');
         INSERT INTO messages VALUES (1, 0, 1, 'Re: ', 1, 1, 100, 0, 0, 0, 1, 0);",
    )
    .unwrap();

    let result = list_emails(&conn, None, 0, 10).unwrap();
    assert_eq!(result.emails[0].subject, "Re: Hello");
}
