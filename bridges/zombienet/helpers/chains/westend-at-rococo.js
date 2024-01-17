module.exports = {
	grandpaPalletName: "bridgeWestendGrandpa",
	parachainsPalletName: "bridgeWestendParachains",
	bestBridgedRelayChainGrandpaAuthoritySet: async function(api) {
		return await api.query.bridgeWestendGrandpa.currentAuthoritySet();
	},
	bestBridgedParachainInfo: async function(api) {
		return await api.query.bridgeWestendParachains.parasInfo(1002);
	},
}
