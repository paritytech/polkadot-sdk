// import { createClient } from "polkadot-api";
import { createClient, type ChainDefinition, type SS58String, type TypedApi } from "polkadot-api"
import { withPolkadotSdkCompat } from "polkadot-api/polkadot-sdk-compat";
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
	// who, what, amount.
	setSystemAsset: (accounts: [SS58String, bigint][]) => Promise<void>,
	setAssets: (assets: [SS58String, Asset, AssetState, bigint][]) => Promise<void>,
	// who, what.
	getSystemAsset: (who: [SS58String][]) => Promise<bigint[]>,
	// boolean = isForeign
	getAssets: (assets: [SS58String, Asset, AssetState][]) => Promise<bigint[]>,
	setXcmVersion: (version: number) => Promise<void>,
}

export interface MockNetwork {
	parachain: MockChain<typeof wnd_penpal>,
	relay: MockChain<typeof wnd_rc>,
	assetHub: MockChain<typeof wnd_ah>,
	paraSovAccOnRelay: SS58String,
	paraSovAccOnAssetHub: SS58String,
}

export const setup = async (): Promise<MockNetwork> => {
	const { parachain, polkadot, assetHub } = await setupNetworks({
		parachain: {
			endpoint: getPenpalEndpoint(),
			'wasm-override': getPenpalWasm(),
			'runtime-log-level': 5,
			db: './db.sqlite',
			 // todo should I specify the port inside .env or hardcoding is fine?
			port: 8006,
		},
		polkadot: {
			endpoint: getRelayEndpoint(),
			'wasm-override': getRelayWasm(),
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

	const parachainClient = createClient(getWsProvider(parachain.ws.endpoint));
	// todo wnd_penpal and other descriptors should be generated somehow automatically. Figure out a way. maybe
	// todo likely from .papi/polkadot-api.json
	const parachainApi = parachainClient.getTypedApi(wnd_penpal);
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
		[Asset.WND]: 2,
	};
	const assetMap = new AssetMap(assetHubIndexMap);

	return {
		parachain: {
			context: parachain,
			api: parachainApi,
			setSystemAsset: async (accounts) => {
				const changes: any = {};
				for (const [who,  amount] of accounts) {
					changes.System = changes.System ?? {};
					changes.System.Account = changes.System.Account ?? [];
					changes.System.Account.push(
						[[who], { providers: 1, data: { free: amount } }],
					);
				}
				await parachain.dev.setStorage(changes);
			},
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
						console.log('who, what, state, amount:', who, what, state, amount)
						changes.ForeignAssets = changes.ForeignAssets ?? {};
						changes.ForeignAssets.Asset = changes.ForeignAssets.Asset ?? [];
						changes.ForeignAssets.Account = changes.ForeignAssets.Account ?? [];

						// TODO: detect asset supply increase and override it.
						changes.ForeignAssets.Asset.push(
							[[assetMap.getRelativeRawLocation(what, false)], {supply: amount }]
							// [[{ parents: 1, interior: 'Here' }], { supply: amount + 2n }],
						);
						changes.ForeignAssets.Account.push(
							[[assetMap.getRelativeRawLocation(what, false), who], { balance: amount }],
						);
						// changes.ForeignAssets.Asset.push(
						// 	[[{
						// 		parents: 1,
						// 		interior: {
						// 			X3: [
						// 				{Parachain: 1000},
						// 				{PalletInstance: 50},
						// 				{GeneralIndex: 1984n},
						// 			],
						// 		},
						// 	}],
						// 		{ supply: amount + 1n }],
						// );
						// changes.ForeignAssets = {
						// 	Asset: [
						// 		// [[{
						// 		// 	parents: 1,
						// 		// 	interior: {
						// 		// 		X3: [
						// 		// 			{Parachain: 1000},
						// 		// 			{PalletInstance: 50},
						// 		// 			{GeneralIndex: 1984n},
						// 		// 		],
						// 		// 	},
						// 		// }],
						// 		// 	{ supply: amount + 1n }],
						// 		[[{ parents: 1, interior: 'Here' }], { supply: amount + 2n }],
						// 	],
						// 	Account: [
						// 		// [[{ parents: 1, interior:
						// 		// 		{
						// 		// 		X3: [
						// 		// 			{Parachain: 1000},
						// 		// 			{PalletInstance: 50},
						// 		// 			{GeneralIndex: 1984n},
						// 		// 		],
						// 		// 	},
						// 		//
						// 		// }, who], { balance: amount + 1n }],
						// 		[[{ parents: 1, interior: 'Here' }, who], { balance: amount + 2n }],
						// 	],
						// };

						// changes.ForeignAssets.Account.push(
						// 	[[{ parents: 1, interior:
						// 			{
						// 			X3: [
						// 				{Parachain: 1000},
						// 				{PalletInstance: 50},
						// 				{GeneralIndex: 1984n},
						// 			],
						// 		},
						//
						// 	}, who], { balance: amount + 1n }],
						// );
					}
				}
				console.log('changes before setting storage: ', changes);
				await parachain.dev.setStorage(changes);
			},
			setXcmVersion: async (version: number) => {
				const changes: any = {};
				changes.PolkadotXcm = changes.PolkadotXcm ?? {};
				changes.PolkadotXcm.SafeXcmVersion = changes.PolkadotXcm.SafeXcmVersion ?? 0;
				changes.PolkadotXcm.SafeXcmVersion = 5;

				changes.PolkadotXcm.SupportedVersion = changes.PolkadotXcm.SupportedVersion ?? [];
				changes.PolkadotXcm.SupportedVersion.push(
					[[5, { V5: {parents: 1, interior: { X1: [{Parachain: 1000}]}}}], 5]
				);

				await parachain.dev.setStorage(changes);
			},
			getSystemAsset: async (accounts: [SS58String][]) => {
				const results = [];
				for (const [who] of accounts) {
					results.push((await parachainApi.query.System.Account.getValue(who)).data.free);
				}
				return results;
			},
			getAssets: async(assets: [SS58String, Asset, AssetState][]) => {
				const results = [];
				for (const [who, what, state] of assets) {
					if (state === AssetState.Local) {
						results.push((await parachainApi
							.query
							.Assets
							.Account
							.getValue(assetMap.getIndex(what), who))?.balance ?? 0n
						);
					} else {
						results.push((await parachainApi
							.query
							.ForeignAssets
							.Account
							.getValue(assetMap.getRelativeLocation(what, false), who))?.balance ?? 0n
						);
					}
				}
				return results;
			},
			// getTokens: async (tokens) => {
			// 	const results = [];
			// 	for (const [who, what] of tokens) {
			// 		if (what === 'Para') {
			// 			results.push((await parachainApi.query.System.Account.getValue(who)).data.free);
			// 		} else if (what === 'Relay') {
			// 			results.push((await parachainApi
			// 				.query
			// 				.ForeignAssets
			// 				.Account
			// 				.getValue({
			// 					parents: 1, interior: XcmV3Junctions.Here()
			// 				}, who))?.balance ?? 0n
			// 			);
			// 		} else if (what == 'USDT') {
			// 			results.push((await parachainApi
			// 				.query
			// 				.ForeignAssets
			// 				.Account
			// 				.getValue({
			// 					parents: 1, interior: XcmV3Junctions.X3([
			// 						XcmV3Junction.Parachain(1000),
			// 						XcmV3Junction.PalletInstance(50),
			// 						XcmV3Junction.GeneralIndex(1984n),
			// 					])}, who))?.balance ?? 0n
			// 			);
			// 		}
			// 	}
			// 	return results;
			// },
			// setTokens: async (tokens) => {
			// 	const changes: any = {};
			// 	for (const [who, what, amount] of tokens) {
			// 		if (what === 'Para') {
			// 			changes.System = {
			// 				Account: [
			// 					[[who], { providers: 1, data: { free: amount } }],
			// 				],
			// 			};
			// 		} else if (what === 'Relay') {
			// 			// changes.ForeignAssets = changes.ForeignAssets ?? {};
			// 			// changes.ForeignAssets.Asset = changes.ForeignAssets.Asset ?? [];
			// 			// changes.ForeignAssets.Account = changes.ForeignAssets.Account ?? [];
			// 			//
			// 			// changes.ForeignAssets.Asset.push();
			// 			//
			// 			// changes.Assets = changes.Assets ?? {};
			// 			// changes.Assets.Asset = changes.Assets.Asset ?? [[[[1984]], { supply: amount }],];
			// 			// changes.Assets.Account = changes.Assets.Account ?? [];
			// 			// changes.Assets.Account.push(
			// 			// 	[[1984, who], { balance: amount }],
			// 			// );
			//
			// 			// todo: split setTOkens into setSystem and SetForeginAsset,
			// 			// todo: make a map of tokens to use in set and get,
			// 			changes.ForeignAssets = {
			// 				Asset: [
			// 					[[{
			// 						parents: 1,
			// 						interior: {
			// 							X3: [
			// 								{Parachain: 1000},
			// 								{PalletInstance: 50},
			// 								{GeneralIndex: 1984n},
			// 							],
			// 						},
			// 					}],
			// 						{ supply: amount + 1n }],
			// 					[[{ parents: 1, interior: 'Here' }], { supply: amount + 2n }],
			// 				],
			// 				Account: [
			// 					[[{ parents: 1, interior:
			// 							{
			// 							X3: [
			// 								{Parachain: 1000},
			// 								{PalletInstance: 50},
			// 								{GeneralIndex: 1984n},
			// 							],
			// 						},
			//
			// 					}, who], { balance: amount + 1n }],
			// 					[[{ parents: 1, interior: 'Here' }, who], { balance: amount + 2n }],
			// 				],
			// 			};
			// 		}
			// 	}
			// 	console.log('setting tokens for Penpal');
			// 	await parachain.dev.setStorage(changes);
			// },
		},
		relay: {
			context: polkadot,
			api: relayApi,
			setSystemAsset: async (accounts) => {
				const changes: any = {};
				for (const [who,  amount] of accounts) {
					changes.System = changes.System ?? {};
					changes.System.Account = changes.System.Account ?? [];
					changes.System.Account.push(
						[[who], { providers: 1, data: { free: amount } }],
					);
				}
				await polkadot.dev.setStorage(changes);
			},
			setAssets: async (assets) => {
				const changes: any = {};

				// TODO: unimplemented for relay
				// for (const [who, what, state, amount] of assets) {
				// 	if (state === AssetState.Local) {
				// 		changes.Assets = changes.Assets ?? {};
				// 		changes.Assets.Asset = changes.Assets.Asset ?? [];
				// 		changes.Assets.Account = changes.Assets.Account ?? [];
				//
				// 		// TODO: detect asset supply increase and override it.
				// 		changes.Assets.Asset.push(
				// 			[[[[assetMap.getIndex(what)]], { supply: amount }]]
				// 		);
				// 		changes.Assets.Account.push(
				// 			[[assetMap.getIndex(what), who], { balance: amount }],
				// 		);
				// 	} else {
				// 		changes.ForeignAssets = changes.ForeignAssets ?? {};
				// 		changes.ForeignAssets.Asset = changes.ForeignAssets.Asset ?? [];
				// 		changes.ForeignAssets.Account = changes.ForeignAssets.Account ?? [];
				//
				// 		// TODO: detect asset supply increase and override it.
				// 		changes.ForeignAssets.Asset.push(
				// 			[[assetMap.getRelativeLocation(what, true), {supply: amount }]]
				// 		);
				// 		changes.ForeignAssets.Account.push(
				// 			[[assetMap.getRelativeLocation(what, true), who], { balance: amount }],
				// 		);
				// 	}
				// }

				await polkadot.dev.setStorage(changes);
			},

			// setTokens: async (tokens) => {
			// 	const changes: any = {};
			// 	for (const [who, what, amount] of tokens) {
			// 		if (what === 'Relay') {
			// 			changes.System = changes.System ?? {};
			// 			changes.System.Account = changes.System.Account ?? [];
			// 			changes.System.Account.push(
			// 				[[who], { providers: 1, data: { free: amount } }],
			// 			);
			// 		}
			// 	}
			// 	await polkadot.dev.setStorage(changes);
			// },
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
				// unimplemented for Relay
				const results = [];
				// for (const [who, what, state, isRelay] of assets) {
				// 	if (state === AssetState.Local) {
				// 		results.push((await relayApi
				// 			.query
				// 			.Assets
				// 			.Account
				// 			.getValue(assetMap.getIndex(what), who))?.balance ?? 0n
				// 		);
				// 	} else {
				// 		results.push((await relayApi
				// 			.query
				// 			.ForeignAssets
				// 			.Account
				// 			.getValue(assetMap.getRelativeLocation(what, isRelay), who))?.balance ?? 0n
				// 		);
				// 	}
				// }
				return results;
			},

			// getTokens: async (tokens) => {
			// 	const results = [];
			// 	for (const [who, what] of tokens) {
			// 		if (what === 'Relay') {
			// 			results.push((await relayApi
			// 				.query
			// 				.System
			// 				.Account
			// 				.getValue(who)).data.free
			// 			);
			// 		}
			// 	}
			// 	return results;
			// },
		},
		assetHub: {
			context: assetHub,
			api: assetHubApi,
			setSystemAsset: async (accounts) => {
				const changes: any = {};
				for (const [who,  amount] of accounts) {
					changes.System = changes.System ?? {};
					changes.System.Account = changes.System.Account ?? [];
					changes.System.Account.push(
						[[who], { providers: 1, data: { free: amount } }],
					);
				}
				await assetHub.dev.setStorage(changes);
			},
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
			// setTokens: async (tokens) => {
			// 	const changes: any = {};
			// 	for (const [who, what, amount] of tokens) {
			// 		if (what === 'Relay') {
			// 			changes.System = changes.System ?? {};
			// 			changes.System.Account = changes.System.Account ?? [];
			// 			changes.System.Account.push(
			// 				[[who], { providers: 1, data: { free: amount } }],
			// 			);
			// 		} else if (what == 'USDT') {
			// 			changes.Assets = changes.Assets ?? {};
			// 			changes.Assets.Asset = changes.Assets.Asset ?? [[[[1984]], { supply: amount }],];
			// 			changes.Assets.Account = changes.Assets.Account ?? [];
			// 			changes.Assets.Account.push(
			// 				[[1984, who], { balance: amount }],
			// 			);
			// 			// changes.Assets = {
			// 			// 	Asset: [
			// 			// 		[[[what]], { supply: amount }],
			// 			// 	],
			// 			// 	Account: [
			// 			// 		[[what, who], { balance: amount }],
			// 			// 	],
			// 			// };
			// 		}
			// 	}
			// 	await assetHub.dev.setStorage(changes);
			// },
			// todo improve setXCMversion to be able to specify [chain -> version] map
			setXcmVersion: async (version: number) => {
				const changes: any = {};
				changes.PolkadotXcm = changes.PolkadotXcm ?? {};
				changes.PolkadotXcm.SafeXcmVersion = changes.PolkadotXcm.SafeXcmVersion ?? 0;
				changes.PolkadotXcm.SafeXcmVersion = 5;

				changes.PolkadotXcm.SupportedVersion = changes.PolkadotXcm.SupportedVersion ?? [];
				changes.PolkadotXcm.SupportedVersion.push(
					[[5, { V5: {parents: 1, interior: { X1: [{Parachain: 2042}]}}}], 5]
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
			// getTokens: async (tokens) => {
			// 	const results = [];
			// 	for (const [who, what] of tokens) {
			// 		if (what === 'Relay') {
			// 			results.push((await assetHubApi
			// 				.query
			// 				.System
			// 				.Account
			// 				.getValue(who)).data.free
			// 			);
			// 		} else if (what === 'USDT') {
			// 			results.push((await assetHubApi
			// 				.query
			// 				.Assets
			// 				.Account
			// 				.getValue(1984, who))?.balance ?? 0n
			// 			);
			// 		}
			// 	}
			// 	return results;
			// },
		},
		paraSovAccOnAssetHub,
		paraSovAccOnRelay,
	};
}


export function createPolkadotClient(endpoint, apiType) {
    const client = createClient(getWsProvider(endpoint));
    return client.getTypedApi(apiType);
}

export async function getFreeBalance(api, accountKey) {
	const balance = await api.query.System.Account.getValue(accountKey);
	return balance.data.free;
}

type WebSocketEndpoint = `wss://${string}`;

export const getRelayEndpoint = (): WebSocketEndpoint => {
	switch (process.env.NETWORK) {
		case 'westend':
			return 'wss://westend-rpc.polkadot.io';
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
			return 'wss://westend-asset-hub-rpc.polkadot.io';
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
			return process.env.RELAY_WASM;
		default:
			throw 'Set one of the available networks: westend kusama polkadot';
	}
}

export const getAssetHubWasm = (): string => {
	switch (process.env.NETWORK) {
		case 'westend':
			return '../../wasms/asset_hub_westend_runtime.compact.compressed.wasm';
		default:
			throw 'Set one of the available networks: westend kusama polkadot';
	}
}

export const getPenpalWasm = (): string => {
	switch (process.env.NETWORK) {
		case 'westend':
			return '../../wasms/penpal_runtime.compact.compressed.wasm';
		default:
			throw 'Set one of the available networks: westend kusama polkadot';
	}
}
