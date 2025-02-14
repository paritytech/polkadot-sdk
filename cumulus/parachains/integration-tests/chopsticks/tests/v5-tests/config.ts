export const CONFIG = {
	// todo move westend_network into index.ts
  WESTEND_NETWORK: Uint8Array.from([
    225, 67, 242, 56, 3, 172, 80, 232, 246, 248, 230, 38, 149, 209, 206, 158,
    78, 29, 104, 170, 54, 193, 205, 44, 253, 21, 52, 2, 19, 243, 66, 62,
  ]),
  KEYS: {
    BOB: "5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty",
    ALICE: "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY",
  },
  WS_ADDRESSES: {
    AH: "wss://westend-asset-hub-rpc.polkadot.io",
    PENPAL: "wss://westend-penpal-rpc.polkadot.io",
    RC: "wss://westend-rpc.polkadot.io",
  },
};
