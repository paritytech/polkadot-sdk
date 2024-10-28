Chopsticks tests introduce a way of sending real-world API requests to Substrate-based networks, which are built and run locally, on your computer.  
For now, we use it to test new APIs locally, before deploying it to any of the testnets. You can find comprehensive documentation under [this link](https://papermoonio.github.io/polkadot-ecosystem-docs-draft/dev-tools/chopsticks/overview/)

To get started with chopsticks:
1. Make sure you have [Node.js](https://nodejs.org/en/) installed
2. Go through the instructions inside the `wasms` folder.
3. Once you have your WASM BLOBs in place, you can run a node using the following command:
- `npx @acala-network/chopsticks@latest xcm -r configs/westend-override.yaml -p configs/westend-asset-hub-override.yaml`  
This particular command should span up two RPC servers (you can specify as many parachains as needed by chaining `-p configs/another-parachain-config.yaml`) and 
output the ports they reserved for themselves.
4. Once relay (and parachains) are up and running you can proceed to sending RPCs with the help of bun (see `tests` folder).   
