use super::*;

/// Create a mock Envelope Index matching V10 normalized schema.
fn mock_envelope_db() -> Store {
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
            deleted INTEGER DEFAULT 0, mailbox INTEGER
         );
         INSERT INTO mailboxes VALUES (1, 'ews://test-uuid/Inbox');
         INSERT INTO subjects VALUES (1, 'Project Update');
         INSERT INTO subjects VALUES (2, 'Meeting Tomorrow');
         INSERT INTO subjects VALUES (3, 'Invoice #123');
         INSERT INTO addresses VALUES (1, 'alice@example.com', 'Alice Smith');
         INSERT INTO addresses VALUES (2, 'bob@example.com', 'Bob Jones');
         INSERT INTO message_global_data VALUES (1, 1, 'msg1@test');
         INSERT INTO message_global_data VALUES (2, 2, 'msg2@test');
         INSERT INTO message_global_data VALUES (3, 3, 'msg3@test');
         INSERT INTO messages VALUES (1, 0, 1, '', 1, 1, 1704067200, 0, 0, 0, 1);
         INSERT INTO messages VALUES (2, 0, 2, '', 2, 2, 1704067300, 1, 0, 0, 1);
         INSERT INTO messages VALUES (3, 0, 3, '', 1, 3, 1704067400, 0, 0, 0, 1);",
    ).unwrap();
    conn
}

#[test]
fn test_search_by_sender() {
    let conn = mock_envelope_db();
    let query = SearchQuery {
        sender: Some("alice".to_string()),
        ..Default::default()
    };
    let results = search_metadata(&conn, &query).unwrap();
    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|e| e.sender_address.contains("alice")));
}

#[test]
fn test_search_by_sender_name() {
    let conn = mock_envelope_db();
    let query = SearchQuery {
        sender: Some("Bob".to_string()),
        ..Default::default()
    };
    let results = search_metadata(&conn, &query).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].sender_name, "Bob Jones");
}

#[test]
fn test_search_by_subject() {
    let conn = mock_envelope_db();
    let query = SearchQuery {
        subject: Some("Meeting".to_string()),
        ..Default::default()
    };
    let results = search_metadata(&conn, &query).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].subject, "Meeting Tomorrow");
}

#[test]
fn test_search_by_date_range() {
    let conn = mock_envelope_db();
    // 1704067300 = 2024-01-01T00:01:40Z
    let query = SearchQuery {
        date_from: Some("2024-01-01T00:01:00+00:00".to_string()),
        ..Default::default()
    };
    let results = search_metadata(&conn, &query).unwrap();
    // Should get msg2 (1704067300) and msg3 (1704067400)
    assert_eq!(results.len(), 2);
}

#[test]
fn test_search_combined_filters() {
    let conn = mock_envelope_db();
    let query = SearchQuery {
        sender: Some("alice".to_string()),
        subject: Some("Invoice".to_string()),
        ..Default::default()
    };
    let results = search_metadata(&conn, &query).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].subject, "Invoice #123");
}

#[test]
fn test_search_no_results() {
    let conn = mock_envelope_db();
    let query = SearchQuery {
        sender: Some("nonexistent@nowhere.com".to_string()),
        ..Default::default()
    };
    let results = search_metadata(&conn, &query).unwrap();
    assert!(results.is_empty());
}

#[test]
fn test_rowid_from_emlx_path() {
    assert_eq!(
        rowid_from_emlx_path("/Users/me/Library/Mail/V10/blah/12345.emlx"),
        Some(12345)
    );
    assert_eq!(
        rowid_from_emlx_path("/path/to/67890.partial.emlx"),
        Some(67890)
    );
    assert_eq!(rowid_from_emlx_path("/path/to/notanumber.emlx"), None);
}

#[test]
fn test_iso8601_to_unix() {
    let unix = iso8601_to_unix("2024-01-01T00:00:00+00:00").unwrap();
    assert_eq!(unix, 1704067200);
}

#[test]
fn test_search_results_shape_matches_list() {
    let conn = mock_envelope_db();
    let query = SearchQuery::default();
    let results = search_metadata(&conn, &query).unwrap();
    for email in &results {
        assert!(email.id > 0);
        assert!(!email.date.is_empty());
    }
}
