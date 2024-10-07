.PHONY: node
node:
	RUST_LOG="error,evm=debug,sc_rpc_server=info,runtime::revive=debug" cargo run -p node -- --dev --consensus instant-seal

.PHONY: rpc
rpc:
	RUST_LOG="info,eth-rpc=debug" cargo run --features dev

# Generate the metadata file for the runtime
# Make sure your chain is running before running this command
# Usage: make metadata METADATA_FILE=<chain_name>.scale
CHAIN ?= kitchen_sink
.PHONY: metadata
metadata:
ifndef CHAIN
	$(error CHAIN is not defined. Please specify it when running make, e.g., make metadata CHAIN=<chain_name>)
endif
	@subxt explore --url ws://localhost:9944 pallet Revive constants chainId > /dev/null
	subxt metadata --url ws://localhost:9944 -o $(CHAIN).scale
	touch src/subxt_client.rs

SALT ?= 0
.PHONY: example_deploy
example_deploy:
	RUST_LOG=info cargo run --manifest-path rpc/Cargo.toml --features example --example deploy $(SALT)

.PHONY: example_transfer
example_transfer:
	RUST_LOG=info cargo run --manifest-path rpc/Cargo.toml --features example --example transfer

