# Running commands

Command bot has been migrated, it is no longer a comment parser and now it is a GitHub action that works as a [`workflow_dispatch`](https://docs.github.com/en/actions/using-workflows/events-that-trigger-workflows#workflow_dispatch) event.

## How to run an action

To run an action, you need to go to the [_actions tab_](https://github.com/paritytech/polkadot-sdk/actions) and pick the one you desire to run.

The current available command actions are:

- [Command FMT](https://github.com/paritytech/polkadot-sdk/actions/workflows/command-fmt.yml)
- [Command Update UI](https://github.com/paritytech/polkadot-sdk/actions/workflows/command-update-ui.yml)
- [Command Sync](https://github.com/paritytech/polkadot-sdk/actions/workflows/command-sync.yml)
- [Command Bench](https://github.com/paritytech/polkadot-sdk/actions/workflows/command-bench.yml)
- [Command Bench All](https://github.com/paritytech/polkadot-sdk/actions/workflows/command-bench-all.yml)

**WIP**: Need more actions.

You need to select the action, and click on the dropdown that says: `Run workflow`. It is located in the upper right.

If this dropdown is not visible, you may not have permission to run the action. Contact IT for help.

![command screenshot](command-screnshot.png)

Each command will have the same two required values, but it could have more.

GitHub's official documentation: [Manually running a workflow](https://docs.github.com/en/actions/using-workflows/manually-running-a-workflow)

### Number of the Pull Request

The number of the pull request. Required so the action can fetch the correct branch and comment if it fails.

## Action configurations

### Bench-all

This is a wrapper to run `bench` for all pallets.

Posible combinations based on the `benchmark` dropdown.

- `pallet`: Benchmark for Substrate/Polkadot/Cumulus/Trappist for specific pallet
  - Requires field `Pallet` to have an input that applies to `^([a-z_]+)([:]{2}[a-z_]+)?$`
- `substrate`: Pallet + Overhead + Machine Benchmark for Substrate for all pallets
  - Requires `Target Directory` to be `substrate`
- `polkadot`: Pallet + Overhead Benchmark for Polkadot
  - Requires `Runtime` to be one of the following:
    - `rococo`
    - `westend`
  - Requires `Target Directory` to be `polkadot`
- `cumulus`: Pallet Benchmark for Cumulus
  - Requires `Runtime` to be one of the following:
    - `rococo`
    - `westend`
    - `asset-hub-kusama`
    - `asset-hub-polkadot`
    - `asset-hub-rococo`
    - `asset-hub-westend`
    - `bridge-hub-kusama`
    - `bridge-hub-polkadot`
    - `bridge-hub-rococo`
    - `bridge-hub-westend`
    - `collectives-polkadot`
    - `collectives-westend`
    - `coretime-rococo`
    - `coretime-westend`
    - `contracts-rococo`
    - `glutton-kusama`
    - `glutton-westend`
    - `people-rococo`
    - `people-westend`
  - Requires `Target Directory` to be `cumulus`

## How to modify an action

If you want to modify an action and test it, you can do by simply pushing your changes and then selecting your branch in the `Use worflow from` option.

This will use a file from a specified branch.
