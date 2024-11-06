# v5-sac-test

To install dependencies:

```bash
bun add polkadot-api
# now you need to switch to another terminal window where you executed bunx command, make sure that chains are running and copy Asset Hub's port in the command below (usually the port is 8001).
bun papi add wnd_ah -w ws://localhost:8001
bun add @polkadot-labs/hdkd
```

To run the test:

```bash
bun test ./index.ts
```

This project was created using `bun init` in bun v1.1.34. [Bun](https://bun.sh) is a fast all-in-one JavaScript runtime.
