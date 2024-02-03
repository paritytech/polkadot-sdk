# Subsystem benchmark client

Run parachain consensus stress and performance tests on your development machine.  

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
A small subset of these collected metrics are displayed in the CLI, but for an in depth analysys of the test results,
a local Grafana/Prometheus stack is needed.

### Run Prometheus, Pyroscope and Graphana in Docker

If docker is not usable, then follow the next sections to manually install Prometheus, Pyroscope and Graphana on your machine.

```bash
cd polkadot/node/subsystem-bench/docker
docker compose up
```

### Install Prometheus

Please follow the [official installation guide](https://prometheus.io/docs/prometheus/latest/installation/) for your
platform/OS.

After succesfully installing and starting up Prometheus, we need to alter it's configuration such that it
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

## How to run a test

To run a test, you need to first choose a test objective. Currently, we support the following:

```
target/testnet/subsystem-bench --help
The almighty Subsystem Benchmark Tool™️

Usage: subsystem-bench [OPTIONS] <COMMAND>

Commands:
  data-availability-read  Benchmark availability recovery strategies

```

Note: `test-sequence` is a special test objective that wraps up an arbitrary number of test objectives. It is tipically
 used to run a suite of tests defined in a `yaml` file like in this [example](examples/availability_read.yaml).

### Standard test options
  
```
      --network <NETWORK>                              The type of network to be emulated [default: ideal] [possible values: ideal, healthy,
                                                       degraded]
      --n-cores <N_CORES>                              Number of cores to fetch availability for [default: 100]
      --n-validators <N_VALIDATORS>                    Number of validators to fetch chunks from [default: 500]
      --min-pov-size <MIN_POV_SIZE>                    The minimum pov size in KiB [default: 5120]
      --max-pov-size <MAX_POV_SIZE>                    The maximum pov size bytes [default: 5120]
  -n, --num-blocks <NUM_BLOCKS>                        The number of blocks the test is going to run [default: 1]
  -p, --peer-bandwidth <PEER_BANDWIDTH>                The bandwidth of emulated remote peers in KiB
  -b, --bandwidth <BANDWIDTH>                          The bandwidth of our node in KiB
      --connectivity <CONNECTIVITY>                    Emulated peer connection ratio [0-100]
      --peer-mean-latency <PEER_MEAN_LATENCY>          Mean remote peer latency in milliseconds [0-5000]
      --peer-latency-std-dev <PEER_LATENCY_STD_DEV>    Remote peer latency standard deviation
      --profile                                        Enable CPU Profiling with Pyroscope
      --pyroscope-url <PYROSCOPE_URL>                  Pyroscope Server URL [default: http://localhost:4040]
      --pyroscope-sample-rate <PYROSCOPE_SAMPLE_RATE>  Pyroscope Sample Rate [default: 113]
      --cache-misses                                   Enable Cache Misses Profiling with Valgrind. Linux only, Valgrind must be in the PATH
  -h, --help                                           Print help
```

These apply to all test objectives, except `test-sequence` which relies on the values being specified in a file.

### Test objectives

Each test objective can have it's specific configuration options, in contrast with the standard test options.

For `data-availability-read` the recovery strategy to be used is configurable.

```
target/testnet/subsystem-bench data-availability-read --help
Benchmark availability recovery strategies

Usage: subsystem-bench data-availability-read [OPTIONS]

Options:
  -f, --fetch-from-backers  Turbo boost AD Read by fetching the full availability datafrom backers first. Saves CPU 
                            as we don't need to re-construct from chunks. Tipically this is only faster if nodes 
                            have enough bandwidth
  -h, --help                Print help
```

### Understanding the test configuration

A single test configuration `TestConfiguration` struct applies to a single run of a certain test objective.

The configuration describes the following important parameters that influence the test duration and resource
usage:

- how many validators are on the emulated network (`n_validators`)
- how many cores per block the subsystem will have to do work on (`n_cores`)
- for how many blocks the test should run (`num_blocks`)

From the perspective of the subsystem under test, this means that it will receive an `ActiveLeavesUpdate` signal
followed by an arbitrary amount of messages. This process repeats itself for `num_blocks`. The messages are generally
test payloads pre-generated before the test run, or constructed on pre-genereated payloads. For example the
`AvailabilityRecoveryMessage::RecoverAvailableData` message includes a `CandidateReceipt` which is generated before
the test is started.

### Example run

Let's run an availabilty read test which will recover availability for 10 cores with max PoV size on a 500
node validator network.

```
 target/testnet/subsystem-bench --n-cores 10 data-availability-read 
[2023-11-28T09:01:59Z INFO  subsystem_bench::core::display] n_validators = 500, n_cores = 10, pov_size = 5120 - 5120, 
                                                            latency = None
[2023-11-28T09:01:59Z INFO  subsystem-bench::availability] Generating template candidate index=0 pov_size=5242880
[2023-11-28T09:01:59Z INFO  subsystem-bench::availability] Created test environment.
[2023-11-28T09:01:59Z INFO  subsystem-bench::availability] Pre-generating 10 candidates.
[2023-11-28T09:02:01Z INFO  subsystem-bench::core] Initializing network emulation for 500 peers.
[2023-11-28T09:02:01Z INFO  substrate_prometheus_endpoint] 〽️ Prometheus exporter started at 127.0.0.1:9999
[2023-11-28T09:02:01Z INFO  subsystem-bench::availability] Current block 1/1
[2023-11-28T09:02:01Z INFO  subsystem_bench::availability] 10 recoveries pending
[2023-11-28T09:02:04Z INFO  subsystem_bench::availability] Block time 3231ms
[2023-11-28T09:02:04Z INFO  subsystem-bench::availability] Sleeping till end of block (2768ms)
[2023-11-28T09:02:07Z INFO  subsystem_bench::availability] All blocks processed in 6001ms
[2023-11-28T09:02:07Z INFO  subsystem_bench::availability] Throughput: 51200 KiB/block
[2023-11-28T09:02:07Z INFO  subsystem_bench::availability] Block time: 6001 ms
[2023-11-28T09:02:07Z INFO  subsystem_bench::availability] 
    
    Total received from network: 66 MiB
    Total sent to network: 58 KiB
    Total subsystem CPU usage 4.16s
    CPU usage per block 4.16s
    Total test environment CPU usage 0.00s
    CPU usage per block 0.00s
```

`Block time` in the context of `data-availability-read` has a different meaning. It measures the amount of time it
took the subsystem to finish processing all of the messages sent in the context of the current test block.

### Test logs

You can select log target, subtarget and verbosity just like with Polkadot node CLI, simply setting
`RUST_LOOG="parachain=debug"` turns on debug logs for all parachain consensus subsystems in the test.

### View test metrics

Assuming the Grafana/Prometheus stack installation steps completed succesfully, you should be able to
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
$ target/testnet/subsystem-bench --n-cores 10 --cache-misses data-availability-read
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
need to also build an `Overseer`, but that should be easy using the mockups for subsystems in`core::mock`.

### Mocking

Ideally we want to have a single mock implementation for subsystems that can be minimally configured to
be used in different tests. A good example is `runtime-api` which currently only responds to session information
requests based on static data. It can be easily extended to service other requests.
