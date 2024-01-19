module.exports = {
	grandpaPalletName: "bridgeRococoGrandpa",
	parachainsPalletName: "bridgeRococoParachains",
	messagesPalletName: "bridgeRococoMessages",
	bestBridgedRelayChainGrandpaAuthoritySet: async function(api) {
		return await api.query.bridgeRococoGrandpa.currentAuthoritySet();
	},
	bestBridgedParachainInfo: async function(api) {
		return await api.query.bridgeRococoParachains.parasInfo(1013);
	},
}
