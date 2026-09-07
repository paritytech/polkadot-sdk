import { XcmV3Junctions, XcmV3Junction } from "@polkadot-api/descriptors"

export enum Asset {
	USDT = "USDT",
	WND = "WND",
	// Add more tokens as needed
}

export class AssetMap {
	private map: Record<Asset, number>;

	constructor(map: Record<Asset, number>) {
		this.map = map;
	}

	getIndex(asset: Asset): number | undefined {
		return this.map[asset];
	}

	getRelativeLocation(asset: Asset, isAH: boolean) {
		const index = this.getIndex(asset);
		if (index === undefined) {
			throw new Error(`Asset ${asset} not found in map`);
		}

		if (asset === Asset.WND) {
			return isAH ? {parents: 0, interior: XcmV3Junctions.Here()} : {parents: 1, interior: XcmV3Junctions.Here()};
		}

		if (isAH) {
			return {
				parents: 0,
				interior: XcmV3Junctions.X2([
					XcmV3Junction.PalletInstance(50),
					XcmV3Junction.GeneralIndex(BigInt(index)),
				]),
			};
		}

		return {
			parents: 1,
			interior: XcmV3Junctions.X3([
				XcmV3Junction.Parachain(1000),
				XcmV3Junction.PalletInstance(50),
				XcmV3Junction.GeneralIndex(BigInt(index)),
			]),
		};
	}

	// Setting ForeignAsset in storage requires raw location
	getRelativeRawLocation(asset: Asset, isAH: boolean) {
		const index = this.getIndex(asset);
		if (index === undefined) {
			throw new Error(`Asset ${asset} not found in map`);
		}

		if (asset === Asset.WND) {
			return isAH ? {parents: 0, interior: 'Here'} : {parents: 1, interior: 'Here'};
		}

		if (isAH) {
			return {
				parents: 0,
				interior: {
					X2: [
						{PalletInstance: 50},
						{GeneralIndex: BigInt(index)},
					],
				}
			};
		}

		return {
			parents: 1,
			interior: {
				X3: [
					{Parachain: 1000},
					{PalletInstance: 50},
					{GeneralIndex: BigInt(index)},
				],
			},
		};
	}
}
