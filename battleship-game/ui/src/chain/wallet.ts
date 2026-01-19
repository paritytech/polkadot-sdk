import type { PolkadotSigner } from "polkadot-api/signer";
import { getChainClient } from "./client.ts";

const DECIMALS = 12;
const UNIT = 10n ** BigInt(DECIMALS);
const MILLI_UNIT = 10n ** 9n;

export interface WalletInfo {
  name: string;
  displayName: string;
}

export interface WalletAccount {
  address: string;
  name: string;
  signer: PolkadotSigner;
}

interface InjectedAccount {
  address: string;
  name?: string;
  polkadotSigner: PolkadotSigner;
}

interface InjectedExtension {
  name: string;
  getAccounts(): InjectedAccount[];
  subscribe(callback: (accounts: InjectedAccount[]) => void): () => void;
  disconnect(): void;
}

const WALLET_DISPLAY_NAMES: Record<string, string> = {
  "polkadot-js": "Polkadot.js",
  "talisman": "Talisman",
  "subwallet-js": "SubWallet",
  "enkrypt": "Enkrypt",
  "fearless-wallet": "Fearless",
};

export class WalletManager {
  private extension: InjectedExtension | null = null;
  private accounts: InjectedAccount[] = [];
  private selectedAccount: InjectedAccount | null = null;
  private unsubscribe: (() => void) | null = null;
  private accountChangeCallback: ((accounts: WalletAccount[]) => void) | null = null;

  detectWallets(): WalletInfo[] {
    if (typeof window === "undefined") return [];
    
    const injectedWeb3 = (window as unknown as { injectedWeb3?: Record<string, unknown> }).injectedWeb3;
    if (!injectedWeb3) return [];
    
    return Object.keys(injectedWeb3).map((name) => ({
      name,
      displayName: WALLET_DISPLAY_NAMES[name] || name,
    }));
  }

  async connect(walletName: string): Promise<boolean> {
    try {
      const injectedWeb3 = (window as unknown as { 
        injectedWeb3?: Record<string, { 
          enable: (dappName: string) => Promise<{
            accounts: {
              get: () => Promise<Array<{ address: string; name?: string }>>;
              subscribe: (cb: (accounts: Array<{ address: string; name?: string }>) => void) => () => void;
            };
            signer: {
              signPayload: (payload: unknown) => Promise<{ signature: string }>;
              signRaw: (raw: unknown) => Promise<{ signature: string }>;
            };
          }>;
        }>;
      }).injectedWeb3;

      const wallet = injectedWeb3?.[walletName];
      if (!wallet) {
        throw new Error(`Wallet ${walletName} not found`);
      }

      const injected = await wallet.enable("Battleship Game");
      
      const mapAccount = (acc: { address: string; name?: string }): InjectedAccount => ({
        address: acc.address,
        name: acc.name,
        polkadotSigner: this.createSignerProxy(acc.address, injected.signer),
      });

      const rawAccounts = await injected.accounts.get();
      this.accounts = rawAccounts.map(mapAccount);

      this.extension = {
        name: walletName,
        getAccounts: () => this.accounts,
        subscribe: (callback) => {
          return injected.accounts.subscribe((accs) => {
            this.accounts = accs.map(mapAccount);
            callback(this.accounts);
          });
        },
        disconnect: () => {},
      };

      this.unsubscribe = this.extension.subscribe((newAccounts) => {
        this.accounts = newAccounts;
        if (this.selectedAccount) {
          const stillExists = newAccounts.some(
            (a) => a.address === this.selectedAccount?.address
          );
          if (!stillExists) {
            this.selectedAccount = newAccounts[0] || null;
          }
        }
        if (this.accountChangeCallback) {
          this.accountChangeCallback(this.getAccounts());
        }
      });

      return true;
    } catch (e) {
      console.error("Failed to connect wallet:", e);
      return false;
    }
  }

  private createSignerProxy(
    address: string, 
    signer: { signPayload: (p: unknown) => Promise<{ signature: string }>; signRaw: (r: unknown) => Promise<{ signature: string }> }
  ): PolkadotSigner {
    return {
      publicKey: new Uint8Array(32),
      signTx: async (callData, signedExtensions, metadata, atBlockNumber) => {
        const result = await signer.signPayload({
          address,
          method: callData,
          signedExtensions,
          version: metadata,
          blockNumber: atBlockNumber,
        });
        return this.hexToBytes(result.signature);
      },
      signBytes: async (data) => {
        const result = await signer.signRaw({
          address,
          data: this.bytesToHex(data),
          type: "bytes",
        });
        return this.hexToBytes(result.signature);
      },
    } as PolkadotSigner;
  }

