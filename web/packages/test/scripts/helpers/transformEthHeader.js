const readline = require('readline');
const rlp = require('rlp');
const web3 = require('web3');

const HEADER_FIELD_MAPPING = [
  ['parentHash', 'parent_hash'],
  ['timestamp', 'timestamp', web3.utils.hexToNumber],
  ['number', 'number', web3.utils.hexToNumber],
  ['miner', 'author'],
  ['transactionsRoot', 'transactions_root'],
  ['sha3Uncles', 'ommers_hash'],
  ['extraData', 'extra_data', web3.utils.hexToBytes],
  ['stateRoot', 'state_root'],
  ['receiptsRoot', 'receipts_root'],
  ['logsBloom', 'logs_bloom', web3.utils.hexToBytes],
  ['gasUsed', 'gas_used'],
  ['gasLimit', 'gas_limit'],
  ['difficulty', 'difficulty'],
  ['baseFeePerGas', 'base_fee'],
];

function parseHeader(input) {
  let data = JSON.parse(input)['result'];
  if (!data) {
    throw Error("Failed to parse header from input. Expected RPC response data as input");
  }
  return data;
}

function transformHeaderForParachain(header) {
  let output = {};
  for (const mapping of HEADER_FIELD_MAPPING) {
    let value =  header[mapping[0]];
    if (!value) {
      if (mapping[0] == "baseFeePerGas") {
        output[mapping[1]] = null;
        continue
      }

      throw Error("Field '" + mapping[0] + "' not found or is empty");
    }

    const mapperFunc = mapping[2];
    if (mapperFunc) {
      value = mapperFunc(value);
    }

    output[mapping[1]] = value;
  }

  output['seal'] = [
    rlp.encode(header['mixHash']).toJSON()['data'],
    rlp.encode(header['nonce']).toJSON()['data'],
  ];

  return output;
}

function run() {
  const rl = readline.createInterface({
    input: process.stdin,
    output: process.stdout,
    terminal: false
  });

  let buffer = "";
  rl.on('line', function(line){
    buffer += line;
  });

  rl.on('close', function() {
    console.log(JSON.stringify(
      transformHeaderForParachain(parseHeader(buffer)),
      null, // replacer
      4, // spaces
    ));
  });
}

run();
