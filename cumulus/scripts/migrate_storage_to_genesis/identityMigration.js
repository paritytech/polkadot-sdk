const { u8aToHex, hexToString } = require('@polkadot/util');
const { xxhashAsHex } = require('@polkadot/util-crypto');

const migrateIdentity = async (key, data, api) => {
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
		Username: 'Vec<u8>',
	});

	// We want to migrate `IdentityOf` storage item
	let IdentityOfKeyToMigrate = "IdentityOf";
	let IdentityOfHexkeyToMigrate = xxhashAsHex(IdentityOfKeyToMigrate, 128);

	// We want to migrate `SubsOf` storage item
	let SubsOfkeyToMigrate = "SubsOf";
	let SubsOfHexkeyToMigrate = xxhashAsHex(SubsOfkeyToMigrate, 128);

	// We take the second half of the key, which is the storage item identifier
	let storageItem = u8aToHex(key.toU8a().slice(18, 34));

	// Migrate `IdentityOf` data to its new format
	if (IdentityOfHexkeyToMigrate === storageItem) {
		let decoded = api.createType('(Registration, Option<Username>)', data.toU8a(true));

		// Default value for `discord` and `github` fields.
		let discord = { none: null };
		let github = { none: null };

		// Get the `Registration` part from the `IdentityInfo` tuple
		let decodedJson = decoded.toJSON()[0];

		// Look for `Discord` and `Github` keys in `additional` field
		decodedJson.info.additional.forEach(([key, data]) => {
			let keyString = hexToString(key.raw)

			if (keyString.toLowerCase() === "discord") {
				discord = { raw: data.raw };
			} else if (keyString.toLowerCase() === "github") {
				github = { raw: data.raw };
			}
		});

		let judgements = [];
		decodedJson.judgements.forEach((judgement) => {
			if (!('feePaid' in judgement[1])) {
				judgements.push(judgement);
			}
		});

		// Migrate `IdentityInfo` data to the new format:
		// - remove `additional` field
		// - add `discord` field
		// - add `github` field
		// - set `deposit` to 0
		let decodedNew = api.createType(
			'(RegistrationNew, Option<Username>)',
			[
				{
					judgements: judgements,
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
				},
				null
			]
		);

		data = decodedNew.toHex();

	} else if (SubsOfHexkeyToMigrate === storageItem) {
		let decoded = api.createType('(Balance, BoundedVec<AccountId, MaxApprovals>)', data.toU8a(true));

		// Migrate `SubsOf` data:
		// - set Deposit to 0
		let decodedNew = api.createType(
			'(Balance, BoundedVec<AccountId, MaxApprovals>)',
			[0, decoded.toJSON()[1]]
		);

		data = decodedNew.toHex();
	}

	return data;
};

module.exports = migrateIdentity;
