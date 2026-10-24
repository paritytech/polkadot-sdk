import { createClient, type ChainDefinition, type SS58String, type TypedApi } from "polkadot-api"
import { getWsProvider } from "polkadot-api/ws-provider/web";
import { setupNetworks, type NetworkContext } from "@acala-network/chopsticks-testing"
import { XcmV3Junctions, XcmV3Junction, XcmVersionedLocation, wnd_penpal, wnd_rc, wnd_ah } from "@polkadot-api/descriptors"
import { Asset, AssetMap } from './data/tokenMap'

export enum AssetState {
	Foreign = "Foreign",
	Local = "Local",
}

export interface MockChain<T extends ChainDefinition> {
	context: NetworkContext,
	api: TypedApi<T>
	setSystemAsset: (accounts: [SS58String, bigint][]) => Promise<void>,
	setAssets: (assets: [SS58String, Asset, AssetState, bigint][]) => Promise<void>,
	getSystemAsset: (who: [SS58String][]) => Promise<bigint[]>,
	getAssets: (assets: [SS58String, Asset, AssetState][]) => Promise<bigint[]>,
	setXcmVersion: (version: number) => Promise<void>,
}

export interface MockNetwork {
	// parachain: MockChain<typeof wnd_penpal>,
	relay: MockChain<typeof wnd_rc>,
	assetHub: MockChain<typeof wnd_ah>,
	paraSovAccOnRelay: SS58String,
	paraSovAccOnAssetHub: SS58String,
}

const setSystemAsset = async (context: NetworkContext, accounts: [SS58String, bigint][]) => {
	const changes: any = {};
	for (const [who, amount] of accounts) {
		changes.System = changes.System ?? {};
		changes.System.Account = changes.System.Account ?? [];
		changes.System.Account.push(
			[[who], { providers: 1, data: { free: amount } }],
		);
	}
	await context.dev.setStorage(changes);
};

