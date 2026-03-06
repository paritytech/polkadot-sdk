import { sr25519CreateDerive } from "@polkadot-labs/hdkd";
import { DEV_PHRASE, entropyToMiniSecret, mnemonicToEntropy, } from "@polkadot-labs/hdkd-helpers";
import { getPolkadotSigner } from "polkadot-api/signer";
import { AccountId } from "polkadot-api";
const miniSecret = entropyToMiniSecret(mnemonicToEntropy(DEV_PHRASE));
const derive = sr25519CreateDerive(miniSecret);
// Bot uses Charlie dev account
const charlieKeyPair = derive("//Charlie");
export const botAccount = {
    signer: getPolkadotSigner(charlieKeyPair.publicKey, "Sr25519", charlieKeyPair.sign),
    address: AccountId().dec(charlieKeyPair.publicKey),
    publicKey: charlieKeyPair.publicKey,
};
// Alice dev account for testing
const aliceKeyPair = derive("//Alice");
export const aliceAccount = {
    signer: getPolkadotSigner(aliceKeyPair.publicKey, "Sr25519", aliceKeyPair.sign),
    address: AccountId().dec(aliceKeyPair.publicKey),
    publicKey: aliceKeyPair.publicKey,
};
