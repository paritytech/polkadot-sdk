import { sr25519CreateDerive } from "@polkadot-labs/hdkd";
import { generateMnemonic, entropyToMiniSecret, mnemonicToEntropy, } from "@polkadot-labs/hdkd-helpers";
import { getPolkadotSigner } from "polkadot-api/signer";
import { AccountId } from "polkadot-api";
export function createRandomAccount() {
    const mnemonic = generateMnemonic(128);
    return createAccountFromMnemonic(mnemonic);
}
export function createAccountFromMnemonic(mnemonic) {
    const miniSecret = entropyToMiniSecret(mnemonicToEntropy(mnemonic));
    const derive = sr25519CreateDerive(miniSecret);
    const keyPair = derive("");
    return {
        signer: getPolkadotSigner(keyPair.publicKey, "Sr25519", keyPair.sign),
        address: AccountId().dec(keyPair.publicKey),
        publicKey: keyPair.publicKey,
    };
}
