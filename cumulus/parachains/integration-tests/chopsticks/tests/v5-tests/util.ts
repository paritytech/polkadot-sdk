// import { createClient } from "polkadot-api";
import { createClient, type ChainDefinition, type SS58String, type TypedApi } from "polkadot-api"
import { withPolkadotSdkCompat } from "polkadot-api/polkadot-sdk-compat";
import { getWsProvider } from "polkadot-api/ws-provider/web";
import { setupNetworks, type NetworkContext } from "@acala-network/chopsticks-testing"
import { XcmV3Junctions, XcmV3Junction, XcmVersionedLocation, wnd_penpal, wnd_rc, wnd_ah } from "@polkadot-api/descriptors"

export interface MockChain<T extends ChainDefinition> {
	context: NetworkContext,
	api: TypedApi<T>
	// who, what, amount.
	setTokens: (tokens: [SS58String, Token, bigint][]) => Promise<void>,
	// who, what.
	getTokens: (tokens: [SS58String, Token][]) => Promise<bigint[]>,
	setXcmVersion: (version: number) => Promise<void>,
}

export interface MockNetwork {
	parachain: MockChain<typeof wnd_penpal>,
	relay: MockChain<typeof wnd_rc>,
	assetHub: MockChain<typeof wnd_ah>,
	paraSovAccOnRelay: SS58String,
	paraSovAccOnAssetHub: SS58String,
}

type Token = 'Para' | 'Relay';

export interface MockChain<T extends ChainDefinition> {
	context: NetworkContext,
	api: TypedApi<T>
	// who, what, amount.
	setTokens: (tokens: [SS58String, Token, bigint][]) => Promise<void>,
	// who, what.
	getTokens: (tokens: [SS58String, Token][]) => Promise<bigint[]>,
	setXcmVersion: (version: number) => Promise<void>,
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

