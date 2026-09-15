use super::*;
use std::os::unix::fs::PermissionsExt;
fn root() -> PathBuf {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "replica-test-{}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&path).unwrap();
    path
}
#[test]
fn throttle_boundaries() {
    assert!(due(None, 100, false));
    assert!(due(Some(0), 10, false));
    assert!(!due(Some(100), 129, false));
    assert!(due(Some(100), 130, false));
    assert!(due(Some(100), 101, true));
    assert!(due(Some(100), 99, false));
}
#[test]
fn shared_connections_block_sync_and_reset_and_keep_their_locks() {
    let root = root();
    let path = root.join("vault.db");
    let first = guard::ReplicaLock::open(&path).unwrap();
    let second = guard::ReplicaLock::open(&path).unwrap();
    assert!(first.exclusive().is_err());
    assert!(second.exclusive().is_err()); // failed upgrade retained protection
    assert!(guard::reset(&path).is_err());
    drop(second);
    // Other parallel tests spawn processes. Their pre-exec copies of CLOEXEC
    // descriptors may briefly retain a shared lock after our File is dropped.
    let deadline = std::time::Instant::now() + Duration::from_secs(1);
    let exclusive = loop {
        match first.exclusive() {
            Ok(guard) => break guard,
            Err(_) if std::time::Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(2))
            }
            Err(error) => panic!("shared lock did not release: {error:#}"),
        }
    };
    assert!(guard::ReplicaLock::open(&path).is_err());
    drop(exclusive);
    drop(first);
    let deadline = std::time::Instant::now() + Duration::from_secs(1);
    loop {
        match guard::reset(&path) {
            Ok(()) => break,
            Err(_) if std::time::Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(2))
            }
            Err(error) => panic!("reset remained busy after close: {error:#}"),
        }
    }
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn primary_binding_and_permissions() {
    let root = root();
    let path = root.join("vault.db");
    let _lock = guard::ReplicaLock::open(&path).unwrap();
    assert!(state::validate(&path, "libsql://one").is_ok());
    state::save(
        &path,
        &SyncState {
            primary_url: "libsql://one".into(),
            ..SyncState::default()
        },
    )
    .unwrap();
    assert!(state::validate(&path, "libsql://one").is_ok());
    assert!(
        state::validate(&path, "libsql://two")
            .unwrap_err()
            .to_string()
            .contains("replica_primary_mismatch")
    );
    assert_eq!(
        std::fs::metadata(&root).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        std::fs::metadata(path.with_extension("sync.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn validation_is_not_a_primary_write_error() {
    let error = errors::write_error(anyhow::anyhow!("--name cannot be empty"));
    assert!(error.downcast_ref::<ReplicaError>().is_none());
    let error = errors::write_error(libsql::Error::ConnectionFailed("offline".into()).into());
    assert!(error.downcast_ref::<ReplicaError>().is_some());
}
#[test]
fn auth_and_dispatch_notices_differ() {
    assert!(notice(&anyhow::anyhow!("JWT InvalidToken"), None).contains("sync failed"));
    assert!(!notice(&anyhow::anyhow!("JWT InvalidToken"), None).contains("offline"));
    assert!(
        notice(
            &anyhow::anyhow!("http dispatch error: connection refused"),
            None
        )
        .ends_with("offline")
    );
}
#[tokio::test]
async fn write_deadline_preserves_timeout_context() {
    let error = errors::deadline_probe().await.unwrap_err();
    assert!(error.to_string().contains("check primary before retrying"));
}
