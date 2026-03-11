import { sr25519CreateDerive } from "@polkadot-labs/hdkd";
import {
  entropyToMiniSecret,
  mnemonicToEntropy,
  generateMnemonic,
} from "@polkadot-labs/hdkd-helpers";
import { getPolkadotSigner, type PolkadotSigner } from "polkadot-api/signer";
import { AccountId } from "polkadot-api";

export interface PlayerAccount {
  signer: PolkadotSigner;
  address: string;
  publicKey?: Uint8Array;
}

const STORAGE_KEY = "battleship-wallet-mnemonic";

export function getOrCreateWallet(): PlayerAccount {
  let mnemonic = localStorage.getItem(STORAGE_KEY);
  if (!mnemonic) {
    mnemonic = generateMnemonic(128);
    localStorage.setItem(STORAGE_KEY, mnemonic);
  }

  const miniSecret = entropyToMiniSecret(mnemonicToEntropy(mnemonic));
  const derive = sr25519CreateDerive(miniSecret);
  const keyPair = derive("");

  const signer = getPolkadotSigner(keyPair.publicKey, "Sr25519", keyPair.sign);
  const address = AccountId().dec(keyPair.publicKey);

  return { signer, address, publicKey: keyPair.publicKey };
}
