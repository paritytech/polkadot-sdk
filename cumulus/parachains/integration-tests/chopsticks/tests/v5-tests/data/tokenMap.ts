import { XcmV3Junctions, XcmV3Junction } from "@polkadot-api/descriptors"

export enum Asset {
	USDT = "USDT",
	WND = "WND",
	// Add more tokens as needed
}

const assetIndexMap: Record<Asset, number> = {
	[Asset.USDT]: 1984,
	[Asset.WND]: 1,
};

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

		return {
			// todo isAH interior needs to be X2 for AH and X3 for others. move to a variable
			parents: isAH ? 0 : 1,
			interior: asset == Asset.WND ? 'Here' : {
				X3: [
					{Parachain: 1000},
					{PalletInstance: 50},
					{GeneralIndex: index},
				],
			},
		};
	}
}

const assetMap = new AssetMap(assetIndexMap);

// Example usage:
console.log(assetMap.getIndex(Asset.USDT)); // Output: 1
console.log(assetMap.getRelativeLocation(Asset.WND, false));
