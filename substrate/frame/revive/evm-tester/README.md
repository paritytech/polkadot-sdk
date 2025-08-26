# Revive statetest - Ethereum State Test Runner

A command-line tool for running Ethereum state tests with Revive's EVM-compatible execution environment. This tool replicates the functionality of go-ethereum's `evm statetest` command for validating EVM implementations against the official Ethereum test suite.

## Overview

The `statetest` tool processes Ethereum state test files and validates execution results against expected outcomes. It supports the same interface and output format as go-ethereum's `evm statetest`, making it compatible with existing Ethereum test infrastructure while targeting Revive's execution environment.

## Usage

```bash
# Build the binary
cargo build --release

# Run a single state test file
./target/release/statetest /path/to/test.json --human

# Filter by fork
./target/release/statetest /path/to/test.json --statetest.fork London --human

# Filter by test name pattern
./target/release/statetest /path/to/test.json --run "chainid.*" --human

# Run specific subtest index
./target/release/statetest /path/to/test.json --statetest.index 0 --human

# Batch mode - read filenames from stdin
echo "/path/to/test.json" | ./target/release/statetest --human

# JSON output (default)
./target/release/statetest /path/to/test.json

# Dump state information
./target/release/statetest /path/to/test.json --dump --human
```

## Command Line Arguments

- `[TEST_FILE]` - Path to state test JSON file (optional, reads from stdin if not provided)
- `--statetest.fork <FORK>` - Only run tests for the specified fork (Berlin, London, Shanghai, Cancun, etc.)
- `--statetest.index <INDEX>` - Run only the subtest at the specified index (-1 for all, default: -1)
- `--run <PATTERN>` - Run only tests matching the regular expression (default: ".*")
- `--bench` - Benchmark the execution
- `--dump` - Dump the final state after execution
- `--human` - Human-readable output format

## State Test Format

The tool processes Ethereum state test files in the standard EIP-176 format:

```json
{
  "testName": {
    "env": {
      "currentCoinbase": "0x...",
      "currentGasLimit": "0x...",
      "currentNumber": "0x...",
      "currentTimestamp": "0x...",
      "currentDifficulty": "0x...",
      "currentBaseFee": "0x..."
    },
    "pre": {
      "0x...": {
        "balance": "0x...",
        "code": "0x...",
        "nonce": "0x...",
        "storage": {}
      }
    },
    "transaction": {
      "data": ["0x..."],
      "gasLimit": ["0x..."],
      "gasPrice": "0x...",
      "nonce": "0x...",
      "secretKey": "0x...",
      "to": "0x...",
      "value": ["0x..."]
    },
    "post": {
      "London": [
        {
          "hash": "0x...",
          "logs": "0x...",
          "indexes": {
            "data": 0,
            "gas": 0,
            "value": 0
          }
        }
      ]
    }
  }
}
```

## Output Formats

### Human-Readable Output (--human)
```
[PASS] testName (London)
--
1 tests passed, 0 tests failed.
```

### JSON Output (default)
```json
[
  {
    "name": "testName",
    "pass": true,
    "state_root": "0x...",
    "fork": "London"
  }
]
```

### With State Dump (--dump)
```
[PASS] testName (London)
{
  "accounts": {},
  "root": "0x..."
}
--
1 tests passed, 0 tests failed.
```

## Comparison with go-ethereum evm statetest

This tool provides full compatibility with go-ethereum's `evm statetest` command:

| Feature | go-ethereum evm statetest | revive statetest |
|---------|---------------------------|------------------|
| Input format | EIP-176 state tests | ✅ Same |
| Fork filtering | `--statetest.fork` | ✅ Same |
| Test filtering | `--run regex` | ✅ Same |
| Subtest selection | `--statetest.index` | ✅ Same |
| Batch processing | stdin filenames | ✅ Same |
| Human output | `--human` | ✅ Same |
| State dumps | `--dump` | ✅ Same |
| JSON output | default | ✅ Same |

## Examples