	return {
		parachain: {
			context: parachain,
			api: parachainApi,
			setTokens: async (tokens) => {
				const changes: any = {};
				for (const [who, what, amount] of tokens) {
					if (what === 'Para') {
						changes.System = {
							Account: [
								[[who], { providers: 1, data: { free: amount } }],
							],
						};
					} else if (what === 'Relay') {
						// changes.ForeignAssets = changes.ForeignAssets ?? {};
						// changes.ForeignAssets.Asset = changes.ForeignAssets.Asset ?? [];
						// changes.ForeignAssets.Account = changes.ForeignAssets.Account ?? [];
						//
						// changes.ForeignAssets.Asset.push();
						//
						// changes.Assets = changes.Assets ?? {};
						// changes.Assets.Asset = changes.Assets.Asset ?? [[[[1984]], { supply: amount }],];
						// changes.Assets.Account = changes.Assets.Account ?? [];
						// changes.Assets.Account.push(
						// 	[[1984, who], { balance: amount }],
						// );

						// todo: split setTOkens into setSystem and SetForeginAsset,
						// todo: make a map of tokens to use in set and get,
						changes.ForeignAssets = {
							Asset: [
								[[{
									parents: 1,
									interior: {
										X3: [
											{Parachain: 1000},
											{PalletInstance: 50},
											{GeneralIndex: 1984n},
										],
									},
								}],
									{ supply: amount + 1n }],
								[[{ parents: 1, interior: 'Here' }], { supply: amount + 2n }],
							],
							Account: [
								[[{ parents: 1, interior:
										{
										X3: [
											{Parachain: 1000},
											{PalletInstance: 50},
											{GeneralIndex: 1984n},
										],
									},

								}, who], { balance: amount + 1n }],
								[[{ parents: 1, interior: 'Here' }, who], { balance: amount + 2n }],
							],
						};
					}
				}
				console.log('setting tokens for Penpal');
				await parachain.dev.setStorage(changes);
			},
			setXcmVersion: async (version: number) => {
				const changes: any = {};
				changes.PolkadotXcm = {
					SafeXcmVersion: version,
				}
				await parachain.dev.setStorage(changes);
			},
			getTokens: async (tokens) => {
				const results = [];
				for (const [who, what] of tokens) {
					if (what === 'Para') {
						results.push((await parachainApi.query.System.Account.getValue(who)).data.free);
					} else if (what === 'Relay') {
						results.push((await parachainApi
							.query
							.ForeignAssets
							.Account
							.getValue({
								parents: 1, interior: XcmV3Junctions.Here()
							}, who))?.balance ?? 0n
						);
					} else if (what == 'USDT') {
						results.push((await parachainApi
							.query
							.ForeignAssets
							.Account
							.getValue({
								parents: 1, interior: XcmV3Junctions.X3([
									XcmV3Junction.Parachain(1000),
									XcmV3Junction.PalletInstance(50),
									XcmV3Junction.GeneralIndex(1984n),
								])}, who))?.balance ?? 0n
						);
					}
				}
				console.log('getting tokens from Penpal');
				return results;
			},
		},
		relay: {
			context: polkadot,
			api: relayApi,
			setTokens: async (tokens) => {
				const changes: any = {};
				for (const [who, what, amount] of tokens) {
					if (what === 'Relay') {
						changes.System = changes.System ?? {};
						changes.System.Account = changes.System.Account ?? [];
						changes.System.Account.push(
							[[who], { providers: 1, data: { free: amount } }],
						);
					}
				}
				await polkadot.dev.setStorage(changes);
			},
			setXcmVersion: async (version: number) => {
				const changes: any = {};
				changes.XcmPallet = {
					SafeXcmVersion: version,
				}
				await polkadot.dev.setStorage(changes);
			},
			getTokens: async (tokens) => {
				const results = [];
				for (const [who, what] of tokens) {
					if (what === 'Relay') {
						results.push((await relayApi
							.query
							.System
							.Account
							.getValue(who)).data.free
						);
					}
				}
				return results;
			},
		},
		assetHub: {
			context: assetHub,
			api: assetHubApi,
			setTokens: async (tokens) => {
				const changes: any = {};
				for (const [who, what, amount] of tokens) {
					if (what === 'Relay') {
						changes.System = changes.System ?? {};
						changes.System.Account = changes.System.Account ?? [];
						changes.System.Account.push(
							[[who], { providers: 1, data: { free: amount } }],
						);
					} else if (what == 'USDT') {
						changes.Assets = changes.Assets ?? {};
						changes.Assets.Asset = changes.Assets.Asset ?? [[[[1984]], { supply: amount }],];
						changes.Assets.Account = changes.Assets.Account ?? [];
						changes.Assets.Account.push(
							[[1984, who], { balance: amount }],
						);
						// changes.Assets = {
						// 	Asset: [
						// 		[[[what]], { supply: amount }],
						// 	],
						// 	Account: [
						// 		[[what, who], { balance: amount }],
						// 	],
						// };
					}
				}
				await assetHub.dev.setStorage(changes);
			},
			setXcmVersion: async (version: number) => {
				const changes: any = {};
				changes.PolkadotXcm = changes.PolkadotXcm ?? {};
				changes.PolkadotXcm.SafeXcmVersion = changes.PolkadotXcm.SafeXcmVersion ?? 0;
				changes.PolkadotXcm.SafeXcmVersion = 5;

				changes.PolkadotXcm.SupportedVersion = changes.PolkadotXcm.SupportedVersion ?? [];
				changes.PolkadotXcm.SupportedVersion.push(
					[[5, { V5: {parents: 1, interior: { X1: [{Parachain: 2042}]}}}], 5]

				);

				// changes.PolkadotXcm.SafeXcmVersion = 5;
				// changes.PolkadotXcm = {
				// 	SafeXcmVersion: version,
				// 	// SupportedVersion: {
				// 	// 	[version]: {  // First key: XCM Version
				// 	// 		['V5']: {         // Second key: VersionedLocation (e.g., V5)
				// 	// 			parents: 0,
				// 	// 			interior: { X1: { Parachain: 2000 } } // Example location
				// 	// 		}
				// 	// 	},
				// 	// 	value: version // Third value: Supported XCM Version
				// 	// }
				// }
				await assetHub.dev.setStorage(changes);
			},
			getTokens: async (tokens) => {
				const results = [];
				for (const [who, what] of tokens) {
					if (what === 'Relay') {
						results.push((await assetHubApi
							.query
							.System
							.Account
							.getValue(who)).data.free
						);
					} else if (what === 'USDT') {
						results.push((await assetHubApi
							.query
							.Assets
							.Account
							.getValue(1984, who))?.balance ?? 0n
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
