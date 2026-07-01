# Zombienet Tests

This folder contains zombienet test definitions for CI execution.

## Structure

- **Test definitions**: YAML files defining test matrices (e.g., `zombienet_substrate_tests.yml`)
- **Flaky tests**: Listed in `.github/zombienet-flaky-tests` - tests with non-deterministic behavior
- **Parser**: `.github/scripts/parse-zombienet-tests.py` converts YAML to GitHub Actions matrix

## Benefits

- Easy test maintenance (add/remove tests)
- Efficient flaky test handling
- Pattern-based test execution for debugging

## Manual Workflow Triggering

### Prerequisites

Before using the dispatch script, you must:

1. **Create a branch** with your changes
2. **Create a Pull Request** for that branch
3. **Ensure CI starts building images** - the PR triggers image builds that the `preflight / wait_build_images` step depends on
4. [OPTIONAL] **Wait for image builds to complete** - zombienet tests require these images.
But if we don't wait then the job triggered by the script will wait for images if their building is in progress.

**Important**: When you push new changes to the PR, CI will rebuild the images. Any jobs triggered after the rebuild will use the updated images.

**Image Retention**: CI images have a 1-day retention period by default. For long-term testing (e.g., over weekends) without pushing changes, temporarily extend the retention by updating the `retention-days` value in `.github/workflows/build-publish-images.yml` to the required number of days.

### Usage

The dispatch script triggers GitHub Actions workflows remotely and monitors their execution.

The script should be executed on developer's machine.

Use `.github/scripts/dispatch-zombienet-workflow.sh`:

```bash
Usage: .github/scripts/dispatch-zombienet-workflow.sh -w <workflow-file> -b <branch> [-m max-triggers] [-p test-pattern]
  -w: Workflow file (required)
  -b: Branch name (required)
  -m: Max triggers (optional, default: infinite)
  -p: Test pattern (optional, supports regex)
```

The script automatically creates a CSV file (`workflow_results_YYYYMMDD_HHMMSS.csv`) containing job results with columns: job_id, job_name, conclusion, started_at, branch, job_url.

### Examples

**Run workflow 5 times (respects flaky test exclusions):**
```bash
.github/scripts/dispatch-zombienet-workflow.sh -w zombienet_substrate.yml -b "my-branch" -m 5
```

**Run specific test infinitely (includes flaky tests):**
```bash
.github/scripts/dispatch-zombienet-workflow.sh -w zombienet_substrate.yml -b "my-branch" -p zombienet-substrate-0000-block-building
```

**Run multiple specific tests using regex pattern:**
```bash
.github/scripts/dispatch-zombienet-workflow.sh -w zombienet_cumulus.yml -b "my-branch" -p "zombienet-cumulus-0002-pov_recovery|zombienet-cumulus-0006-rpc_collator_builds_blocks"
```

### Requirements

- Run from `polkadot-sdk` repository root
- Requires `gh` CLI (will prompt for login on first use)

## Flaky Tests

Flaky tests should have corresponding issues in the [Zombienet CI reliability project](https://github.com/orgs/paritytech/projects/216/views/1).
