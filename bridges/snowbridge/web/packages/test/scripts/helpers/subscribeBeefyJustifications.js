// This script is to help with pulling in BEEFY data for generating fixtures for testing purposes.
// To use, run at least 2 relay chain nodes which have the BEEFY protocol active, eg:
//   polkadot --chain=rococo-local --tmp --ws-port=9944 --port=30444 --alice  --enable-offchain-indexing true
//   polkadot --chain=rococo-local --tmp --ws-port=9955 --port=30555 --bob  --enable-offchain-indexing true
// Then run this script to see output.
// Additionally, to get the addresses/public

const { ApiPromise, WsProvider } = require('@polkadot/api');
const WebSocket = require('ws');
const { base58Decode, addressToEvm, secp256k1Expand, secp256k1Compress, decodeAddress, encodeAddress, ethereumEncode, blake2AsHex, keccakAsHex } = require("@polkadot/util-crypto");
const { hexToU8a, u8aToHex, u8aToU8a } = require("@polkadot/util");
let { bundle } = require("@snowfork/snowbridge-types");

const RELAY_CHAIN_RPC_ENDPOINT = 'ws://127.0.0.1:9944';
const RELAY_CHAIN_HTTP_RPC_ENDPOINT = 'http://127.0.0.1:30444';
const PARACHAIN_ID = 1013;
const PARACHAIN_RPC_ENDPOINT = 'ws://127.0.0.1:11144';

async function start() {
  const wsProvider = new WsProvider(RELAY_CHAIN_RPC_ENDPOINT);
  const api = await ApiPromise.create({
    provider: wsProvider,
    types: {
      SignedCommitment: {
        commitment: 'Commitment',
        signatures: 'Vec<Option<BeefySignature>>'
      },
      Commitment: {
        payload: 'H256',
        block_number: 'BlockNumber',
        validator_set_id: 'ValidatorSetId'
      },
      ValidatorSetId: 'u64',
      BeefySignature: '[u8; 65]',
      Authorities: 'Vec<[u8; 33]>',
      MMRStorageKey: {
        prefix: 'Vec<u8>',
        pos: 'u64'
      },
      PalletId: 'u64',
      GenerateMMRProofResponse: {
        blockHash: 'BlockHash',
        leaf: 'MMREncodableOpaqueLeaf',
        proof: 'MMRProof',
      },
      BlockHash: 'H256',
      MMREncodableOpaqueLeaf: 'Vec<u8>',
      MMRProof: {
        /// The index of the leaf the proof is for.
        leafIndex: 'u64',
        /// Number of leaves in MMR, when the proof was generated.
        leafCount: 'u64',
        /// Proof elements (hashes of siblings of inner nodes on the path to the leaf).
        items: 'Vec<Hash>',
      },
      MMRLeafVec: 'Vec<MMRLeaf>',
      MMRLeaf: {
        parentNumberAndHash: 'ParentNumberAndHash',
        parachainHeads: 'H256',
        beefyNextAuthoritySet: 'BeefyNextAuthoritySet',
      },
      ParentNumberAndHash: {
        parentNumber: 'ParentNumber',
        hash: '[u8; 32]'
      },
      ParentNumber: 'u32',
      BeefyNextAuthoritySet: {
        id: 'u64',
        /// Number of validators in the set.
        len: 'u32',
        /// Merkle Root Hash build from BEEFY uncompressed AuthorityIds.
        root: 'H256',
      },
      ParachainHeader: {
        parentHash: 'H256',
        number: 'u32',
        stateRoot: 'H256',
        extrinsicsRoot: 'H256',
        digest: {
          logs: []
        }
      }
    },
    rpc: {
      beefy: {
        subscribeJustifications: {
          alias: ['beefy_subscribeJustifications', 'beefy_unsubscribeJustifications'],
          params: [],
          type: 'SignedCommitment',
          pubsub: [
            'justifications',
            'subscribeJustifications',
            'unsubscribeJustifications'
          ],
        }
      },
      mmr: {
        generateProof: {
          alias: ['mmr_generateProof'],
          params: [{
            name: 'leaf_index',
            type: 'u64'
          }],
          type: 'GenerateMMRProofResponse'
        }
      }
    }
  });

  console.log("Getting initial relay chain beefy authorities...");
  await getAuthorities(api);
  console.log("Subscribing to new beefy justifications...");
  await subscribeJustifications(api);
}

