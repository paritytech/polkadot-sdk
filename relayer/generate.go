//go:generate bash -c "jq .abi ../contracts/out/BeefyClient.sol/BeefyClient.json | abigen --abi - --type BeefyClient --pkg contracts --out contracts/beefy_client.go"
//go:generate bash -c "jq .abi ../contracts/out/IGateway.sol/IGateway.json | abigen --abi - --type Gateway --pkg contracts --out contracts/gateway.go"

package main