export const setup = async (): Promise<MockNetwork> => {
	const { /*parachain, */ polkadot, assetHub } = await setupNetworks({
		// parachain: {
		// 	endpoint: getPenpalEndpoint(),
		// 	'wasm-override': getPenpalWasm(),
		// 	'runtime-log-level': 5,
		// 	db: './db.sqlite',
		// 	// TODO: ports might be moved to .env if needed.
		// 	port: 8006,
		// },
		polkadot: {
			endpoint: getRelayEndpoint(),
			'wasm-override': getRelayWasm(),
			'runtime-log-level': 5,
			db: './db.sqlite',
			port: 8007,
		},
		assetHub: {
			endpoint: getAssetHubEndpoint(),
			'wasm-override': getAssetHubWasm(),
			'runtime-log-level': 5,
			db: './db.sqlite',
			port: 8008,
		},
	});

	// const parachainClient = createClient(getWsProvider(parachain.ws.endpoint));
	// const parachainApi = parachainClient.getTypedApi(wnd_penpal);
	const relayClient = createClient(getWsProvider(polkadot.ws.endpoint));
	const relayApi = relayClient.getTypedApi(wnd_rc);
	const assetHubClient = createClient(getWsProvider(assetHub.ws.endpoint));
	const assetHubApi = assetHubClient.getTypedApi(wnd_ah);

	const paraSovAccOnRelay = (await relayApi
		.apis
		.LocationToAccountApi
		.convert_location(XcmVersionedLocation.V4({
			parents: 0,
			interior: XcmV3Junctions.X1(XcmV3Junction.Parachain(2042))
		}))).value as SS58String;
	const paraSovAccOnAssetHub = (await assetHubApi
		.apis
		.LocationToAccountApi
		.convert_location(XcmVersionedLocation.V4({
			parents: 1,
			interior: XcmV3Junctions.X1(XcmV3Junction.Parachain(2042))
		}))).value as SS58String;

	const assetHubIndexMap: Record<Asset, number> = {
		[Asset.USDT]: 1984,
		[Asset.WND]: 1,
	};
	const assetMap = new AssetMap(assetHubIndexMap);

	return {
		// parachain: {
		// 	context: parachain,
		// 	api: parachainApi,
		// 	setSystemAsset: async (accounts) => setSystemAsset(parachain, accounts),
		// 	setAssets: async (assets) => {
		// 		const changes: any = {};
		//
		// 		for (const [who, what, state, amount] of assets) {
		// 			if (state === AssetState.Local) {
		// 				changes.Assets = changes.Assets ?? {};
		// 				changes.Assets.Asset = changes.Assets.Asset ?? [];
		// 				changes.Assets.Account = changes.Assets.Account ?? [];
		//
		// 				// TODO: detect asset supply increase and override it.
		// 				changes.Assets.Asset.push(
		// 					[[[assetMap.getIndex(what)]], { supply: amount }]
		// 				);
		// 				changes.Assets.Account.push(
		// 					[[assetMap.getIndex(what), who], { balance: amount }],
		// 				);
		// 			} else {
		// 				changes.ForeignAssets = changes.ForeignAssets ?? {};
		// 				changes.ForeignAssets.Asset = changes.ForeignAssets.Asset ?? [];
		// 				changes.ForeignAssets.Account = changes.ForeignAssets.Account ?? [];
		//
		// 				// TODO: detect asset supply increase and override it.
		// 				changes.ForeignAssets.Asset.push(
		// 					[[assetMap.getRelativeRawLocation(what, false)], {supply: amount }]
		// 				);
		// 				changes.ForeignAssets.Account.push(
		// 					[[assetMap.getRelativeRawLocation(what, false), who], { balance: amount }],
		// 				);
		// 			}
		// 		}
		//
		// 		await parachain.dev.setStorage(changes);
		// 	},
		// 	setXcmVersion: async (version: number) => {
		// 		const changes: any = {};
		// 		changes.PolkadotXcm = changes.PolkadotXcm ?? {};
		// 		changes.PolkadotXcm.SafeXcmVersion = changes.PolkadotXcm.SafeXcmVersion ?? 0;
		// 		changes.PolkadotXcm.SafeXcmVersion = 5;
		//
		// 		changes.PolkadotXcm.SupportedVersion = changes.PolkadotXcm.SupportedVersion ?? [];
		// 		changes.PolkadotXcm.SupportedVersion.push(
		// 			[[5, { V5: {parents: 1, interior: { X1: [{Parachain: 1000}]}}}], 5]
		// 		);
		//
		// 		await parachain.dev.setStorage(changes);
		// 	},
		// 	getSystemAsset: async (accounts: [SS58String][]) => {
		// 		const results = [];
		// 		for (const [who] of accounts) {
		// 			results.push((await parachainApi.query.System.Account.getValue(who)).data.free);
		// 		}
		// 		return results;
		// 	},
		// 	getAssets: async(assets: [SS58String, Asset, AssetState][]) => {
		// 		const results = [];
		// 		for (const [who, what, state] of assets) {
		// 			if (state === AssetState.Local) {
		// 				results.push((await parachainApi
		// 					.query
		// 					.Assets
		// 					.Account
		// 					.getValue(assetMap.getIndex(what), who))?.balance ?? 0n
		// 				);
		// 			} else {
		// 				results.push((await parachainApi
		// 					.query
		// 					.ForeignAssets
		// 					.Account
		// 					.getValue(assetMap.getRelativeLocation(what, false), who))?.balance ?? 0n
		// 				);
		// 			}
		// 		}
		// 		return results;
		// 	},
		// },
		relay: {
			context: polkadot,
			api: relayApi,
			setSystemAsset: async (accounts) => setSystemAsset(polkadot, accounts),
			setAssets: async (assets) => {
				throw new Error(`setAssets should not be called for Relay Chain`);
			},
			setXcmVersion: async (version: number) => {
				const changes: any = {};
				changes.XcmPallet = {
					SafeXcmVersion: version,
				}
				await polkadot.dev.setStorage(changes);
			},
			getSystemAsset: async (accounts: [SS58String][]) => {
				const results = [];
				for (const [who] of accounts) {
					results.push((await relayApi.query.System.Account.getValue(who)).data.free);
				}
				return results;
			},
			getAssets: async(assets: [SS58String, Asset, AssetState][]) => {
				throw new Error(`getAssets should not be called for Relay Chain`);
			},
		},
		assetHub: {
			context: assetHub,
			api: assetHubApi,
			setSystemAsset: async (accounts) => setSystemAsset(assetHub, accounts),
			setAssets: async (assets) => {
				const changes: any = {};

				for (const [who, what, state, amount] of assets) {
					if (state === AssetState.Local) {
						changes.Assets = changes.Assets ?? {};
						changes.Assets.Asset = changes.Assets.Asset ?? [];
						changes.Assets.Account = changes.Assets.Account ?? [];

						// TODO: detect asset supply increase and override it.
						changes.Assets.Asset.push(
							[[[assetMap.getIndex(what)]], { supply: amount }]
						);
						changes.Assets.Account.push(
							[[assetMap.getIndex(what), who], { balance: amount }],
						);
					} else {
						changes.ForeignAssets = changes.ForeignAssets ?? {};
						changes.ForeignAssets.Asset = changes.ForeignAssets.Asset ?? [];
						changes.ForeignAssets.Account = changes.ForeignAssets.Account ?? [];

						// TODO: detect asset supply increase and override it.
						changes.ForeignAssets.Asset.push(
							[[assetMap.getRelativeRawLocation(what, true), {supply: amount }]]
						);
						changes.ForeignAssets.Account.push(
							[[assetMap.getRelativeRawLocation(what, true), who], { balance: amount }],
						);
					}
				}

				await assetHub.dev.setStorage(changes);
			},
			// TODO improve setXCMversion to be able to specify [chain: version] map
			setXcmVersion: async (version: number) => {
				const changes: any = {};

				changes.PolkadotXcm = changes.PolkadotXcm ?? {};
				changes.PolkadotXcm.SafeXcmVersion = changes.PolkadotXcm.SafeXcmVersion ?? 0;
				changes.PolkadotXcm.SafeXcmVersion = version;

				changes.PolkadotXcm.SupportedVersion = changes.PolkadotXcm.SupportedVersion ?? [];
				changes.PolkadotXcm.SupportedVersion.push(
					[[version, { V5: {parents: 1, interior: { X1: [{Parachain: 2042}]}}}], version]
				);

				await assetHub.dev.setStorage(changes);
			},
			getSystemAsset: async (accounts: [SS58String][]) => {
				const results = [];
				for (const [who] of accounts) {
					results.push((await assetHubApi.query.System.Account.getValue(who)).data.free);
				}
				return results;
			},
			getAssets: async(assets: [SS58String, Asset, AssetState][]) => {
				const results = [];
				for (const [who, what, state] of assets) {
					if (state === AssetState.Local) {
						results.push((await assetHubApi
							.query
							.Assets
							.Account
							.getValue(assetMap.getIndex(what), who))?.balance ?? 0n
						);
					} else {
						results.push((await assetHubApi
							.query
							.ForeignAssets
							.Account
							.getValue(assetMap.getRelativeLocation(what, true), who))?.balance ?? 0n
						);
					}
				}
				return results;
			},
		},
		paraSovAccOnAssetHub,
		paraSovAccOnRelay,
	};
}

