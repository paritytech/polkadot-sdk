# Subsystem benchmark client

Run parachain consensus stress and performance tests on your development machine or in CI.

## Motivation

The parachain consensus node implementation spans across many modules which we call subsystems. Each subsystem is
responsible for a small part of logic of the parachain consensus pipeline, but in general the most load and
performance issues are localized in just a few core subsystems like `availability-recovery`, `approval-voting` or
`dispute-coordinator`. In the absence of such a tool, we would run large test nets to load/stress test these parts of
the system. Setting up and making sense of the amount of data produced by such a large test is very expensive, hard
to orchestrate and is a huge development time sink.

This tool aims to solve the problem by making it easy to:

- set up and run core subsystem load tests locally on your development machine
- iterate and conclude faster when benchmarking new optimizations or comparing implementations
- automate and keep track of performance regressions in CI runs
- simulate various networking topologies, bandwidth and connectivity issues

## Test environment setup

`cargo build --profile=testnet --bin subsystem-bench -p polkadot-subsystem-bench`

The output binary will be placed in `target/testnet/subsystem-bench`.

### Test metrics

Subsystem, CPU usage and network metrics are exposed via a prometheus endpoint during the test execution.
A small subset of these collected metrics are displayed in the CLI, but for an in depth analysis of the test results,
a local Grafana/Prometheus stack is needed.

### Run Prometheus, Pyroscope and Graphana in Docker

If docker is not usable, then follow the next sections to manually install Prometheus, Pyroscope and Graphana
on your machine.

```bash
cd polkadot/node/subsystem-bench/docker
docker compose up
```

### Install Prometheus

