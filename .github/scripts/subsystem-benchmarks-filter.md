# Subsystem benchmark filter: report-only rollout

The `report-impact` job proposes whether a pull request needs the five jobs in
`benchmarks-subsystem.yml`. Its JSON log and job summary explain the decision. All benchmark
jobs continue to run: `build` does not depend on this job or consume its result, and reporting
failures are allowed. There is no switch to enable skipping in this change.

Only PRs run the report job. Master pushes and merge queues retain their existing full benchmark
runs without an additional reporting runner. Sparse checkout includes all Cargo manifests, the
selector and its tests, and this workflow; it does not materialize Rust source files.

For PRs, checkout fetches two commits of history so both parents of the synthetic merge are
available. The selector verifies the base and head from the event against those parents, then
compares the actual checkout to its first parent. Rename detection is disabled to preserve both
old and new paths. Missing history, mismatched event data and unknown inputs propose a full run.
Invoking the selector directly for a non-PR event still proposes a full run.

The dependency model follows local Cargo path dependencies, including workspace inheritance,
optional dependencies, all targets and build dependencies. It includes development dependencies
of the benchmark roots, and conservatively includes local registry patches and known cross-crate
fixtures. It does not resolve feature combinations. Any manifest, root lockfile, toolchain or CI
configuration change proposes a full run. Files outside known packages also propose a full run;
`docs/` and `prdoc/` are excluded. Unsupported graphs propose a full run, including explicit
target or build-script paths outside their owning crate or inside a nested crate.

Keep `BENCHMARK_PACKAGES` in the selector synchronized with the workflow matrix. The report job
runs the contract and regression tests before emitting a proposed decision. The contract inspects
only `build.strategy.matrix.features`, so quoted job and step names do not affect it. Reporting uses
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

Before enabling any skipping, compare reports with full runs over representative PRs, including
changes to the benchmark roots, transitive dependencies, fixtures and global configuration.
Retain full runs on `master` and the merge queue during rollout. Historical resource savings are
aggregate job durations; PR completion-time savings depend on other jobs and runner queues.