type WebSocketEndpoint = `wss://${string}`;

export const getRelayEndpoint = (): WebSocketEndpoint => {
	switch (process.env.NETWORK) {
		case 'westend':
			return 'wss://polkadot-rpc.dwellir.com';
			// return 'wss://westend-rpc.polkadot.io';
		case 'kusama':
			return 'wss://rpc-kusama.luckyfriday.io';
		case 'polkadot':
			return 'wss://rpc-polkadot.luckyfriday.io';
		default:
			throw 'Set one of the available networks: westend kusama polkadot';
	}
}

export const getAssetHubEndpoint = (): WebSocketEndpoint => {
	switch (process.env.NETWORK) {
		case 'westend':
			return 'wss://polkadot-asset-hub-rpc.polkadot.io';
			// return 'wss://westend-asset-hub-rpc.polkadot.io';
		case 'kusama':
			return 'wss://kusama-asset-hub-rpc.polkadot.io';
		case 'polkadot':
			return 'wss://polkadot-asset-hub-rpc.polkadot.io';
		default:
			throw 'Set one of the available networks: westend kusama polkadot';
	}
}

export const getPenpalEndpoint = () : WebSocketEndpoint => {
	switch (process.env.NETWORK) {
		case 'westend':
			return 'wss://westend-penpal-rpc.polkadot.io';
		default:
			throw 'Only westend network is being supported for now';
	}
}

export const getRelayWasm = (): string => {
	switch (process.env.NETWORK) {
		case 'westend':
			return '../../wasms/polkadot_runtime.compact.compressed.wasm';
			// return process.env.RELAY_WASM;
		default:
			throw 'Set one of the available networks: westend kusama polkadot';
	}
}

export const getAssetHubWasm = (): string => {
	switch (process.env.NETWORK) {
		case 'westend':
			return '../../wasms/asset_hub_polkadot_runtime.compact.compressed.wasm';
		default:
			throw 'Set one of the available networks: westend kusama polkadot';
	}
}

// export const getPenpalWasm = (): string => {
// 	switch (process.env.NETWORK) {
// 		case 'westend':
// 			return '../../wasms/penpal_runtime.compact.compressed.wasm';
// 		default:
// 			throw 'Set one of the available networks: westend kusama polkadot';
// 	}
// }
