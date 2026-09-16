//! US2 scenarios 2–3 at the Rust level — the FFI's clock feeds the pipeline's budget (FR-007;
//! SC-002 partial). Model-backed.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use xtriever_ffi::{IndexHandle, LoadPath, SearchOptions, XtrieverError};

fn open(dir: &std::path::Path) -> std::sync::Arc<IndexHandle> {
    IndexHandle::open(
        dir.to_string_lossy().into_owned(),
        support::embedder_dir().to_string_lossy().into_owned(),
        Some(support::reranker_dir().to_string_lossy().into_owned()),
        LoadPath::Buffered,
    )
    .unwrap()
}

fn budgeted(ms: u64, strict: bool) -> SearchOptions {
    SearchOptions {
        k: 20,
        depth: None,
        rerank_depth: Some(20),
        rerank_mode: None,
        max_time_ms: Some(ms),
        max_items: None,
        strict,
        explain: true,
    }
}

/// No time budget at all — the only "generous" budget that is not a number about the machine.
fn unbudgeted() -> SearchOptions {
    SearchOptions {
        max_time_ms: None,
        ..budgeted(0, false)
    }
}

/// The budget contract, not the machine (Feature 017, spec FR-003). Until 017 this test asserted
/// `elapsed_ms < 1000` under a fixed 200 ms budget, which measured the laptop: on a slow day the
/// first query took 3.4 s uncontended and the test failed on `main` (015 report F-003) — and on
/// that machine a fixed 200 ms never even reaches the re-ranker (every stage report says
/// `skipped`), while a fast machine finishes everything inside it. So the budget is derived from
/// each query's own measured cost — endpoints *verified* by probing (a budget that scores
/// nothing, one that re-ranks everything), then bisected, re-measured at every step. What is then asserted is only the
/// contract: no error in degrading mode, the time-limit flag not set (the FFI attaches a clock),
/// the scored hits first, at least one *partial* re-rank across the queries, and — the negative —
/// with no budget everything is re-ranked and nothing skipped. No assertion mentions wall-clock time.
#[test]
#[ignore = "needs both models"]
fn a_short_time_budget_yields_a_partial_rerank_without_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    drop(support::build_fixture_index(tmp.path()));
    let ffi = open(tmp.path());
    let queries = support::fixture_docs().queries;

    // Warm the models once (the first call pays the cold-start), then measure per query.
    let _ = ffi.search(queries[0].text.clone(), unbudgeted()).unwrap();
    let mut costs = Vec::new();
    for q in &queries {
        let pre = ffi
            .search(
                q.text.clone(),
                SearchOptions {
                    rerank_depth: Some(0),
                    ..unbudgeted()
                },
            )
            .unwrap()
            .elapsed_ms;
        let full = ffi.search(q.text.clone(), unbudgeted()).unwrap();
        let rr = full.stages.rerank.as_ref().expect("re-rank ran");
        // The negative half of the contract: without a budget every candidate is re-ranked and
        // nothing is skipped (a numeric "generous" 60 s budget was exhausted once under 22
        // parallel model-backed tests on a slow laptop — a number about the machine again).
        assert!(rr.skipped.is_none(), "{}: {:?}", q.id, rr.skipped);
        assert_eq!(rr.scored, rr.candidates, "{}: fully re-ranked", q.id);
        assert!(rr.scored > 0, "{}: the fixture query has candidates", q.id);
        costs.push((q, pre, full.elapsed_ms.saturating_sub(pre)));
    }

    /// What one budgeted probe did: the stage never scored (skipped, or nothing scored yet),
    /// scored some of the head, or scored all of it.
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    enum Probe {
        TooSmall,
        Partial,
        Full,
    }
    let probe = |q: &support::FixtureQuery, ms: u64| -> Probe {
        let r = ffi
            .search(q.text.clone(), budgeted(ms.max(1), false))
            .unwrap();
        assert!(
            !r.stages.time_limit_ignored,
            "{}: a clock is attached, the limit is never ignored",
            q.id
        );
        let scored = r
            .hits
            .iter()
            .take_while(|h| h.rerank_score.is_some())
            .count();
        match &r.stages.rerank {
            None => {
                assert_eq!(scored, 0, "{}: no re-rank report, no scored hits", q.id);
                Probe::TooSmall
            }
            Some(rr) => {
                assert_eq!(
                    scored as u32, rr.scored,
                    "{}: scored hits form the prefix",
                    q.id
                );
                if rr.skipped.is_some() || rr.scored == 0 {
                    Probe::TooSmall
                } else if rr.scored < rr.candidates {
                    Probe::Partial
                } else {
                    Probe::Full
                }
            }
        }
    };

    // A budget that lands inside the re-rank stage on *this* machine, *now*. The elapsed times
    // above are a starting guess, not a bracket: under a rising load the unbudgeted full time is
    // no longer a budget that completes the call. So first *verify* the endpoints by probing —
    // doubling the upper budget until a probe re-ranks everything, halving the lower until one
    // scores nothing — and only then bisect between verified endpoints, re-measuring at every
    // step. A probe that already lands on a partial result ends the search.
    let mut partial = 0;
    let mut tried = Vec::new();
    'queries: for (q, pre, rerank) in &costs {
        let (mut lo, mut hi) = (*pre, pre + rerank.max(&1));
        // Upper endpoint: a budget verified to re-rank everything (or a partial on the way).
        let mut upper_ok = false;
        for _ in 0..8 {
            tried.push(hi);
            match probe(q, hi) {
                Probe::Full => {
                    upper_ok = true;
                    break;
                }
                Probe::Partial => {
                    partial += 1;
                    break 'queries;
                }
                Probe::TooSmall => {
                    lo = hi;
                    hi *= 2;
                }
            }
        }
        if !upper_ok {
            continue; // this query never completed under any probed budget; try the next
        }
        // Lower endpoint: a budget verified to score nothing (or a partial on the way).
        let mut lower_ok = false;
        for _ in 0..8 {
            tried.push(lo);
            match probe(q, lo) {
                Probe::TooSmall => {
                    lower_ok = true;
                    break;
                }
                Probe::Partial => {
                    partial += 1;
                    break 'queries;
                }
                Probe::Full => {
                    hi = lo;
                    lo /= 2;
                    if lo == 0 {
                        break;
                    }
                }
            }
        }
        if !lower_ok {
            continue;
        }
        // Bisect between verified endpoints; a probe may reclassify an endpoint under a
        // changing load, which only narrows the bracket again.
        for _ in 0..8 {
            if hi <= lo + 1 {
                break;
            }
            let ms = (lo + hi) / 2;
            tried.push(ms);
            match probe(q, ms) {
                Probe::TooSmall => lo = ms,
                Probe::Full => hi = ms,
                Probe::Partial => {
                    partial += 1;
                    break 'queries;
                }
            }
        }
    }
    assert!(
        partial >= 1,
        "no query was partially re-ranked under any of the probed budgets {tried:?} ms"
    );
}

