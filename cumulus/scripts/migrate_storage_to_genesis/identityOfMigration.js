const { u8aToHex, hexToString } = require('@polkadot/util');
const { xxhashAsHex } = require('@polkadot/util-crypto');

// yarn migrate -c wss://rococo-rpc.polkadot.io -f asset-hub-kusama.json -p Identity -m ./identityOfMigration.js

// const queryIdentityOf = async (api) => {
// 	let allEntries = await api.query.identity.identityOf.entries();

// 	let totalIdentities = 0;
// 	let identitiesWithAdditional = 0;
// 	let additionalKeys = {}

// 	allEntries.forEach(([a, b]) => {
// 		JSON.parse(b).info.additional.forEach(([key, data]) => {
// 			identitiesWithAdditional += 1;

// 			let keyString = hexToString(key.raw)
// 			let dataString = hexToString(data.raw)

// 			// console.log(keyString, dataString)

// 			if (additionalKeys[keyString] !== undefined) {
// 				additionalKeys[keyString] += 1
// 			} else {
// 				additionalKeys[keyString] = 1
// 			}
// 		});
// 		totalIdentities += 1;
// 	});

// 	console.log("-------------------------------------------");
// 	console.log("Total Identities: ", totalIdentities);
// 	console.log("With additional:  ", identitiesWithAdditional);
// 	console.log("Additional Keys:  ", additionalKeys);
// 	console.log("-------------------------------------------");
// }

const migrateIdentityOf = async (key, data, api) => {
	// Print `IdentityOf` summary
	// await queryIdentityOf(api);

	// From https://github.com/polkadot-js/api/blob/master/packages/types/src/interfaces/identity/definitions.ts
	// Register the new `Registration` format type
	api.registry.register({
		IdentityInfoNew: {
		_fallback: 'IdentityInfoTo198',
		display: 'Data',
		legal: 'Data',
		web: 'Data',
		riot: 'Data',
		email: 'Data',
		pgpFingerprint: 'Option<H160>',
		image: 'Data',
		twitter: 'Data',
		discord: 'Data',
		},
		RegistrationNew: {
		_fallback: 'RegistrationTo198',
		judgements: 'Vec<RegistrationJudgement>',
		deposit: 'Balance',
		info: 'IdentityInfoNew'
		},
	});

	// We want to migrate `IdentityOf` storage item
	let keyToMigrate = "IdentityOf";
	let HexkeyToMigrate = xxhashAsHex(keyToMigrate, 128);

	// We take the second half of the key, which is the storage item identifier
	let storageItem = u8aToHex(key.toU8a().slice(18, 34));

	// Migrate `IdentitOf` data to its new format
	if (HexkeyToMigrate === storageItem) {
		console.log("Migrating 'IdentityOf' storage item...");

		let decoded = api.createType('Registration', data.toU8a(true));

		let discord = { none: null };

		// Look for `Discord` key
		let decodedJson = decoded.toJSON();

		decodedJson.info.additional.forEach(([key, data]) => {
			let keyString = hexToString(key.raw)
			let dataString = hexToString(data.raw)

			if (keyString === "Discord" || keyString === "discord") {
				console.log("Discord", dataString)
				discord = { raw: data.raw };
			}
		});

		// Migrate data to new format
		let decodedNew = api.createType(
			'RegistrationNew',
			{
				judgements: decodedJson.judgements,
				deposit: decodedJson.deposit,
				info: {
					display: decodedJson.info.display,
					legal: decodedJson.info.legal,
					web: decodedJson.info.web,
					riot: decodedJson.info.riot,
					email: decodedJson.info.email,
					pgpFingerprint: decodedJson.info.pgpFingerprint,
					image: decodedJson.info.image,
					twitter: decodedJson.info.twitter,
					discord: discord,
				}
			}
		);
		if (discord.raw) {
			console.log("==================================================");
			console.log("Decoded", decoded.toJSON());
			console.log("DecodedNew", decodedNew.toJSON());
			console.log("==================================================\n");
		}
		data = decodedNew;
	}

	return data;
};

module.exports = migrateIdentityOf;
