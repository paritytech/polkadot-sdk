import { sr25519CreateDerive } from "@polkadot-labs/hdkd";
import {
  generateMnemonic,
  entropyToMiniSecret,
  mnemonicToEntropy,
} from "@polkadot-labs/hdkd-helpers";
import { getPolkadotSigner, type PolkadotSigner } from "polkadot-api/signer";
import { AccountId } from "polkadot-api";

export interface BotAccount {
  signer: PolkadotSigner;
  address: string;
  publicKey: Uint8Array;
}

export function createRandomAccount(): BotAccount {
  const mnemonic = generateMnemonic(128);
  return createAccountFromMnemonic(mnemonic);
}

export function createAccountFromMnemonic(mnemonic: string): BotAccount {
  const miniSecret = entropyToMiniSecret(mnemonicToEntropy(mnemonic));
  const derive = sr25519CreateDerive(miniSecret);
  const keyPair = derive("");
  return {
    signer: getPolkadotSigner(keyPair.publicKey, "Sr25519", keyPair.sign),
    address: AccountId().dec(keyPair.publicKey),
    publicKey: keyPair.publicKey,
  };
}
