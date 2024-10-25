Chopsticks tests introduce a way of sending real-world API requests to Substrate-based networks, which are built and run on your local computer.
Imagine you're adding a new API and want to make sure that it works as intended before deploying it to any of the testnets, chopsticks is the way to go.
TODO: improve introduction before proceeding to the set of commands. 

1. Before executing any of the commands below, please, read README at the `wasms` folder.  
2. Once you have your WASM BLOBs in place, you can run a node using the following command:
- `npx @acala-network/chopsticks@latest xcm -r configs/westend-override.yaml -p configs/westend-asset-hub-override.yaml`  
This particular command should span up two RPC servers (you can specify as many parachains as needed by chaining `-p configs/another-parachain-config.yaml`) and 
output the ports they reserved for themselves.
3. Once relay (and parachains) are up and running you can proceed to sending RPCs with the help of bun.   