Please follow the [official installation guide](https://prometheus.io/docs/prometheus/latest/installation/) for your
platform/OS.

After successfully installing and starting up Prometheus, we need to alter it's configuration such that it
will scrape the benchmark prometheus endpoint `127.0.0.1:9999`. Please check the prometheus official documentation
regarding the location of `prometheus.yml`. On MacOS for example the full path `/opt/homebrew/etc/prometheus.yml`

prometheus.yml:

```
global:
  scrape_interval: 5s

scrape_configs:
  - job_name: "prometheus"
    static_configs:
    - targets: ["localhost:9090"]
  - job_name: "subsystem-bench"
    scrape_interval: 0s500ms
    static_configs:
    - targets: ['localhost:9999']
```

To complete this step restart Prometheus server such that it picks up the new configuration.

### Install Pyroscope

To collect CPU profiling data, you must be running the Pyroscope server.
Follow the [installation guide](https://grafana.com/docs/pyroscope/latest/get-started/)
relevant to your operating system.

### Install Grafana

Follow the [installation guide](https://grafana.com/docs/grafana/latest/setup-grafana/installation/) relevant
to your operating system.

### Setup Grafana

Once you have the installation up and running, configure the local Prometheus and Pyroscope (if needed)
as data sources by following these guides:

- [Prometheus](https://grafana.com/docs/grafana/latest/datasources/prometheus/configure-prometheus-data-source/)
- [Pyroscope](https://grafana.com/docs/grafana/latest/datasources/grafana-pyroscope/)

If you are running the servers in Docker, use the following URLs:

- Prometheus `http://prometheus:9090/`
- Pyroscope `http://pyroscope:4040/`

#### Import dashboards

Follow [this guide](https://grafana.com/docs/grafana/latest/dashboards/manage-dashboards/#export-and-import-dashboards)
to import the dashboards from the repository `grafana` folder.

### Standard test options

```
$ subsystem-bench --help
Usage: subsystem-bench [OPTIONS] <PATH>

Arguments:
  <PATH>  Path to the test sequence configuration file

Options:
      --profile                                        Enable CPU Profiling with Pyroscope
      --pyroscope-url <PYROSCOPE_URL>                  Pyroscope Server URL [default: http://localhost:4040]
      --pyroscope-sample-rate <PYROSCOPE_SAMPLE_RATE>  Pyroscope Sample Rate [default: 113]
      --cache-misses                                   Enable Cache Misses Profiling with Valgrind. Linux only, Valgrind must be in the PATH
  -h, --help                                           Print help
```

## How to run a test

To run a test, you need to use a path to a test objective:

```
target/testnet/subsystem-bench polkadot/node/subsystem-bench/examples/availability_read.yaml
```

Note: test objectives may be wrapped up into a test sequence.
It is typically used to run a suite of tests like in this [example](examples/availability_read.yaml).

### Understanding the test configuration

A single test configuration `TestConfiguration` struct applies to a single run of a certain test objective.

The configuration describes the following important parameters that influence the test duration and resource
usage:

- how many validators are on the emulated network (`n_validators`)
- how many cores per block the subsystem will have to do work on (`n_cores`)
- for how many blocks the test should run (`num_blocks`)

From the perspective of the subsystem under test, this means that it will receive an `ActiveLeavesUpdate` signal
followed by an arbitrary amount of messages. This process repeats itself for `num_blocks`. The messages are generally
test payloads pre-generated before the test run, or constructed on pre-generated payloads. For example the
`AvailabilityRecoveryMessage::RecoverAvailableData` message includes a `CandidateReceipt` which is generated before
the test is started.

### Example run

Let's run an availability read test which will recover availability for 200 cores with max PoV size on a 1000
node validator network.

<!-- markdownlint-disable line-length -->

```
target/testnet/subsystem-bench polkadot/node/subsystem-bench/examples/availability_write.yaml
[2024-02-19T14:10:32.981Z INFO  subsystem_bench] Sequence contains 1 step(s)
[2024-02-19T14:10:32.981Z INFO  subsystem-bench::cli] Step 1/1
[2024-02-19T14:10:32.981Z INFO  subsystem-bench::cli] [objective = DataAvailabilityWrite] n_validators = 1000, n_cores = 200, pov_size = 5120 - 5120, connectivity = 75, latency = Some(PeerLatency { mean_latency_ms: 30, std_dev: 2.0 })
[2024-02-19T14:10:32.982Z INFO  subsystem-bench::availability] Generating template candidate index=0 pov_size=5242880
[2024-02-19T14:10:33.106Z INFO  subsystem-bench::availability] Created test environment.
[2024-02-19T14:10:33.106Z INFO  subsystem-bench::availability] Pre-generating 600 candidates.
[2024-02-19T14:10:34.096Z INFO  subsystem-bench::network] Initializing emulation for a 1000 peer network.
[2024-02-19T14:10:34.096Z INFO  subsystem-bench::network] connectivity 75%, latency Some(PeerLatency { mean_latency_ms: 30, std_dev: 2.0 })
[2024-02-19T14:10:34.098Z INFO  subsystem-bench::network] Network created, connected validator count 749
[2024-02-19T14:10:34.099Z INFO  subsystem-bench::availability] Seeding availability store with candidates ...
[2024-02-19T14:10:34.100Z INFO  substrate_prometheus_endpoint] 〽️ Prometheus exporter started at 127.0.0.1:9999
[2024-02-19T14:10:34.387Z INFO  subsystem-bench::availability] Done
[2024-02-19T14:10:34.387Z INFO  subsystem-bench::availability] Current block #1
[2024-02-19T14:10:34.389Z INFO  subsystem-bench::availability] Waiting for all emulated peers to receive their chunk from us ...
[2024-02-19T14:10:34.625Z INFO  subsystem-bench::availability] All chunks received in 237ms
[2024-02-19T14:10:34.626Z INFO  polkadot_subsystem_bench::availability] Waiting for 749 bitfields to be received and processed
[2024-02-19T14:10:35.710Z INFO  subsystem-bench::availability] All bitfields processed
[2024-02-19T14:10:35.710Z INFO  subsystem-bench::availability] All work for block completed in 1322ms
[2024-02-19T14:10:35.710Z INFO  subsystem-bench::availability] Current block #2
[2024-02-19T14:10:35.712Z INFO  subsystem-bench::availability] Waiting for all emulated peers to receive their chunk from us ...
[2024-02-19T14:10:35.947Z INFO  subsystem-bench::availability] All chunks received in 236ms
[2024-02-19T14:10:35.947Z INFO  polkadot_subsystem_bench::availability] Waiting for 749 bitfields to be received and processed
[2024-02-19T14:10:37.038Z INFO  subsystem-bench::availability] All bitfields processed
[2024-02-19T14:10:37.038Z INFO  subsystem-bench::availability] All work for block completed in 1328ms
[2024-02-19T14:10:37.039Z INFO  subsystem-bench::availability] Current block #3
[2024-02-19T14:10:37.040Z INFO  subsystem-bench::availability] Waiting for all emulated peers to receive their chunk from us ...
[2024-02-19T14:10:37.276Z INFO  subsystem-bench::availability] All chunks received in 237ms
[2024-02-19T14:10:37.276Z INFO  polkadot_subsystem_bench::availability] Waiting for 749 bitfields to be received and processed
[2024-02-19T14:10:38.362Z INFO  subsystem-bench::availability] All bitfields processed
[2024-02-19T14:10:38.362Z INFO  subsystem-bench::availability] All work for block completed in 1323ms
[2024-02-19T14:10:38.362Z INFO  subsystem-bench::availability] All blocks processed in 3974ms
[2024-02-19T14:10:38.362Z INFO  subsystem-bench::availability] Avg block time: 1324 ms
[2024-02-19T14:10:38.362Z INFO  parachain::availability-store] received `Conclude` signal, exiting
[2024-02-19T14:10:38.362Z INFO  parachain::bitfield-distribution] Conclude
[2024-02-19T14:10:38.362Z INFO  subsystem-bench::network] Downlink channel closed, network interface task exiting

polkadot/node/subsystem-bench/examples/availability_write.yaml #1 DataAvailabilityWrite

Network usage, KiB                     total   per block
Received from peers                12922.000    4307.333
Sent to peers                      47705.000   15901.667

CPU usage, seconds                     total   per block
availability-distribution              0.045       0.015
bitfield-distribution                  0.104       0.035
availability-store                     0.304       0.101
Test environment                       3.213       1.071
```

<!-- markdownlint-enable line-length -->

`Block time` in the current context has a different meaning. It measures the amount of time it
took the subsystem to finish processing all of the messages sent in the context of the current test block.

### Test logs

You can select log target, subtarget and verbosity just like with Polkadot node CLI, simply setting
`RUST_LOOG="parachain=debug"` turns on debug logs for all parachain consensus subsystems in the test.

### View test metrics

Assuming the Grafana/Prometheus stack installation steps completed successfully, you should be able to
view the test progress in real time by accessing [this link](http://localhost:3000/goto/SM5B8pNSR?orgId=1).

Now run
`target/testnet/subsystem-bench test-sequence --path polkadot/node/subsystem-bench/examples/availability_read.yaml`
and view the metrics in real time and spot differences between different `n_validators` values.

### Profiling cache misses

Cache misses are profiled using Cachegrind, part of Valgrind. Cachegrind runs slowly, and its cache simulation is basic
and unlikely to reflect the behavior of a modern machine. However, it still represents the general situation with cache
usage, and more importantly it doesn't require a bare-metal machine to run on, which means it could be run in CI or in
a remote virtual installation.

To profile cache misses use the `--cache-misses` flag. Cache simulation of current runs tuned for Intel Ice Lake CPU.
Since the execution will be very slow, it's recommended not to run it together with other profiling and not to take
benchmark results into account. A report is saved in a file `cachegrind_report.txt`.

Example run results:

```
$ target/testnet/subsystem-bench --cache-misses cache-misses-data-availability-read.yaml
$ cat cachegrind_report.txt
I refs:        64,622,081,485
I1  misses:         3,018,168
LLi misses:           437,654
I1  miss rate:           0.00%
LLi miss rate:           0.00%

D refs:        12,161,833,115  (9,868,356,364 rd   + 2,293,476,751 wr)
D1  misses:       167,940,701  (   71,060,073 rd   +    96,880,628 wr)
LLd misses:        33,550,018  (   16,685,853 rd   +    16,864,165 wr)
D1  miss rate:            1.4% (          0.7%     +           4.2%  )
LLd miss rate:            0.3% (          0.2%     +           0.7%  )

LL refs:          170,958,869  (   74,078,241 rd   +    96,880,628 wr)
LL misses:         33,987,672  (   17,123,507 rd   +    16,864,165 wr)
LL miss rate:             0.0% (          0.0%     +           0.7%  )
```

The results show that 1.4% of the L1 data cache missed, but the last level cache only missed 0.3% of the time.
Instruction data of the L1 has 0.00%.

Cachegrind writes line-by-line cache profiling information to a file named `cachegrind.out.<pid>`.
This file is best interpreted with `cg_annotate --auto=yes cachegrind.out.<pid>`. For more information see the
[cachegrind manual](https://www.cs.cmu.edu/afs/cs.cmu.edu/project/cmt-40/Nice/RuleRefinement/bin/valgrind-3.2.0/docs/html/cg-manual.html).

For finer profiling of cache misses, better use `perf` on a bare-metal machine.

## Create new test objectives

This tool is intended to make it easy to write new test objectives that focus individual subsystems,
or even multiple subsystems (for example `approval-distribution` and `approval-voting`).

A special kind of test objectives are performance regression tests for the CI pipeline. These should be sequences
of tests that check the performance characteristics (such as CPU usage, speed) of the subsystem under test in both
happy and negative scenarios (low bandwidth, network errors and low connectivity).

### Reusable test components

To faster write a new test objective you need to use some higher level wrappers and logic: `TestEnvironment`,
`TestConfiguration`, `TestAuthorities`, `NetworkEmulator`. To create the `TestEnvironment` you will
need to also build an `Overseer`, but that should be easy using the mockups for subsystems in `mock`.

### Mocking

Ideally we want to have a single mock implementation for subsystems that can be minimally configured to
be used in different tests. A good example is `runtime-api` which currently only responds to session information
requests based on static data. It can be easily extended to service other requests.