async function getAuthorities(api) {
  const authoritiesResponse = await getAuthoritiesDirect(api);
  let authorities = api.createType('Authorities', authoritiesResponse.toHex());

  const authoritiesEthereum = authorities.map(a => ethereumEncode(a));
  console.log("Starting authorities: ");
  console.log({
    authoritiesEthereum
  });

  return authoritiesEthereum;
}

async function subscribeJustifications(api) {
  api.rpc.beefy.subscribeJustifications(justification => {
    console.log("New beefy justification received:");
    const commitment = justification.commitment;
    const commitmentBytes = commitment.toHex();
    const rawCommitmentBytes = hexToU8a(commitmentBytes)
    const hashedCommitment = blake2AsHex(commitmentBytes, 256);
    console.log({ justification: justification.toHuman(), commitmentBytes, rawCommitmentBytes, hashedCommitment });
    getLatestMMRInJustification(justification, api)
  });
}

async function getLatestMMRInJustification(justification, api) {
  const blockNumber = justification.commitment.block_number.toString();
  const mmrRoot = justification.commitment.payload.toString();
  console.log({
    blockNumber,
    mmrRoot
  });
  console.log(`Justification is for block ${blockNumber}, getting MMR Leaf for that block...`);
  // Note: This just gets the MMR leaf for that block in the latest MMR.
  // We actually want to be getting the MMR leaf in this particulat justification's MMR
  // If there is a newer one, this may fail
  // TODO: Address this (may require adding to the generateProof RPC endpoint?)
  latestMMRLeaf = await getMMRLeafForBlock(blockNumber, api)
  paraHead = await getParaheads(blockNumber, api, PARACHAIN_ID);
  paraHeadData = await getParaHeadData(paraHead);
}

async function getMMRLeafForBlock(blockNumber, api) {
  console.log(`Getting proof and leaf for block ${blockNumber}...`);
  const mmrProof = await api.rpc.mmr.generateProof(blockNumber);
  console.log({ mmrProof: mmrProof.toHuman() });

  const mmrEncodableOpqueLeaf = api.createType('MMREncodableOpaqueLeaf', hexToU8a(mmrProof.leaf.toHex()))
  console.log({ leaf: api.createType('MMRLeaf', mmrEncodableOpqueLeaf).toHuman() })
  console.log({ proof: api.createType('MMRProof', mmrProof.proof).toHuman() })
}

async function getParaheads(blockNumber, api, parachainID) {
  // Note: This just gets the paraheads for that block in the latest MMR.
  // We actually want to be getting the paraheads in this particular leaf's para_heads commitments
  // If there is a newer one, this may fail
  // TODO: Address this

  // Also, for some reason the polkadot-js query.paras.heads api does not work
  // with no parameter to retrieve all heads, so we just use the raw state query
  console.log(`Getting parachain heads for ${blockNumber}...`);
  const allParaHeadsStorageKey = '0xcd710b30bd2eab0352ddcc26417aa1941b3c252fcb29d88eff4f3de5de4476c30a31c34bd88c539ec8000000';
  const allParaHeadsRaw = await api.rpc.state.getStorage(allParaHeadsStorageKey);
  console.log({ allParaHeadsRaw: allParaHeadsRaw.toHuman() });

  console.log(`Getting parachain head for ${PARACHAIN_ID} only...`);
  const paraHead = await api.query.paras.heads(parachainID);
  console.log({ paraHead: paraHead.toHuman() });
  return paraHead
}

async function getAuthoritiesDirect(api) {
  // For some reason the polkadot-js beefy.authorities function is not returning enough bytes.
  // This function just manually gets them.
  const beefyStorageQuery = "0x08c41974a97dbf15cfbec28365bea2da5e0621c4869aa60c02be9adcc98a0d1d";
  const authorities = await api.rpc.state.getStorage(beefyStorageQuery);
  return authorities;
}

async function getParaHeadData(paraHead) {
  const parachainWsProvider = new WsProvider(PARACHAIN_RPC_ENDPOINT);
  const parachainApi = await ApiPromise.create({
    provider: parachainWsProvider,
    typesBundle: bundle,
  });

  const truncatedHead = paraHead.toHuman().slice(0, 66);
  console.log({ truncatedHead })
  const headData = await parachainApi.rpc.chain.getHeader(truncatedHead);
  console.log({ headData: headData.toHuman() });

  const headerLogs = headData.toJSON().digest && headData.toJSON().digest.logs;
  const commitmentLog = headerLogs && headerLogs[0];
  if (commitmentLog) {
    console.log("Got new commitment: ");
    console.log({ commitmentLog });
  }
}

start();
