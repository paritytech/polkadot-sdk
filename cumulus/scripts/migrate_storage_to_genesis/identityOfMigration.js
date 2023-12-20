const { u8aToHex, hexToString } = require('@polkadot/util');
const { xxhashAsHex } = require('@polkadot/util-crypto');

const migrateIdentityOf = async (key, data, api) => {
	// Register the new `Registration` format type
	api.registry.register({
		IdentityInfoNew: {
		_fallback: 'IdentityInfoTo198',
		display: 'Data',
		legal: 'Data',
		web: 'Data',
		matrix: 'Data',
		email: 'Data',
		pgpFingerprint: 'Option<H160>',
		image: 'Data',
		twitter: 'Data',
		github: 'Data',
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
		let decoded = api.createType('Registration', data.toU8a(true));

		// Default value for `discord` and `github` fields.
		let discord = { none: null };
		let github = { none: null };

		let decodedJson = decoded.toJSON();

		// Look for `Discord` and `Github` keys in `additional` field
		decodedJson.info.additional.forEach(([key, data]) => {
			let keyString = hexToString(key.raw)

			if (keyString.toLowerCase() === "discord") {
				discord = { raw: data.raw };
			}
			if (keyString.toLowerCase() === "github") {
				github = { raw: data.raw };
			}
		});

		// Migrate data to the new format:
		// - remove `additional` field
		// - add `discord` field
		// - add `github` field
		// - set `deposit` to 0
		let decodedNew = api.createType(
			'RegistrationNew',
			{
				judgements: decodedJson.judgements,
				deposit: 0,
				info: {
					display: decodedJson.info.display,
					legal: decodedJson.info.legal,
					web: decodedJson.info.web,
					matrix: decodedJson.info.riot,
					email: decodedJson.info.email,
					pgpFingerprint: decodedJson.info.pgpFingerprint,
					image: decodedJson.info.image,
					twitter: decodedJson.info.twitter,
					github: github,
					discord: discord,
				}
			}
		);

		console.log("\n------------- 'IdentityOf' migration  ------------");
		console.log("Original", decoded.toJSON());
		console.log("Migration", decodedNew.toJSON());

		data = decodedNew.toHex();
	}

	return data;
};

module.exports = migrateIdentityOf;
