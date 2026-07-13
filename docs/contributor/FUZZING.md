# Fuzzing Guidelines for Polkadot SDK

This document describes how to write, run, and maintain fuzz tests in the Polkadot SDK repository.
It covers tooling setup, input design best practices, CI integration, and common pitfalls.

## Overview

The Polkadot SDK contains fuzz targets for critical components including arithmetic primitives,
election algorithms, pallet state machines, XCM message handling, and erasure coding. These fuzzers
use coverage-guided mutation to explore code paths and verify invariants.

We use [**ziggy**](https://github.com/srlabs/ziggy) as our fuzzing orchestrator. Ziggy drives
AFL++ (via `afl.rs`) under the hood, providing a unified interface for building, running, and
measuring coverage of fuzz targets. For substrate runtime fuzzers we run ziggy with the
`--no-honggfuzz` flag, as AFL++ alone provides more stable results in this codebase.

## Tooling: Ziggy

### Installation

```bash
# Install ziggy and its dependencies
cargo install --force ziggy cargo-afl honggfuzz grcov

# Configure the system for AFL++ (Linux only, requires root)
cargo afl system-config
```

On macOS, `cargo afl system-config` is not needed. The `system-config` step tunes kernel parameters
(core pattern, CPU scaling) that significantly improve fuzzing throughput on Linux.

### Running a Fuzzer

From the fuzzer's directory (e.g., `substrate/frame/bags-list/fuzzer`):

```bash
# Run with 4 parallel jobs, AFL++ only, 128-byte max input size
cargo ziggy fuzz -j 4 --no-honggfuzz -G 128
```

Key flags:

- `-j N` -- number of parallel AFL++ instances.
- `--no-honggfuzz` -- skip honggfuzz backend (recommended for substrate runtime fuzzers).
- `-G N` -- maximum input size in bytes. Keep this small to improve throughput.

### Generating Coverage Reports

```bash
# After at least 15 minutes of fuzzing
cargo ziggy cover -s ..

# If fuzzing ran for less than 15 minutes, point directly at the AFL++ queue
cargo ziggy cover -s .. -i output/<fuzzer-name>/afl/mainaflfuzzer/queue/
```

Coverage reports are generated via `grcov` and output to an HTML directory you can open in a
browser.

## Existing Fuzzers

| Component | Path | Targets | Framework |
|---|---|---|---|
| sp-arithmetic | `substrate/primitives/arithmetic/fuzzer` | biguint, normalize, per_thing_from_rational, per_thing_mult_fraction, multiply_by_rational_with_rounding, fixed_point | honggfuzz (migration pending) |
| sp-npos-elections | `substrate/primitives/npos-elections/fuzzer` | reduce, phragmen_balancing, phragmms_balancing, phragmen_pjr | ziggy |
| pallet-bags-list | `substrate/frame/bags-list/fuzzer` | bags-list | ziggy |
| pallet-nomination-pools | `substrate/frame/nomination-pools/fuzzer` | call | ziggy |
| pallet-paged-list | `substrate/frame/paged-list/fuzzer` | paged-list | ziggy |
| election-solution-type | `substrate/frame/election-provider-support/solution-type/fuzzer` | solution-type | ziggy |
| xcm-simulator | `polkadot/xcm/xcm-simulator/fuzzer` | xcm-fuzzer | ziggy |
| erasure-coding | `polkadot/erasure-coding/fuzzer` | reconstruct, round_trip | ziggy |
| sp-core | `substrate/primitives/core/fuzz` | fuzz_address_uri | ziggy |
| sp-state-machine | `substrate/primitives/state-machine/fuzz` | fuzz_append | ziggy |

## Writing a New Fuzzer

### Cargo.toml Setup

For a ziggy-based fuzzer:

```toml
[package]
name = "pallet-my-pallet-fuzzer"
version = "2.0.0"
edition.workspace = true
license = "Apache-2.0"
publish = false

[[bin]]
name = "my-pallet-fuzzer"
path = "src/main.rs"

[dependencies]
# The pallet under test -- enable the "fuzz" or "fuzzing" feature if available
pallet-my-pallet = { features = ["fuzz"], workspace = true, default-features = true }
# Ziggy orchestrates AFL++ under the hood
ziggy = { workspace = true }
```

Add the fuzzer crate as a workspace member in the root `Cargo.toml` and register it in the CI
workflow if it should run on PRs.

### Harness Code

A minimal harness using `ziggy::fuzz!`:

```rust
fn main() {
    ziggy::fuzz!(|data: &[u8]| {
        // Parse data into actions and execute them.
        // Check invariants after each action.
    });
}
```

For structured inputs, you can pass tuples or types that implement `arbitrary::Arbitrary`:

```rust
fn main() {
    ziggy::fuzz!(|data: (u64, u32, u8)| {
        let (value, weight, action_selector) = data;
        // ...
    });
}
```

Note: ziggy handles iteration internally -- do **not** wrap `ziggy::fuzz!` in a `loop {}`.

## Input Design

Good input design is the single most impactful factor for fuzzing effectiveness. The AFL++
documentation emphasizes that coverage-guided fuzzers work best when they can incrementally discover
new behavior by flipping individual bytes.

### Deterministic Byte Mapping

**Prefer deterministic, positional byte mapping over random number generators.**

The fuzzer's mutation engine works by flipping, splicing, and inserting bytes at specific positions
in the input buffer. When each byte position has a clear, consistent meaning, the fuzzer can
efficiently learn which bytes control which behavior.

Good -- each byte directly selects an action or argument:

```rust
ziggy::fuzz!(|data: &[u8]| {
    if data.len() < 3 { return; }
    let action = data[0] % 3;      // first byte selects action
    let arg1 = data[1] as u32;     // second byte is first argument
    let arg2 = data[2] as u64;     // third byte is second argument
    // ...
});
```

Good -- using an `InputBytes` reader (see `npos-elections` and `nomination-pools` fuzzers for
real-world examples):

```rust
struct InputBytes<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> InputBytes<'a> {
    fn new(bytes: &'a [u8]) -> Self { Self { bytes, pos: 0 } }

    fn next_u8(&mut self) -> u8 {
        if self.bytes.is_empty() { return 0; }
        let b = self.bytes[self.pos % self.bytes.len()];
        self.pos = self.pos.wrapping_add(1);
        b
    }

    fn range_u32(&mut self, min: u32, max: u32) -> u32 {
        if min == max { return min; }
        let span = max.saturating_sub(min).saturating_add(1);
        min + (self.next_u32() % span)
    }
    // ... next_u32, next_u64, range_u64 follow the same pattern
}

ziggy::fuzz!(|data: &[u8]| {
    let mut input = InputBytes::new(data);
    let action = input.next_u8() % 3;
    let amount = input.range_u64(10, 10_000);
    // ...
});
```

Good -- using `arbitrary` for structured decomposition:

```rust
ziggy::fuzz!(|data: (AccountId, VoteWeight, u32)| {
    let (account_id_seed, vote_weight, action_seed) = data;
    let id = account_id_seed % ID_RANGE;
    let action = Action::from(action_seed);
    // ...
});
```

### Structured Inputs with `arbitrary`

The `arbitrary` crate integrates directly with the `fuzz!` macro. Implement `Arbitrary` for custom
types when you need the fuzzer to generate valid domain objects:

```rust
use arbitrary::{Arbitrary, Unstructured};

enum Action {
    Insert { id: u32, weight: u64 },
    Update { id: u32, weight: u64 },
    Remove { id: u32 },
}

impl<'a> Arbitrary<'a> for Action {
    fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
        match u.int_in_range(0..=2)? {
            0 => Ok(Action::Insert { id: u.arbitrary()?, weight: u.arbitrary()? }),
            1 => Ok(Action::Update { id: u.arbitrary()?, weight: u.arbitrary()? }),
            _ => Ok(Action::Remove { id: u.arbitrary()? }),
        }
    }
}
```

### What to Avoid

**Do not use runtime random number generators (e.g., `rand::thread_rng()`) for selecting actions or
generating parameters.** This makes the fuzzer's mutations meaningless -- changing a byte in the
input won't predictably change the action taken. The fuzzer cannot learn what input bytes correspond
to what behavior.

Bad -- the fuzzer has no control over actions:

```rust
ziggy::fuzz!(|seed: [u8; 32]| {
    let mut rng = SmallRng::from_seed(seed);
    let action = rng.gen_range(0..3);  // Opaque to the coverage engine
    // ...
});
```

All fuzzers in this repo now use deterministic byte mapping. New fuzzers should use direct byte
mapping (via `InputBytes`) or `arbitrary` -- never RNG-seeded input.

**Limit enum/action spaces** to values the fuzzer can realistically explore. If you have 256
possible actions but only 8 are interesting, constrain the range.

**Keep inputs small.** Prefer `-G 128` or smaller. Smaller inputs mean faster execution and more
mutations per second. Only increase the limit if the target genuinely needs larger inputs (e.g., XCM
message sequences).

## Corpus Management

### Seed Corpus

Provide a `corpus/` directory with small, representative inputs that exercise the main code paths.
Good seeds include:

- Minimal valid inputs for each action type.
- Edge cases: zero values, maximum values, empty collections.
- Inputs that trigger each error path (if the fuzzer should exercise error handling).

For SCALE-encoded data, use `codec::Encode` to generate seed files programmatically.

### Corpus Minimization

After a fuzzing campaign, minimize the corpus to remove redundant inputs:

```bash
# Using AFL++ tools directly
afl-cmin -i output/queue -o minimized_corpus -- ./target/release/my-fuzzer

# Then optionally reduce individual inputs
afl-tmin -i minimized_corpus/input1 -o minimized_corpus/input1.min -- ./target/release/my-fuzzer
```

Ziggy handles some of this automatically when using `cargo ziggy cover`.

### Preserving Corpora Across Runs

Accumulated corpora find more bugs than fresh starts. When running in CI:

1. Cache the `output/<fuzzer>/afl/mainaflfuzzer/queue/` directory between runs.
2. Use minimized corpora as the input for subsequent campaigns.
3. New seeds can be added mid-campaign by placing them in the corpus directory.

## Effective Fuzzer Properties

### State Invariant Checking

The most valuable pattern for pallet fuzzers: execute a random action, then verify that all state
invariants hold. This is the pattern used by `pallet-bags-list` and `pallet-nomination-pools`:

```rust
ziggy::fuzz!(|data: &[u8]| {
    let mut input = InputBytes::new(data);
    let action = Action::from(input.next_u8());
    match action {
        Action::Insert => { /* ... */ },
        Action::Update => { /* ... */ },
        Action::Remove => { /* ... */ },
    }
    // Verify all invariants after every action
    assert!(MyPallet::do_try_state().is_ok());
});
```

Use `do_try_state()` or `try_state()` methods when available. These perform comprehensive state
consistency checks that catch bugs no single assertion can.

### Round-Trip / Differential Testing

Verify that encode-decode round-trips are lossless, or that two implementations produce identical
results:

```rust
ziggy::fuzz!(|data: (u128, u128, u128)| {
    let (a, b, c) = data;
    let fast_result = fast_multiply_rational(a, b, c);
    let reference_result = BigInt::from(a) * BigInt::from(b) / BigInt::from(c);
    assert_eq!(fast_result, reference_result);
});
```

This pattern is used by the `sp-arithmetic` fuzzers to verify optimized math against reference
implementations.

### Action Sequences

For stateful fuzzers, generate sequences of actions and verify invariants across the sequence:

```rust
ziggy::fuzz!(|data: &[u8]| {
    let mut input = InputBytes::new(data);
    let mut state = TestState::new();
    let num_actions = input.range_u32(1, MAX_ACTIONS as u32);
    for _ in 0..num_actions {
        let action = Action::from_bytes(&mut input);
        state.apply(&action);
        state.check_invariants();
    }
});
```

Limit the number of actions per sequence to keep execution time bounded.

## Performance Tips

Ranked by impact:

1. **Keep inputs small** (`-G 128` or less). Smaller inputs = faster mutations = more executions per
   second.
2. **Minimize initialization overhead**. Set up test externalities once outside `ziggy::fuzz!`
   when possible (see the `bags-list` fuzzer pattern with `ExtBuilder`).
3. **Use selective instrumentation**. Only the code under test needs coverage instrumentation. Large
   dependency trees slow down fuzzing without adding useful coverage signal.
4. **Use tmpfs for output** (Linux). Point `AFL_TMPDIR` to a RAM-backed filesystem to reduce I/O
   overhead.
5. **Enable fast calibration** (`AFL_FAST_CAL=1`). Halves the time spent calibrating saturated
   corpus entries. Especially useful in CI with short time budgets.
6. **Set memory cache size** (`AFL_TESTCACHE_SIZE=50` to `500`). Caches testcase data in memory,
   reducing disk reads.

## Multi-Core Fuzzing

AFL++ benefits from multiple parallel instances with diverse configurations:

```bash
# Simple: ziggy handles parallelism automatically
cargo ziggy fuzz -j 4 --no-honggfuzz -G 128
```

When running manually with AFL++ directly, diversify instances:

- **1 main instance** (`-M main`) with `AFL_FINAL_SYNC=1`.
- **Remaining secondaries** (`-S variant-N`) with varied configurations.
- Mix power schedules across instances: `explore` (default), `fast`, `rare`, `exploit`.
- Allocate ~40% with `-P explore` mode, ~20% with `-P exploit`.
- Useful limit: 32-64 cores per target. Beyond that, diminishing returns.

## Common Pitfalls

### Randomness Undermines Coverage Guidance

If the harness uses `rand::Rng` seeded from the fuzzer input, byte mutations in the input propagate
unpredictably through the RNG, defeating the fuzzer's ability to correlate byte changes with
coverage changes. Use deterministic byte mapping instead (see [Input Design](#input-design)).

### State Leaking Between Iterations

In persistent mode, any state that persists between iterations can cause non-determinism. The fuzzer
tracks a "stability" metric -- if it drops below 90%, you likely have state leaks.

Solutions:

- Reset storage/state at the start of each iteration.
- Use `ExtBuilder::default().build_and_execute(|| ziggy::fuzz!(...))` to wrap iterations in a
  fresh externalities context.
- If state accumulation is intentional (e.g., nomination-pools building up pools over time), accept
  lower stability but be aware it reduces fuzzing effectiveness.

### Cryptographic Checks Block Coverage

Signature verification, hash validation, and Merkle proof checks prevent the fuzzer from reaching
code behind them. In fuzzing builds:

- Use feature flags (e.g., `#[cfg(feature = "fuzzing")]`) to bypass signature verification.
- Decode SCALE data without cryptographic validation.
- Gate these bypasses so they never compile into production builds.

### Panicking on Invalid Input

Fuzzers will send garbage. The harness should handle invalid inputs gracefully -- return early or
skip the iteration. Only `panic!` / `assert!` on invariant violations that represent actual bugs.

```rust
ziggy::fuzz!(|data: &[u8]| {
    // Good: gracefully reject bad input
    let Ok(action) = Action::try_from(data) else { return };
    // Now test the action...
});
```

### Oversized Input Buffers

Large inputs slow down execution and make it harder for the fuzzer to discover coverage. Most pallet
fuzzers need fewer than 128 bytes. Only increase `-G` if the target genuinely requires larger
inputs.

## CI Integration

The fuzzing CI workflow (`.github/workflows/tests-fuzzing.yml`) automatically detects which fuzzers
to run based on changed files. To add a new fuzzer to CI:

1. Add a file detection entry in the `changed-files` step.
2. Add a corresponding `add_fuzzer` call in the matrix builder.
3. Specify the framework: use `ziggy` for new fuzzers.

CI fuzzers run with a 60-second time budget per target. This is short -- the goal is regression
detection, not deep exploration. For thorough fuzzing, run locally for hours or days.

## References

- [Ziggy documentation](https://github.com/srlabs/ziggy)
- [AFL++ documentation](https://github.com/AFLplusplus/AFLplusplus/tree/stable/docs)
  - [Fuzzing in depth](https://github.com/AFLplusplus/AFLplusplus/blob/stable/docs/fuzzing_in_depth.md) --
    comprehensive guide covering corpus design, multi-core strategies, and performance tuning
  - [FAQ](https://github.com/AFLplusplus/AFLplusplus/blob/stable/docs/FAQ.md)
- [afl.rs](https://github.com/rust-fuzz/afl.rs) -- Rust bindings for AFL++
- [arbitrary crate](https://docs.rs/arbitrary) -- structured fuzzer input generation
- [honggfuzz-rs](https://docs.rs/honggfuzz) -- Rust bindings for honggfuzz