#[test]
#[ignore = "needs both models"]
fn a_zero_budget_degrades_by_default_and_errors_in_strict_mode() {
    let tmp = tempfile::tempdir().unwrap();
    drop(support::build_fixture_index(tmp.path()));
    let ffi = open(tmp.path());
    let q = support::fixture_docs().queries[0].text.clone();
    let r = ffi.search(q.clone(), budgeted(0, false)).unwrap();
    assert!(r.stages.degraded.is_some(), "{:?}", r.stages);
    assert!(
        r.stages
            .rerank
            .as_ref()
            .is_some_and(|rr| rr.skipped.is_some())
    );
    assert!(!r.hits.is_empty(), "the lexical ranking still comes back");
    match ffi.search(q, budgeted(0, true)) {
        Err(XtrieverError::BudgetExhausted { message }) => assert!(!message.is_empty()),
        other => panic!("expected BudgetExhausted, got {other:?}"),
    }
}

/// FR-007: the clock starts when the call starts, not when the lock is won. A search that
/// contends on the handle behind a long one spends its budget waiting and arrives at check
/// point A with nothing left, so it degrades — it does not get a fresh budget on top of the
/// wait.
#[test]
#[ignore = "needs both models"]
fn a_search_queued_behind_another_spends_its_budget_while_it_waits() {
    let tmp = tempfile::tempdir().unwrap();
    drop(support::build_fixture_index(tmp.path()));
    let ffi = open(tmp.path());
    let q = support::fixture_docs().queries[0].text.clone();

    // Unbudgeted, full depth: holds the lock for the whole re-rank (seconds).
    let long = std::thread::spawn({
        let ffi = ffi.clone();
        let q = q.clone();
        move || {
            ffi.search(
                q,
                SearchOptions {
                    max_time_ms: None,
                    ..budgeted(0, false)
                },
            )
            .unwrap()
        }
    });
    // Give the long search time to enter and take the lock; 100 ms against a multi-second hold.
    std::thread::sleep(std::time::Duration::from_millis(100));

    let started = std::time::Instant::now();
    let r = ffi.search(q, budgeted(200, false)).unwrap();
    let waited = started.elapsed();
    let long = long.join().unwrap();

    assert!(
        waited.as_millis() > 200,
        "the budgeted search did not contend on the lock ({waited:?}); the long one took {} ms",
        long.elapsed_ms
    );
    assert!(
        r.elapsed_ms >= 200,
        "elapsed_ms must include the lock wait, got {}",
        r.elapsed_ms
    );
    assert!(r.stages.degraded.is_some(), "{:?}", r.stages);
    assert!(
        r.stages
            .rerank
            .as_ref()
            .is_some_and(|rr| rr.skipped.is_some()),
        "{:?}",
        r.stages
    );
    assert!(!r.hits.is_empty(), "the lexical ranking still comes back");
}
