1. Please, create a separate folder for your decentralized APP or chopstick tests.
2. Use the following commands to install `bun`:
- curl -fsSL https://bun.sh/install | bash
- export BUN_INSTALL="$HOME/.bun"
- export PATH="$BUN_INSTALL/bin:$PATH"
3. Once you have your folder you can run `bun init` inside of it, and it will create a default set-up for you. Click enter twice to choose the default options. 
4. Now you need to add polkadot-api or papi: `bun add polkadot-api`
5. Add your ecosystem (`westend` or `wnd` in this case): 
- `bun papi add wnd -w ws://localhost:8001`
- you can find the port as the output of running `npx` command in the folder above
6. Once it's done you can implement your tests. Feel free to use other's tests as code reference.
7. To run the tests execute `bun test ./index.ts`