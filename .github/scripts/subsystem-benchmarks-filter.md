# Subsystem benchmark selection

The `select-benchmarks` job decides whether a pull request needs the five jobs in
`benchmarks-subsystem.yml`. Its JSON log and job summary explain the decision. A successful
selection with `run_benchmarks=false` skips the entire benchmark matrix. Unknown inputs and
dependency or checkout errors select a full run.

Only PRs run the selection job. Master pushes and merge queues always run the full benchmark
suite without a selection runner. Sparse checkout includes all Cargo manifests, the
selector and its tests, and this workflow; it does not materialize Rust source files.

Selection requires successful preflight but does not use its broad `changes_rust` output.
This allows unrelated Rust changes to skip benchmarks, as well as excluded documentation changes.
The benchmark gate explicitly checks successful preflight and workflow cancellation. A failed,
timed-out or skipped selector, or a missing decision, falls back to the full suite. The selection
job allows failure, and only a successful selector step can publish a skip decision. The gate's
`!cancelled()` status check also allows full runs when selection is skipped on non-PR events.

For PRs, checkout fetches two commits of history so both parents of the synthetic merge are
available. The selector verifies the base and head from the event against those parents, then
compares the actual checkout to its first parent. Rename detection is disabled to preserve both
old and new paths. Missing history, mismatched event data and unknown inputs select a full run.
Invoking the selector directly for a non-PR event also selects a full run.

The dependency model follows local Cargo path dependencies, including workspace inheritance,
optional dependencies, all targets and build dependencies. It includes development dependencies
of the benchmark roots, and conservatively includes local registry patches and known cross-crate
fixtures. It does not resolve feature combinations. Any manifest, root lockfile, toolchain or CI
configuration change selects a full run. Files outside known packages also select a full run;
`prdoc/` and files ending in `.md`, `.mmd` or `.png` under `docs/` are excluded, matching shared
preflight. Code and other files under `docs/` use the same dependency checks as other build inputs;
unknown inputs select a full run. Unsupported graphs select a full run, including explicit target
or build-script paths outside their owning crate or inside a nested crate.

Keep `BENCHMARK_PACKAGES` in the selector synchronized with the workflow matrix. The selection job
runs the contract and regression tests before emitting a decision. The contract inspects
only `build.strategy.matrix.features`, so quoted job and step names do not affect it. Selection uses
the actual checked-out manifests and does not require Cargo, third-party Python packages or a Rust
build. Python 3.11 or newer is required.

Run the tests locally:

```sh
python3 .github/scripts/test_subsystem_benchmarks_filter.py
```

Replay a checked-out synthetic PR merge locally, using its actual first-parent SHA:

```sh
python3 .github/scripts/subsystem_benchmarks_filter.py --event pull_request --base BASE_SHA
```

The CLI writes `run_benchmarks=true` or `run_benchmarks=false` to `GITHUB_OUTPUT` after completing
its log and summary. Local invocation without `GITHUB_OUTPUT` prints the same decision as JSON.
Historical replay validates selection against actual merge checkouts; measured job durations
estimate avoidable runner usage, not end-to-end PR completion-time savings. Future savings depend
on PR volume, changed inputs and selection overhead.
