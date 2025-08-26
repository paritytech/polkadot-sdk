# v5-sac-test

To install dependencies:

```bash
bun add polkadot-api
bun add @polkadot-labs/hdkd
```

To run the test:

```bash
# timeout per test. time in ms. during start-up it takes around 11 sec for test to be executed
bun test ./index.ts --timeout 12000
# note: the test may time out during the first run. Single retry usually helps.
```

This project was created using `bun init` in bun v1.1.34. [Bun](https://bun.sh) is a fast all-in-one JavaScript runtime.