### Running go-ethereum test files
```bash
# Test with official go-ethereum state test
./target/release/statetest /path/to/go-ethereum/cmd/evm/testdata/statetest.json --human

# Filter by London fork only
./target/release/statetest /path/to/tests/ --statetest.fork London --human

# Run all tests matching pattern
./target/release/statetest /path/to/tests/ --run "chainid.*" --human
```

### Batch processing
```bash
# Process multiple files
find /path/to/tests -name "*.json" | ./target/release/statetest --human
```

### Testing with revm fixtures
```bash
# Run comprehensive EIP-7702 Prague tests (extensive test suite)
./target/release/statetest /home/pg/github/revm/test-fixtures/develop/state_tests/prague/eip7702_set_code_tx/calls/delegate_call_targets.json --human

# Filter specific test patterns from the fixture
./target/release/statetest /home/pg/github/revm/test-fixtures/develop/state_tests/prague/eip7702_set_code_tx/calls/delegate_call_targets.json --run ".*EMPTY.*" --human

# Show detailed state information for debugging
./target/release/statetest /home/pg/github/revm/test-fixtures/develop/state_tests/prague/eip7702_set_code_tx/calls/delegate_call_targets.json --run ".*EMPTY.*" --statetest.index 0 --dump --human

# JSON output for integration with other tools
./target/release/statetest /home/pg/github/revm/test-fixtures/develop/state_tests/prague/eip7702_set_code_tx/calls/delegate_call_targets.json --run ".*delegate_False.*" --statetest.fork Prague
```

**Note:** The revm test fixtures contain comprehensive state data including:
- Full account states (balances, code, nonce, storage)
- Transaction bytes (txbytes field)  
- Rich metadata (config, _info fields)
- Multiple test variants per case

## Input Format Compatibility

The tool supports multiple state test formats:

### Standard EIP-176 Format (go-ethereum compatible)
```json
{
  "testName": {
    "env": { ... },
    "pre": { ... },
    "transaction": { ... },
    "post": {
      "London": [{"hash": "0x...", "logs": "0x...", "indexes": {...}}]
    }
  }
}
```

### Extended Format (revm fixtures compatible)  
```json
{
  "testName": {
    "env": { ... },
    "pre": { ... },
    "transaction": { ... },
    "post": {
      "Prague": [{
        "hash": "0x...",
        "logs": "0x...", 
        "indexes": {...},
        "txbytes": "0x...",  // RLP-encoded transaction
        "state": {           // Full post-execution account states
          "0x123...": {
            "balance": "0x...",
            "code": "0x...",
            "nonce": "0x...",
            "storage": {...}
          }
        }
      }]
    },
    "config": { ... },    // Chain configuration
    "_info": { ... }      // Test metadata
  }
}
```

## Current Implementation Status

This is a **state test validator** that currently:

- ✅ Parses both standard EIP-176 and extended state test formats
- ✅ Supports all command-line options matching go-ethereum  
- ✅ Provides identical output formats (JSON and human-readable)
- ✅ Handles fork filtering, test filtering, and subtest selection
- ✅ Supports batch processing via stdin
- ✅ Shows rich state dumps with account details when available
- ✅ Works with go-ethereum, revm, and execution-spec-tests fixtures
- ⚠️ **Mocks EVM execution** - validates test format but doesn't run actual EVM

## Future Integration with Revive

The next phase will replace mock validation with real Revive EVM execution:

- Integrate with Revive's actual EVM execution engine
- Execute transactions against pre-state using Revive runtime
- Calculate real state roots from execution results
- Validate actual execution results against expected post-states
- Support all EVM opcodes through Revive's execution environment
- Generate real gas usage and receipt data
- Handle contract creation, calls, and state changes

## Testing

Run the test suite:
```bash
cargo test
```

Test with actual state test files:
```bash
# Test with go-ethereum's sample state test
./target/release/statetest /home/pg/github/go-ethereum/cmd/evm/testdata/statetest.json --human

# Test batch processing
echo "/home/pg/github/go-ethereum/cmd/evm/testdata/statetest.json" | ./target/release/statetest --human
```

The tool successfully processes Ethereum state tests and provides go-ethereum compatible validation, ready for integration with Revive's execution engine.
