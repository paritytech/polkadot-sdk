1. Please, create a separate folder for your decentralized APP or chopstick tests.
2. TODO: add the documentation how to install bun
- curl -fsSL https://bun.sh/install | bash
- export BUN_INSTALL="$HOME/.bun"
- export PATH="$BUN_INSTALL/bin:$PATH"
2. Once you have your folder you can run `bun init` and it will create a default set-up for running a bun node.
3. Now you need to add polkadot-api or papi: `bun add polkadot-api`
4. Add your ecosystem (`westend` or `wnd` in this case): 
- `bun papi add wnd -w ws://localhost:8001`
- you can find the port as the output of running `npx` command in the folder above
- TODO: understand what exactly `npx` command does and then refactor this guide
5. Once it's done you can implement your tests. Feel free to use other's tests as code reference.
6. To run the tests execute `bun test ./index.ts`