  private hexToBytes(hex: string): Uint8Array {
    const cleanHex = hex.startsWith("0x") ? hex.slice(2) : hex;
    const bytes = new Uint8Array(cleanHex.length / 2);
    for (let i = 0; i < bytes.length; i++) {
      bytes[i] = parseInt(cleanHex.substr(i * 2, 2), 16);
    }
    return bytes;
  }

  private bytesToHex(bytes: Uint8Array): string {
    return "0x" + Array.from(bytes).map(b => b.toString(16).padStart(2, "0")).join("");
  }

  getAccounts(): WalletAccount[] {
    return this.accounts.map((account) => ({
      address: account.address,
      name: account.name || "Account",
      signer: account.polkadotSigner,
    }));
  }

  selectAccount(address: string): boolean {
    const account = this.accounts.find((a) => a.address === address);
    if (account) {
      this.selectedAccount = account;
      return true;
    }
    return false;
  }

  getSelectedAccount(): WalletAccount | null {
    if (!this.selectedAccount) return null;
    return {
      address: this.selectedAccount.address,
      name: this.selectedAccount.name || "Account",
      signer: this.selectedAccount.polkadotSigner,
    };
  }

  async getBalance(address: string): Promise<bigint> {
    try {
      const client = await getChainClient();
      const api = client.getUnsafeApi();
      const accountInfo = await api.query.System.Account.getValue(address, {
        at: "best",
      });
      return accountInfo.data.free as bigint;
    } catch (e) {
      console.error("Failed to get balance:", e);
      return 0n;
    }
  }

  onAccountChange(callback: (accounts: WalletAccount[]) => void): void {
    this.accountChangeCallback = callback;
  }

  disconnect(): void {
    if (this.unsubscribe) {
      this.unsubscribe();
      this.unsubscribe = null;
    }
    if (this.extension) {
      this.extension.disconnect();
      this.extension = null;
    }
    this.accounts = [];
    this.selectedAccount = null;
    this.accountChangeCallback = null;
  }

  isConnected(): boolean {
    return this.extension !== null;
  }

  getConnectedWalletName(): string | null {
    return this.extension?.name || null;
  }

  static formatBalance(raw: bigint): string {
    if (raw === 0n) return "0.000";

    const units = raw / UNIT;
    const remainder = raw % UNIT;
    const decimals = remainder.toString().padStart(12, "0").slice(0, 3);
    return `${units}.${decimals}`;
  }

  static parseBalance(input: string): bigint {
    const trimmed = input.trim();
    if (!trimmed || trimmed === "") return 0n;

    if (!/^\d*\.?\d*$/.test(trimmed)) {
      throw new Error("Invalid number format");
    }

    const [whole = "0", decimal = "0"] = trimmed.split(".");
    const paddedDecimal = decimal.padEnd(12, "0").slice(0, 12);

    const wholePart = whole === "" ? 0n : BigInt(whole);
    const decimalPart = paddedDecimal === "000000000000" ? 0n : BigInt(paddedDecimal);

    return wholePart * UNIT + decimalPart;
  }

  static validateStakeAmount(
    amount: bigint,
    balance: bigint
  ): { valid: boolean; error?: string } {
    const MIN_STAKE = MILLI_UNIT;
    const EXISTENTIAL_DEPOSIT = MILLI_UNIT;
    const ESTIMATED_FEES = MILLI_UNIT / 10n;

    if (amount < MIN_STAKE) {
      return { valid: false, error: "Minimum stake is 0.001 UNIT" };
    }

    const required = amount + EXISTENTIAL_DEPOSIT + ESTIMATED_FEES;
    if (required > balance) {
      return {
        valid: false,
        error: `Insufficient balance. Need at least ${WalletManager.formatBalance(required)} UNIT`,
      };
    }

    return { valid: true };
  }

  static getAvailableBalance(balance: bigint): bigint {
    const EXISTENTIAL_DEPOSIT = MILLI_UNIT;
    const ESTIMATED_FEES = MILLI_UNIT / 10n;
    const reserved = EXISTENTIAL_DEPOSIT + ESTIMATED_FEES;

    if (balance <= reserved) return 0n;
    return balance - reserved;
  }
}

let walletManagerInstance: WalletManager | null = null;

export function getWalletManager(): WalletManager {
  if (!walletManagerInstance) {
    walletManagerInstance = new WalletManager();
  }
  return walletManagerInstance;
}
