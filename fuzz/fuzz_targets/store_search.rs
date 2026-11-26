//! Search queries and indexed text come from users and documents. Any input
//! must be treated as literal text: searching never fails, any text can be
//! indexed and removed, and existing entries stay findable.

#![no_main]

use libfuzzer_sys::fuzz_target;
use paperwarden_store::Store;
use std::sync::{Mutex, OnceLock};

fn store() -> &'static Mutex<Store> {
    static STORE: OnceLock<Mutex<Store>> = OnceLock::new();
    STORE.get_or_init(|| {
        let mut store = Store::open_in_memory().expect("open");
        store.enable_index().expect("enable index");
        store
            .index_document("known", "Known", "quarterly invoice résumé")
            .expect("index known document");
        Mutex::new(store)
    })
}

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    let mut store = store().lock().expect("lock");

    store.search(&text, 10).expect("any query is accepted");
    store
        .index_document("fuzz", "Fuzz", &text)
        .expect("any text can be indexed");
    store
        .search(&text, 10)
        .expect("any query is accepted after indexing");
    store.remove_document("fuzz").expect("remove");

    let known = store.search("invoice", 10).expect("search");
    assert_eq!(known.len(), 1, "an unrelated document disappeared");
});
