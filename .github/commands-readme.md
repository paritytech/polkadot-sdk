# Running commands

Command bot has been migrated, it is no longer a comment parser and now it is a GitHub action that works as a [`workflow_dispatch`](https://docs.github.com/en/actions/using-workflows/events-that-trigger-workflows#workflow_dispatch) event.

## How to run an action

To run an action, you need to go to the [_actions tab_](https://github.com/paritytech/polkadot-sdk/actions) and pick the one you desire to run.

The current available command actions are:

- [Command FMT](https://github.com/paritytech/polkadot-sdk/actions/workflows/command-fmt.yml)

**WIP**: Need more actions.

You need to select the action, and click on the dropdown that says: `Run workflow`. It is located in the upper right.

If this dropdown is not visible, you may not have permission to run the action. Contact IT for help.

![command screenshot](command-screnshot.png)

Each command will have the same two required values, but it could have more.

GitHub's official documentation: [Manually running a workflow](https://docs.github.com/en/actions/using-workflows/manually-running-a-workflow)

### Number of the Pull Request

The number of the pull request. Required so the action can fetch the repo.

## How to modify an action

If you want to modify an action and test it, you can do by simply pushing your changes and then selecting your branch in the `Use worflow from` option.

This will use a file from a specified branch.
