import { sr25519CreateDerive } from "@polkadot-labs/hdkd";
import {
  DEV_PHRASE,
  entropyToMiniSecret,
  mnemonicToEntropy,
} from "@polkadot-labs/hdkd-helpers";
import { getPolkadotSigner, type PolkadotSigner } from "polkadot-api/signer";
import { AccountId } from "polkadot-api";
import type { Player } from "../types/index.ts";

export interface PlayerAccount {
  signer: PolkadotSigner;
  address: string;
  publicKey: Uint8Array;
}

const miniSecret = entropyToMiniSecret(mnemonicToEntropy(DEV_PHRASE));
const derive = sr25519CreateDerive(miniSecret);

const aliceKeyPair = derive("//Alice");
const bobKeyPair = derive("//Bob");

export const alice: PlayerAccount = {
  signer: getPolkadotSigner(aliceKeyPair.publicKey, "Sr25519", aliceKeyPair.sign),
  address: AccountId().dec(aliceKeyPair.publicKey),
  publicKey: aliceKeyPair.publicKey,
};

export const bob: PlayerAccount = {
  signer: getPolkadotSigner(bobKeyPair.publicKey, "Sr25519", bobKeyPair.sign),
  address: AccountId().dec(bobKeyPair.publicKey),
  publicKey: bobKeyPair.publicKey,
};

export function getPlayerFromUrl(): Player {
  const params = new URLSearchParams(window.location.search);
  const player = params.get("player");
  return player === "bob" ? "bob" : "alice";
}

export function getPlayerAccount(player: Player): PlayerAccount {
  return player === "alice" ? alice : bob;
}

export function getOpponentAccount(player: Player): PlayerAccount {
  return player === "alice" ? bob : alice;
}

export function getOpponentAddress(player: Player): string {
  return getOpponentAccount(player).address;
}
