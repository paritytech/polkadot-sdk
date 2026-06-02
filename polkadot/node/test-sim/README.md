# polkadot-subsystem-test-sim

A subsystem-agnostic, **deterministic** simulator for testing Polkadot node subsystems.

Instead of standing up a real overseer and hand-driving its message pump, you describe a chain
(schedule, claim queues, scheduling lookahead, sessions) as *facts*, drive the subsystem under
test (SUT) with stimuli, and assert on the **observable contract** — the effects the subsystem
emits (`SendRequest`, `DisconnectPeers`, reputation changes, …). A controlled `MockClock` makes
time explicit, so timeouts and delays are driven, not waited on.

```rust
// CQ = [B, A, A]: para B holds the earliest slot, A the next two. Both collators must fetch.
#[crate::sim_test]
fn shared_core_para_b_can_fetch_alongside_para_a<S: CollatorSut>() {
    let mut w = shared_core_world::<S>();
    let leaf = w.leaf();
    let leaf_n = w.leaf_number();

    let a1 = Candidate::builder()
        .para(PARA_A).relay_parent(leaf).relay_parent_number(leaf_n)
        .parent_head(HeadData(Vec::new())).head_data(HeadData(vec![1])).build();
    let b1 = Candidate::builder()
        .para(PARA_B).relay_parent(leaf).relay_parent_number(leaf_n)
        .parent_head(HeadData(Vec::new())).head_data(HeadData(vec![10])).build();

    let peer_a = w.declared_peer(PARA_A, V2);
    let peer_b = w.declared_peer(PARA_B, V2);

    // Drive the scenario; `full_second` advertises → fetches → seconds, asserting each effect.
    w.full_second(&peer_a, &a1);
    w.full_second(&peer_b, &b1);
}
```

## Layout

- `polkadot-subsystem-test-sim` (this crate) — the generic engine: chain model, deterministic
  runtime + clock, harness, recorder/classifier, query responders, real auxiliary subsystems,
  builders, report rendering, and the [known-bug support](#known-bug-tests--test-first-flow).
  Knows nothing about any specific subsystem.
- `polkadot-subsystem-test-sim-macros` — generic proc-macros, currently `#[known_bug]`.
- Per-subsystem consumer crates wire the engine to one production subsystem. Collator-protocol
  is the pilot: it adds a `ClockAdapter`, a `SubsystemUnderTest` impl, and a `#[sim_test]` macro
  that fans one scenario out across its two validator implementations (legacy + experimental).

## 1. Why this exists — what it buys you

Compared to hand-rolled subsystem tests against a mock overseer:

- **~10× less code, and far less of the brittle kind.** A scenario is the prose of the test;
  the harness plumbing is gone. No `assert_matches!(overseer.recv(), RuntimeApi(...))` ladders.
- **Order-independent.** Mock-overseer tests must answer runtime requests in the exact order the
  subsystem happens to make them — get it wrong and the test hangs. Here the chain model answers
  any query in any order, so you model *facts*, not the subsystem's internal control flow.
- **Differential testing is a free correctness oracle.** When a subsystem has two
  implementations (collator-protocol's legacy and experimental), one scenario runs against both
  and any divergence in observable behavior fails the test — which *locates* a bug to one side
  with no extra work.
- **Deterministic time.** Timeouts, delays and reputation decay are driven via the mock clock
  (`advance(dur)`), not slept on. No flakes, no wall-clock waits.
- **Observability.** Assertions read the recorded effect stream, so a failure shows exactly which
  effect was (not) emitted and when, instead of being buried in a message exchange.

These compound: cheap, sharp tests mean the edge cases actually get written. The collator-protocol
work that introduced this framework surfaced several distinct production bugs (claim-queue /
scheduling-lookahead handling at session boundaries among them) precisely because writing the
edge-case scenario was a five-minute job rather than an afternoon of harness wrestling.

## 2. Known-bug tests & test-first flow

A **known-bug test** asserts the *correct* behavior of something that is currently broken. While
the bug is open the test body fails; the framework swallows that failure so the suite stays green.
The moment the body stops failing — the bug got fixed — the test fails loudly, telling you to
remove the marker.

```rust
#[known_bug(url = "github:paritytech/polkadot-sdk#12345")]
#[test]
fn rejects_double_spend() {
    // asserts the FIXED behavior. Panics today (bug open) → swallowed → green.
    // Stops panicking once #12345 lands → fails with:
    //   KNOWN-BUG TEST PASSED: `rejects_double_spend` no longer fails — the tracked bug
    //   appears FIXED. Remove the known-bug marker ... Tracking: github:...#12345
}
```

`url` is optional (`#[known_bug]` is fine for a bug with no issue yet). The attribute expands to
[`run_known_bug`], which any test — sim-based or not — can also call directly. Subsystems with a
fan-out macro compose the same primitive: collator-protocol's `#[sim_test(bug_on = "experimental",
bug_url = "...")]` marks the experimental wrapper as a known bug while the legacy wrapper asserts
normally, so the *differential* itself becomes the known-bug record.

### The flow this enables

1. **Land the tests first.** Write the scenarios for the desired behavior and merge them marked
   `#[known_bug(...)]`. The suite stays green; CI documents the gap; the expected behavior is now
   pinned and reviewable *before* any implementation exists.
2. **Fix in a separate PR.** The implementation PR makes the behavior correct. As a result the
   known-bug tests start passing — which the framework turns into a *loud failure* until the
   markers are removed.
3. **Remove the markers in that same PR.** Because `#[known_bug]` is a single attribute line, the
   removal is a one-line-per-test deletion that leaves the test body (and its indentation)
   untouched. The fix PR's diff therefore reads as: *here is the behavior change, and here are
   exactly the tests it makes pass.* The fix and its proof are legible in one place.

This keeps red tests first-class (no `#[ignore]` that silently rots, no commented-out asserts),
and makes "what did this PR actually fix" answerable from the diff alone.

[`run_known_bug`]: src/known_bug.rs

## Wiring up a new subsystem

See the crate-level docs in `src/lib.rs` (`SubsystemUnderTest`, the `ClockAdapter` pattern, why a
hand-rolled harness rather than a real overseer) and the collator-protocol consumer crate for the
canonical end-to-end example.
