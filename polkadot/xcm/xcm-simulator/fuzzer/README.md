# XCM Simulator Fuzzer

This project will fuzz-test the XCM simulator. It can catch reachable panics, timeouts as well as integer overflows and
underflows.

## Install dependencies

```
cargo install --force ziggy cargo-afl honggfuzz grcov
```

## Run the fuzzer

In this directory, run this command:

```
cargo ziggy fuzz
```

You can use the following options to improve fuzzing:
- `-j number_of_jobs` for fuzzing using multiple threads
- `-G 1024` to limit the size of inputs to 1024 bytes
  - this will improve fuzzing effectiveness, and you can drop the limit once you have good coverage

## Run a single input

In this directory, run this command:

```
cargo ziggy run -i path/to/your/input
```

## Generate coverage

In this directory, run this command:

```
cargo ziggy cover
```

The code coverage will be in `./output/xcm-simulator-fuzzer/coverage/index.html`.
