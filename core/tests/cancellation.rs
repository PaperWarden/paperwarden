//! Cancellation is how the UI stops a running job. A cancelled job still ends
//! cleaned (`PRODUCT_INVARIANTS.md` 9), so the token only signals; the job's
//! owner performs the cleanup.

use std::thread;

use paperwarden_core::CancellationToken;

#[test]
fn a_new_token_is_not_cancelled() {
    assert!(!CancellationToken::new().is_cancelled());
}

#[test]
fn cancelling_is_seen_by_every_clone() {
    let token = CancellationToken::new();
    let observer = token.clone();

    token.cancel();

    assert!(observer.is_cancelled());
    assert!(token.is_cancelled());
}

#[test]
fn cancelling_twice_is_harmless() {
    let token = CancellationToken::new();
    token.cancel();
    token.cancel();

    assert!(token.is_cancelled());
}

#[test]
fn check_reports_cancellation_as_an_error() {
    let token = CancellationToken::new();
    assert_eq!(token.check(), Ok(()));

    token.cancel();

    assert_eq!(token.check(), Err(paperwarden_core::Cancelled));
}

#[test]
fn cancellation_crosses_threads() {
    let token = CancellationToken::new();
    let worker = {
        let token = token.clone();
        thread::spawn(move || {
            while !token.is_cancelled() {
                thread::yield_now();
            }
        })
    };

    token.cancel();

    worker.join().expect("worker stops once cancelled");
}
