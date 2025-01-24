# v5-sac-test

To install dependencies:

```bash
bun add polkadot-api
# now you need to switch to another terminal window where you executed bunx command, make sure that chains are running and copy Asset Hub's port in the command below (usually the port is 8001).
bun papi add wnd_ah -w ws://localhost:8000
bun papi add wnd_penpal -w ws://localhost:8001
bun add @polkadot-labs/hdkd
```

To run the test:

```bash
# timeout per test. time in ms. during start-up it takes around 11 sec for test to be executed 
bun test ./index.ts --timeout 12000
# note: the test may time out during the first run. Single retry usually helps.
```

This project was created using `bun init` in bun v1.1.34. [Bun](https://bun.sh) is a fast all-in-one JavaScript runtime.
