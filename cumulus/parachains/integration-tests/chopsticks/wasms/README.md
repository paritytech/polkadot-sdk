This folder contains WASM BLOBs which are supposed to be used by chopstick tests.  
To populate this folder with the required files you need to:
1. BUILD relevant runtimes: usually relay and parachain(s).
- `cargo build --release -p westend-runtime `
- `cargo build --release -p asset-hub-westend-runtime `
2. Copy compressed wasms from target folder into the current one:
- `WBUILD_PATH=../../../../../target/release/wbuild`
- `cp $WBUILD_PATH/asset-hub-westend-runtime/asset_hub_westend_runtime.compact.compressed.wasm .`
- `cp $WBUILD_PATH/westend-runtime/westend_runtime.compact.compressed.wasm .`
