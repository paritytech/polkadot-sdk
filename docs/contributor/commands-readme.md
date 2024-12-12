# Running Commands in PRs

You can run commands in PRs by triggering it via comment. It will use the context of your PR and post the results back.
Note: it works only for members of the `paritytech` organization.

## Usage

`/cmd --help` to see all available commands and usage format

`/cmd <command> --help` to see the usage of a specific command

### Commands

- `/cmd fmt` to format the code in the PR. It commits back with the formatted code (fmt) and configs (taplo).

- `/cmd bench` to generate weights for a runtime. Read more about [Weight Generation](weight-generation.md)

- `/cmd prdoc` to generate a prdoc for a PR. Read more about [PRDoc](prdoc.md)

### Flags

1.`--quiet` to suppress the output of the command in the comments.
By default, the Start and End/Failure of the command will be commented with the link to a pipeline.
If you want to avoid, use this flag. Go to
[Action Tab](https://github.com/paritytech/polkadot-sdk/actions/workflows/cmd.yml) to see the pipeline status.

3.`--clean` to clean up all yours and bot's comments in PR relevant to `/cmd` commands. If you run too many commands,
or they keep failing, and you're rerunning them again, it's handy to add this flag to keep a PR clean.

### Adding new Commands

Feel free to add new commands to the workflow, however **_note_** that triggered workflows will use the actions
from `main` (default) branch, meaning they will take effect only after the PR with new changes/command is merged.
If you want to test the new command, it's better to test in your fork and local-to-fork PRs, where you control
the default branch.

### Examples

The regex in cmd.yml is: `^(\/cmd )([-\/\s\w.=:]+)$` accepts only alphanumeric, space, "-", "/", "=", ":", "." chars.

`/cmd bench --runtime bridge-hub-westend --pallet=pallet_name`
`/cmd prdoc --audience runtime_dev runtime_user --bump patch --force`
`/cmd update-ui --image=docker.io/paritytech/ci-unified:bullseye-1.77.0-2024-04-10-v202407161507 --clean`
