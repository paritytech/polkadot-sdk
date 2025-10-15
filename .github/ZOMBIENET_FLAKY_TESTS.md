# Zombienet Flaky Tests

This document explains how to manage flaky or temporarily disabled zombienet tests in the Polkadot SDK repository.

## Overview

The `.github/zombienet-flaky-tests` file contains a list of zombienet tests that are currently disabled due to flakiness or known issues. These tests are automatically skipped during CI runs but are tracked for future re-enabling.

## File Format

Each line in the `zombienet-flaky-tests` file follows this format:

```
<test-job-name>:<issue-number>
```

**Example:**
```
zombienet-polkadot-functional-0014-chunk-fetching-network-compatibility:9980
zombienet-cumulus-0009-elastic_scaling_pov_recovery:8986
```

- **test-job-name**: The exact job name as defined in the zombienet test YAML files
- **issue-number**: GitHub issue number tracking the flaky test (for reference and follow-up)

## How It Works

1. **Test Discovery**: The zombienet workflows read the test definitions from:
   - `.github/zombienet-tests/zombienet_polkadot_tests.yml`
   - `.github/zombienet-tests/zombienet_cumulus_tests.yml`
   - `.github/zombienet-tests/zombienet_substrate_tests.yml`
   - `.github/zombienet-tests/zombienet_parachain-template_tests.yml`

2. **Filtering**: During the preflight job, tests listed in `zombienet-flaky-tests` are filtered out from the test matrix.

3. **Execution**: Only non-flaky tests are executed in the CI pipeline.

## Adding a Flaky Test

If you encounter a flaky test that needs to be temporarily disabled:

1. **Create or find a GitHub issue** tracking the flaky behavior
2. **Add an entry** to `.github/zombienet-flaky-tests`:
   ```
   zombienet-<suite>-<test-name>:<issue-number>
   ```
3. **Commit and push** the change
4. The CI will automatically validate that:
   - The entry follows the correct format
   - The referenced GitHub issue exists
   - (Warning if the issue is closed)
5. The test will be automatically skipped in subsequent CI runs

## Re-enabling a Test

Once a flaky test has been fixed:

1. **Verify the fix** by running the test locally or in a test branch
2. **Remove the entry** from `.github/zombienet-flaky-tests`
3. **Close the associated GitHub issue** (or update it with the fix)
4. **Commit and push** the change
5. The test will be automatically included in subsequent CI runs

## Validation

The `.github/zombienet-flaky-tests` file is automatically validated in CI whenever it's modified. The validation checks:

- **Format**: Each entry must follow the `<test-name>:<issue-number>` format
- **Issue existence**: The referenced GitHub issue must exist in the repository
- **Issue state**: A warning is shown if the referenced issue is closed (suggesting the entry might be outdated)

The validation workflow runs on pull requests that modify:
- `.github/zombienet-flaky-tests`
- `.github/scripts/check-zombienet-flaky-tests.sh`
- `.github/workflows/check-zombienet-flaky-tests.yml`

## Monitoring

- The number of currently disabled tests is displayed in the CI logs during zombienet test runs
- You can view the current list at: [`.github/zombienet-flaky-tests`](./zombienet-flaky-tests)
- Each disabled test should have an associated GitHub issue for tracking
- The validation script can be run locally: `.github/scripts/check-zombienet-flaky-tests.sh .github/zombienet-flaky-tests`
