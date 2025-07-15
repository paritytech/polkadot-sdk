# Zombienet Tests

This folder contains zombienet test definitions for CI execution.

## Structure

- **Test definitions**: YAML files defining test matrices (e.g., `zombienet_parachain-template_tests.yml`)
- **Flaky tests**: Listed in `.github/zombienet-flaky-tests` - tests with non-deterministic behavior
- **Parser**: `.github/scripts/parse-zombienet-tests.py` converts YAML to GitHub Actions matrix

## Benefits

- Easy test maintenance (add/remove tests)
- Efficient flaky test handling
- Pattern-based test execution for debugging

## Manual Workflow Triggering

Use `.github/scripts/dispatch-zombienet-workflow.sh`:

```bash
Usage: .github/scripts/dispatch-zombienet-workflow.sh -w <workflow-file> -b <branch> [-m max-triggers] [-p test-pattern]
  -w: Workflow file (required)
  -b: Branch name (required)
  -m: Max triggers (optional, default: infinite)
  -p: Test pattern (optional)
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

### Requirements

- Branch must have corresponding PR with CI enabled (for required images)
- Run from `polkadot-sdk` repository root
- Requires `gh` CLI (will prompt for login on first use)

## Flaky Tests

Flaky tests should have corresponding issues in the [Zombienet CI reliability project](https://github.com/orgs/paritytech/projects/216/views/1).
