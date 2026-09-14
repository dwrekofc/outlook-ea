use super::*;
use crate::db::test_store;

#[test]
fn test_cache_and_retrieve_body() {
    let conn = test_store().unwrap();
    let to = vec!["alice@test.com".to_string()];
    let cc = vec!["bob@test.com".to_string()];
    cache_body(&conn, 1, "msg@test", "Hello world", "plain", &to, &cc).unwrap();

    let cached = get_cached_body(&conn, 1, "msg@test").unwrap().unwrap();
    assert_eq!(cached.body_text, "Hello world");
    assert_eq!(cached.body_format, "plain");
    assert_eq!(cached.to, vec!["alice@test.com"]);
    assert_eq!(cached.cc, vec!["bob@test.com"]);
}

#[test]
fn test_cache_miss() {
    let conn = test_store().unwrap();
    let cached = get_cached_body(&conn, 999, "missing@test").unwrap();
    assert!(cached.is_none());
}

#[test]
fn test_cache_upsert() {
    let conn = test_store().unwrap();
    cache_body(&conn, 1, "msg@test", "Old body", "plain", &[], &[]).unwrap();
    cache_body(&conn, 1, "msg@test", "New body", "markdown", &[], &[]).unwrap();

    let cached = get_cached_body(&conn, 1, "msg@test").unwrap().unwrap();
    assert_eq!(cached.body_text, "New body");
    assert_eq!(cached.body_format, "markdown");
}

#[test]
fn test_parse_plain_email() {
    let raw =
        b"From: test@example.com\r\nSubject: Test\r\nContent-Type: text/plain\r\n\r\nHello, world!";
    let (body, format) = parse_email_body(raw).unwrap();
    assert_eq!(format, "plain");
    assert!(body.contains("Hello, world!"));
}

#[test]
fn test_parse_html_email() {
    let raw = b"From: test@example.com\r\nSubject: Test\r\nContent-Type: text/html\r\n\r\n<html><body><h1>Hello</h1><p>World</p></body></html>";
    let (body, format) = parse_email_body(raw).unwrap();
    assert_eq!(format, "markdown");
    assert!(body.contains("Hello"));
    assert!(body.contains("World"));
}

#[test]
fn test_parse_emlx() {
    let emlx = b"15\nFrom: a@b\r\nX: y\n<?xml version=\"1.0\"?><plist></plist>";
    let message = parse_emlx(emlx).unwrap();
    assert_eq!(message, b"From: a@b\r\nX: y");
}

#[test]
fn test_parse_emlx_invalid() {
    let result = parse_emlx(b"notanumber\nstuff");
    assert!(result.is_err());
}

#[test]
fn test_second_read_from_cache() {
    let conn = test_store().unwrap();
    // Pre-populate cache
    cache_body(&conn, 42, "msg42@test", "Cached content", "plain", &[], &[]).unwrap();

    // Verify it returns from cache
    let cached = get_cached_body(&conn, 42, "msg42@test").unwrap().unwrap();
    assert_eq!(cached.body_text, "Cached content");
}

#[test]
fn test_clean_html_text_removes_separators() {
    let input = "Hello\n────\n────\n────\n────\nWorld";
    let result = clean_html_text(input);
    assert_eq!(result, "Hello\nWorld");
}

#[test]
fn test_clean_html_text_removes_image_tracking() {
    let input = "Hello\n[Image]\nWorld\n[image]\nEnd\n[Image: logo]\nKeep";
    let result = clean_html_text(input);
    assert!(result.contains("Hello"));
    assert!(result.contains("World"));
    assert!(!result.contains("[Image]\n"));
    assert!(!result.contains("[image]\n"));
    assert!(result.contains("[Image: logo]"));
}

#[test]
fn test_clean_html_text_collapses_blank_lines() {
    let input = "A\n\n\n\n\nB";
    let result = clean_html_text(input);
    assert_eq!(result, "A\n\nB");
}

#[test]
fn test_clean_html_text_trims_trailing_whitespace() {
    let input = "Hello   \nWorld  ";
    let result = clean_html_text(input);
    assert_eq!(result, "Hello\nWorld");
}

#[test]
fn test_body_persists_across_connections() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.db");

    {
        let conn = crate::db::Store::connect(path.to_str().unwrap(), "").unwrap();
        conn.install_test_schema().unwrap();
        cache_body(
            &conn,
            1,
            "msg@persist",
            "Persistent body",
            "markdown",
            &[],
            &[],
        )
        .unwrap();
    }
    {
        let conn = crate::db::Store::connect(path.to_str().unwrap(), "").unwrap();
        conn.install_test_schema().unwrap();
        let cached = get_cached_body(&conn, 1, "msg@persist").unwrap().unwrap();
        assert_eq!(cached.body_text, "Persistent body");
    }
}
