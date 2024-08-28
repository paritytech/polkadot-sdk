# Weight Generation

To generate weights for a runtime.
Weights generation is using self-hosted runner which is provided by Parity CI, the rest commands are using standard
GitHub runners on `ubuntu-latest` or `ubuntu-20.04`.
Self-hosted runner for benchmarks (arc-runners-Polkadot-sdk-benchmark) is configured to meet requirements of reference
hardware for running validators
https://wiki.polkadot.network/docs/maintain-guides-how-to-validate-polkadot#reference-hardware

In a PR run the actions through comment:

```sh
/cmd bench --help # outputs the actual usage documentation with examples and supported runtimes

# or

/cmd --help # to see all available commands
```

To regenerate all weights (however it will take long,
so don't do it unless you really need it), run the following command:
```sh
/cmd bench
```

To generate weights for all pallets in a particular runtime(s), run the following command:
```sh
/cmd bench --runtime kusama polkadot
```

For Substrate pallets (supports sub-modules too):
```sh
/cmd bench --runtime dev --pallet pallet_asset_conversion_ops
```

> **📝 Note**: The action is not being run right-away, it will be queued and run in the next available runner.
So might be quick, but might also take up to 10 mins (That's in control of Github).  
Once the action is run, you'll see reaction 👀 on original comment, and if you didn't pass `--quiet` -
it will also send a link to a pipeline when started, and link to whole workflow when finished.

---

> **💡Hint #1** : if you run all runtimes or all pallets, it might be that some pallet in the middle is failed
to generate weights, thus it stops (fails) the whole pipeline.
> If you want, you can make it to continue running, even if some pallets are failed, add `--continue-on-fail`
flag to the command. The report will include which runtimes/pallets have failed, then you can re-run
them separately after all is done.

This way it runs all possible runtimes for the specified pallets, if it finds them in the runtime
```sh
/cmd bench --pallet pallet_balances pallet_xcm_benchmarks::generic pallet_xcm_benchmarks::fungible
```

If you want to run all specific pallet(s) for specific runtime(s), you can do it like this:
```sh
/cmd bench --runtime bridge-hub-polkadot --pallet pallet_xcm_benchmarks::generic pallet_xcm_benchmarks::fungible
```


> **💡Hint #2** : Sometimes when you run too many commands, or they keep failing and you're rerunning them again,
it's handy to add `--clean` flag to the command. This will clean up all yours and bot's comments in PR relevant to
/cmd commands.

```sh
/cmd bench --runtime kusama polkadot --pallet=pallet_balances --clean --continue-on-fail
```

> **💡Hint #3** : If you have questions or need help, feel free to tag @paritytech/opstooling (in github comments)
or ping in [matrix](https://matrix.to/#/#command-bot:parity.io) channel.
