## Westend Migration Tests

How to run the Rust migration test for Westend.

Make sure you have the prerequisite tools installed:
- just (https://github.com/casey/just)
- curl
- lz4
- zepter (https://github.com/ggwpez/zepter)

You need to have the `polkadot-sdk` and the `runtimes` repo both checked out. Polkadot-SDK must be on
branch `donal-ahm`. Then go into the runtimes repo, navigate to the correct subfolder and run the
Just command.

You only need to modify the first argument to where you have checked out the `polkadot-sdk`.

⚠️ Run this only after you committed all you work.

```bash
cd integration-tests/ahm
just port westend /home/user/polkadot-sdk cumulus/test/ahm
```
You should see a lot of console output and eventually the Rust test running:

```pre
... lots of output ...
test tests::pallet_migration_works has been running for over 60 seconds
```

It should take about 3 minutes to run. If you want to run the test again, please undo all changes in the
SDK. There is currently no command for this, but I normally use `git add --all && git stash push && git stash drop`.
