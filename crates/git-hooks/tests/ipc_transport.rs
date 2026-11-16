//! Live IPC transport tests.
//! # Environment note (important)
//!
//! The round-trip tests are `#[ignore]`d because THIS HOST cannot open named pipes:
//! connecting to a freshly bound pipe fails with `拒绝访问 (os error 5)` -- the same
//! restriction that stops `sh.exe` from starting. The implementation is therefore
//! COMPILED BUT NOT VERIFIED HERE.
//!
//! What IS verified here is the degradation path, which is the property that matters
//! for correctness: when the channel is unavailable, delivery reports
//! `Unreachable` quickly and the Helper falls back to the Spool instead of blocking
//! or dropping the event.
//!
//! Run the ignored tests on an unrestricted machine with `cargo test -- --ignored`.
//!
//! # Why a self-contained round trip first
//!
//! If a bind-and-connect in one process fails, nothing above it can work, and no
//! amount of end-to-end debugging will say why. This file isolates the transport from
//! the Companion wiring.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use devchievements_git_hooks::ipc::{deliver_live, socket_name, Delivery, Server};

/// Unique per test so parallel tests never share an endpoint.
fn discriminator(name: &str) -> String {
    format!("test-{}-{name}", std::process::id())
}

#[test]
#[ignore = "host limitation: this sandbox denies named pipes (os error 5); run on an unrestricted machine"]
fn a_bound_server_accepts_and_acknowledges_a_record() {
    let user = discriminator("basic");
    let server = Server::bind(&user).expect("bind");

    let payload = r#"{"event_id":"evt-1","commit_sha":"abc"}"#.to_string();
    let expected = payload.clone();

    let handle = std::thread::spawn(move || {
        let mut received = None;
        let result = server.serve_one(|payload| {
            received = Some(payload.to_string());
            true
        });
        (result, received)
    });

    // Give the listener a moment; bind already completed, so this is generous.
    std::thread::sleep(std::time::Duration::from_millis(50));
    let delivery = deliver_live(&user, "evt-1", &payload);

    let (result, received) = handle.join().expect("server thread");
    assert!(result.is_ok(), "serving must succeed: {result:?}");
    assert_eq!(
        received.as_deref(),
        Some(expected.as_str()),
        "the payload must arrive intact"
    );
    assert_eq!(
        delivery,
        Delivery::Acknowledged,
        "an acked record must report Acknowledged"
    );
}

#[test]
fn delivery_reports_unreachable_when_nothing_is_listening() {
    // The normal "Companion is closed" path, which must be cheap and must not panic.
    let delivery = deliver_live(&discriminator("absent"), "evt-1", r#"{"event_id":"evt-1"}"#);
    assert_eq!(delivery, Delivery::Unreachable);
}

#[test]
#[ignore = "host limitation: this sandbox denies named pipes (os error 5); run on an unrestricted machine"]
fn a_refused_record_is_not_acknowledged() {
    // If the server cannot make the record durable it must stay silent, so the Helper
    // falls back to the Spool instead of dropping the event.
    let user = discriminator("refuse");
    let server = Server::bind(&user).expect("bind");

    let handle = std::thread::spawn(move || server.serve_one(|_| false));
    std::thread::sleep(std::time::Duration::from_millis(50));

    let delivery = deliver_live(&user, "evt-1", r#"{"event_id":"evt-1"}"#);
    handle.join().expect("server thread").expect("serve");

    assert_eq!(
        delivery,
        Delivery::Unreachable,
        "no ack means the caller must retry via spool"
    );
}

#[test]
fn an_unavailable_channel_never_blocks_the_hook() {
    // The property that actually protects the user: whatever the reason the channel
    // is unusable, delivery gives up quickly instead of stalling `git commit`.
    let start = std::time::Instant::now();
    let delivery = deliver_live(
        &discriminator("fast-fail"),
        "evt-1",
        r#"{"event_id":"evt-1"}"#,
    );
    let elapsed = start.elapsed();

    assert_eq!(delivery, Delivery::Unreachable);
    assert!(
        elapsed < std::time::Duration::from_millis(500),
        "delivery must fail fast, took {elapsed:?}"
    );
}

#[test]
fn the_socket_name_is_stable_and_user_scoped() {
    assert_eq!(socket_name("alice"), socket_name("alice"));
    assert_ne!(socket_name("alice"), socket_name("bob"));
    assert!(socket_name("alice").contains("alice"));
}
