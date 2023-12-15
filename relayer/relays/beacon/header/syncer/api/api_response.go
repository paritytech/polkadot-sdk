package api

import (
	"encoding/hex"
	"fmt"
	"math/big"
	"strconv"
	"strings"

	"github.com/snowfork/snowbridge/relayer/relays/beacon/config"

	"github.com/ethereum/go-ethereum/common"
	"github.com/snowfork/go-substrate-rpc-client/v4/types"
	beaconjson "github.com/snowfork/snowbridge/relayer/relays/beacon/header/syncer/json"
	"github.com/snowfork/snowbridge/relayer/relays/beacon/header/syncer/scale"
	"github.com/snowfork/snowbridge/relayer/relays/beacon/header/syncer/util"
	"github.com/snowfork/snowbridge/relayer/relays/beacon/state"
)

type SyncCommitteePeriodUpdateResponse struct {
	Data struct {
		AttestedHeader struct {
			Beacon HeaderResponse `json:"beacon"`
		} `json:"attested_header"`
		NextSyncCommittee       SyncCommitteeResponse `json:"next_sync_committee"`
		NextSyncCommitteeBranch []string              `json:"next_sync_committee_branch"`
		FinalizedHeader         struct {
			Beacon HeaderResponse `json:"beacon"`
		} `json:"finalized_header"`
		FinalityBranch []string              `json:"finality_branch"`
		SyncAggregate  SyncAggregateResponse `json:"sync_aggregate"`
		SignatureSlot  string                `json:"signature_slot"`
	} `json:"data"`
}

type BeaconBlockResponse struct {
	Data struct {
		Message struct {
			Slot          string `json:"slot"`
			ProposerIndex string `json:"proposer_index"`
			ParentRoot    string `json:"parent_root"`
			StateRoot     string `json:"state_root"`
			Body          struct {
				RandaoReveal string `json:"randao_reveal"`
				Eth1Data     struct {
					DepositRoot  string `json:"deposit_root"`
					DepositCount string `json:"deposit_count"`
					BlockHash    string `json:"block_hash"`
				} `json:"eth1_data"`
				Graffiti          string                        `json:"graffiti"`
				ProposerSlashings []ProposerSlashingResponse    `json:"proposer_slashings"`
				AttesterSlashings []AttesterSlashingResponse    `json:"attester_slashings"`
				Attestations      []AttestationResponse         `json:"attestations"`
				Deposits          []DepositResponse             `json:"deposits"`
				VoluntaryExits    []SignedVoluntaryExitResponse `json:"voluntary_exits"`
				SyncAggregate     SyncAggregateResponse         `json:"sync_aggregate"`
				ExecutionPayload  struct {
					ParentHash    string               `json:"parent_hash"`
					FeeRecipient  string               `json:"fee_recipient"`
					StateRoot     string               `json:"state_root"`
					ReceiptsRoot  string               `json:"receipts_root"`
					LogsBloom     string               `json:"logs_bloom"`
					PrevRandao    string               `json:"prev_randao"`
					BlockNumber   string               `json:"block_number"`
					GasLimit      string               `json:"gas_limit"`
					GasUsed       string               `json:"gas_used"`
					Timestamp     string               `json:"timestamp"`
					ExtraData     string               `json:"extra_data"`
					BaseFeePerGas string               `json:"base_fee_per_gas"`
					BlockHash     string               `json:"block_hash"`
					Transactions  []string             `json:"transactions"`
					Withdrawals   []WithdrawalResponse `json:"withdrawals"`
				} `json:"execution_payload"`
				BlsToExecutionChanges []SignedBLSToExecutionChangeResponse `json:"bls_to_execution_changes"`
			} `json:"body"`
		} `json:"message"`
	} `json:"data"`
}

type BootstrapResponse struct {
	Data struct {
		Header struct {
			Beacon HeaderResponse `json:"beacon"`
		} `json:"header"`
		CurrentSyncCommittee       SyncCommitteeResponse `json:"current_sync_committee"`
		CurrentSyncCommitteeBranch []string              `json:"current_sync_committee_branch"`
	} `json:"data"`
}

type FinalizedCheckpointResponse struct {
	Data struct {
		Finalized struct {
			Root string `json:"root"`
		} `json:"finalized"`
	} `json:"data"`
}

type SignedHeaderResponse struct {
	Message   HeaderResponse `json:"message"`
	Signature string         `json:"signature"`
}

type CheckpointResponse struct {
	Epoch string `json:"epoch"`
	Root  string `json:"root"`
}

type DepositDataResponse struct {
	Pubkey                string `json:"pubkey"`
	WithdrawalCredentials string `json:"withdrawal_credentials"`
	Amount                string `json:"amount"`
	Signature             string `json:"signature"`
}

type DepositResponse struct {
	Proof []string            `json:"proof"`
	Data  DepositDataResponse `json:"data"`
}

type AttestationDataResponse struct {
	Slot            string             `json:"slot"`
	Index           string             `json:"index"`
	BeaconBlockRoot string             `json:"beacon_block_root"`
	Source          CheckpointResponse `json:"source"`
	Target          CheckpointResponse `json:"target"`
}

type IndexedAttestationResponse struct {
	AttestingIndices []string                `json:"attesting_indices"`
	Data             AttestationDataResponse `json:"data"`
	Signature        string                  `json:"signature"`
}

type AttesterSlashingResponse struct {
	Attestation1 IndexedAttestationResponse `json:"attestation_1"`
	Attestation2 IndexedAttestationResponse `json:"attestation_2"`
}

type ProposerSlashingResponse struct {
	SignedHeader1 SignedHeaderResponse `json:"signed_header_1"`
	SignedHeader2 SignedHeaderResponse `json:"signed_header_2"`
}

type AttestationResponse struct {
	AggregationBits string                  `json:"aggregation_bits"`
	Data            AttestationDataResponse `json:"data"`
	Signature       string                  `json:"signature"`
}

type SignedVoluntaryExitResponse struct {
	Message   VoluntaryExitResponse `json:"message"`
	Signature string                `json:"signature"`
}

type VoluntaryExitResponse struct {
	Epoch          string `json:"epoch"`
	ValidatorIndex string `json:"validator_index"`
}

type HeaderResponse struct {
	Slot          string `json:"slot"`
	ProposerIndex string `json:"proposer_index"`
	ParentRoot    string `json:"parent_root"`
	StateRoot     string `json:"state_root"`
	BodyRoot      string `json:"body_root"`
}

type SyncCommitteeResponse struct {
	Pubkeys         []string `json:"pubkeys"`
	AggregatePubkey string   `json:"aggregate_pubkey"`
}

type BeaconHeader struct {
	Slot          uint64      `json:"slot"`
	ProposerIndex uint64      `json:"proposer_index"`
	ParentRoot    common.Hash `json:"parent_root"`
	StateRoot     common.Hash `json:"state_root"`
	BodyRoot      common.Hash `json:"body_root"`
}

type Bootstrap struct {
	Header                     HeaderResponse
	CurrentSyncCommittee       beaconjson.SyncCommittee
	CurrentSyncCommitteeBranch []string
}

type Genesis struct {
	ValidatorsRoot common.Hash
	Time           uint64
}

type BeaconBlock struct {
	Slot          uint64
	ProposerIndex uint64
	ParentRoot    common.Hash
	StateRoot     common.Hash
	BodyRoot      common.Hash
}

type FinalizedCheckpoint struct {
	FinalizedBlockRoot common.Hash
}

func (h *HeaderResponse) ToBeaconHeader() (BeaconHeader, error) {
	slot, err := util.ToUint64(h.Slot)
	if err != nil {
		return BeaconHeader{}, err
	}

	proposerIndex, err := util.ToUint64(h.ProposerIndex)
	if err != nil {
		return BeaconHeader{}, err
	}

	return BeaconHeader{
		Slot:          slot,
		ProposerIndex: proposerIndex,
		ParentRoot:    common.HexToHash(h.ParentRoot),
		StateRoot:     common.HexToHash(h.StateRoot),
		BodyRoot:      common.HexToHash(h.BodyRoot),
	}, nil
}

type BranchResponse []string

type BeaconHeaderResponse struct {
	Data struct {
		Root      string `json:"root"`
		Canonical bool   `json:"canonical"`
		Header    struct {
			Message   HeaderResponse `json:"message"`
			Signature string         `json:"signature"`
		} `json:"header"`
	} `json:"data"`
}

type SyncAggregateResponse struct {
	SyncCommitteeBits      string `json:"sync_committee_bits"`
	SyncCommitteeSignature string `json:"sync_committee_signature"`
}

type GenesisResponse struct {
	Data struct {
		GenesisValidatorsRoot string `json:"genesis_validators_root"`
		Time                  string `json:"genesis_time"`
	} `json:"data"`
}

type ErrorMessage struct {
	StatusCode int    `json:"statusCode"`
	Error      string `json:"error"`
	Message    string `json:"message"`
}

type ForkResponse struct {
	Data struct {
		PreviousVersion string `json:"previous_version"`
		CurrentVersion  string `json:"current_version"`
		Epoch           string `json:"epoch"`
	} `json:"data"`
}

type LatestFinalisedUpdateResponse struct {
	Data struct {
		AttestedHeader struct {
			Beacon HeaderResponse `json:"beacon"`
		} `json:"attested_header"`
		FinalizedHeader struct {
			Beacon HeaderResponse `json:"beacon"`
		} `json:"finalized_header"`
		FinalityBranch []string              `json:"finality_branch"`
		SyncAggregate  SyncAggregateResponse `json:"sync_aggregate"`
		SignatureSlot  string                `json:"signature_slot"`
	} `json:"data"`
}

type LatestHeaderUpdateResponse struct {
	Data struct {
		AttestedHeader HeaderResponse        `json:"attested_header"`
		SyncAggregate  SyncAggregateResponse `json:"sync_aggregate"`
	} `json:"data"`
}

func (h *HeaderResponse) ToScale() (scale.BeaconHeader, error) {
	slot, err := strconv.ParseUint(h.Slot, 10, 64)
	if err != nil {
		return scale.BeaconHeader{}, fmt.Errorf("parse slot as int: %w", err)
	}

	proposerIndex, err := strconv.ParseUint(h.ProposerIndex, 10, 64)
	if err != nil {
		return scale.BeaconHeader{}, fmt.Errorf("parse proposerIndex as int: %w", err)
	}

	return scale.BeaconHeader{
		Slot:          types.NewU64(slot),
		ProposerIndex: types.NewU64(proposerIndex),
		ParentRoot:    types.NewH256(common.HexToHash(h.ParentRoot).Bytes()),
		StateRoot:     types.NewH256(common.HexToHash(h.StateRoot).Bytes()),
		BodyRoot:      types.NewH256(common.HexToHash(h.BodyRoot).Bytes()),
	}, nil
}

type WithdrawalResponse struct {
	Index          string `json:"index"`
	ValidatorIndex string `json:"validator_index"`
	Address        string `json:"address"`
	Amount         string `json:"amount"`
}

type BLSToExecutionChangeResponse struct {
	ValidatorIndex     string `json:"validator_index"`
	FromBlsPubkey      string `json:"from_bls_pubkey"`
	ToExecutionAddress string `json:"to_execution_address"`
}

type SignedBLSToExecutionChangeResponse struct {
	Message   *BLSToExecutionChangeResponse `json:"message,omitempty"`
	Signature string                        `json:"signature,omitempty"`
}

func (s *SignedBLSToExecutionChangeResponse) ToFastSSZ() (*state.SignedBLSToExecutionChange, error) {
	validateIndex, err := util.ToUint64(s.Message.ValidatorIndex)
	if err != nil {
		return nil, err
	}
	pubKey, err := util.HexStringToPublicKey(s.Message.FromBlsPubkey)
	if err != nil {
		return nil, err
	}
	address, err := util.HexStringTo20Bytes(s.Message.ToExecutionAddress)
	if err != nil {
		return nil, err
	}
	signature, err := util.HexStringTo96Bytes(s.Signature)
	if err != nil {
		return nil, err
	}
	return &state.SignedBLSToExecutionChange{Message: &state.BLSToExecutionChange{ValidatorIndex: validateIndex, FromBlsPubkey: pubKey[:], ToExecutionAddress: address[:]}, Signature: signature[:]}, nil
}

func (w *WithdrawalResponse) ToFastSSZ() (*state.Withdrawal, error) {
	index, err := util.ToUint64(w.Index)
	if err != nil {
		return nil, err
	}
	validatorIndex, err := util.ToUint64(w.ValidatorIndex)
	if err != nil {
		return nil, err
	}
	address, err := util.HexStringTo20Bytes(w.Address)
	if err != nil {
		return nil, err
	}
	amount, err := util.ToUint64(w.Amount)
	if err != nil {
		return nil, err
	}
	return &state.Withdrawal{
		Index:          index,
		ValidatorIndex: validatorIndex,
		Address:        address,
		Amount:         amount,
	}, nil
}

func (h *HeaderResponse) ToFastSSZ() (*state.BeaconBlockHeader, error) {
	slot, err := util.ToUint64(h.Slot)
	if err != nil {
		return nil, err
	}

	proposerIndex, err := util.ToUint64(h.ProposerIndex)
	if err != nil {
		return nil, err
	}

	parentRoot, err := util.HexStringToByteArray(h.ParentRoot)
	if err != nil {
		return nil, err
	}

	stateRoot, err := util.HexStringToByteArray(h.StateRoot)
	if err != nil {
		return nil, err
	}

	bodyRoot, err := util.HexStringToByteArray(h.BodyRoot)
	if err != nil {
		return nil, err
	}

	return &state.BeaconBlockHeader{
		Slot:          slot,
		ProposerIndex: proposerIndex,
		ParentRoot:    parentRoot,
		StateRoot:     stateRoot,
		BodyRoot:      bodyRoot,
	}, nil
}

func (h BeaconHeader) ToScale() (scale.BeaconHeader, error) {
	return scale.BeaconHeader{
		Slot:          types.NewU64(h.Slot),
		ProposerIndex: types.NewU64(h.ProposerIndex),
		ParentRoot:    types.NewH256(h.ParentRoot.Bytes()),
		StateRoot:     types.NewH256(h.StateRoot.Bytes()),
		BodyRoot:      types.NewH256(h.BodyRoot.Bytes()),
	}, nil
}

func (s SyncCommitteeResponse) ToScale() (scale.SyncCommittee, error) {
	var syncCommitteePubkeys [][48]byte

	for _, pubkey := range s.Pubkeys {
		publicKey, err := util.HexStringToPublicKey(pubkey)
		if err != nil {
			return scale.SyncCommittee{}, fmt.Errorf("convert sync committee pubkey to byte array: %w", err)
		}

		syncCommitteePubkeys = append(syncCommitteePubkeys, publicKey)
	}

	syncCommitteeAggPubkey, err := util.HexStringToPublicKey(s.AggregatePubkey)
	if err != nil {
		return scale.SyncCommittee{}, fmt.Errorf("convert sync committee aggregate bukey to byte array: %w", err)
	}

	return scale.SyncCommittee{
		Pubkeys:         syncCommitteePubkeys,
		AggregatePubkey: syncCommitteeAggPubkey,
	}, nil
}

func (s SyncAggregateResponse) ToScale() (scale.SyncAggregate, error) {
	bits, err := util.HexStringToByteArray(s.SyncCommitteeBits)
	if err != nil {
		return scale.SyncAggregate{}, err
	}

	aggregateSignature, err := util.HexStringToByteArray(s.SyncCommitteeSignature)
	if err != nil {
		return scale.SyncAggregate{}, err
	}

	var syncCommitteeSignature [96]byte
	copy(syncCommitteeSignature[:], aggregateSignature)

	return scale.SyncAggregate{
		SyncCommitteeBits:      bits,
		SyncCommitteeSignature: syncCommitteeSignature,
	}, nil
}

func (b BeaconBlockResponse) ToScale() (scale.BeaconBlock, error) {
	dataMessage := b.Data.Message

	slot, err := util.ToUint64(dataMessage.Slot)
	if err != nil {
		return scale.BeaconBlock{}, fmt.Errorf("parse slot as int: %w", err)
	}

	proposerIndex, err := util.ToUint64(dataMessage.ProposerIndex)
	if err != nil {
		return scale.BeaconBlock{}, fmt.Errorf("parse proposerIndex as int: %w", err)
	}

	body := dataMessage.Body

	syncAggregate, err := body.SyncAggregate.ToScale()
	if err != nil {
		return scale.BeaconBlock{}, err
	}

	proposerSlashings := []scale.ProposerSlashing{}

	for _, proposerSlashing := range body.ProposerSlashings {
		proposerSlashingScale, err := proposerSlashing.ToScale()
		if err != nil {
			return scale.BeaconBlock{}, err
		}

		proposerSlashings = append(proposerSlashings, proposerSlashingScale)
	}

	attesterSlashings := []scale.AttesterSlashing{}

	for _, attesterSlashing := range body.AttesterSlashings {
		attesterSlashingScale, err := attesterSlashing.ToScale()
		if err != nil {
			return scale.BeaconBlock{}, err
		}

		attesterSlashings = append(attesterSlashings, attesterSlashingScale)
	}

	attestations := []scale.Attestation{}

	for _, attestation := range body.Attestations {
		attestationScale, err := attestation.ToScale()
		if err != nil {
			return scale.BeaconBlock{}, err
		}

		attestations = append(attestations, attestationScale)
	}

	deposits := []scale.Deposit{}

	for _, deposit := range body.Deposits {
		depositScale, err := deposit.ToScale()
		if err != nil {
			return scale.BeaconBlock{}, err
		}

		deposits = append(deposits, depositScale)
	}

	voluntaryExits := []scale.SignedVoluntaryExit{}

	for _, voluntaryExit := range body.VoluntaryExits {
		voluntaryExitScale, err := voluntaryExit.ToScale()
		if err != nil {
			return scale.BeaconBlock{}, err
		}

		voluntaryExits = append(voluntaryExits, voluntaryExitScale)
	}

	depositCount, err := util.ToUint64(body.Eth1Data.DepositCount)
	if err != nil {
		return scale.BeaconBlock{}, err
	}

	executionPayload := body.ExecutionPayload

	baseFeePerGasUint64, err := util.ToUint64(executionPayload.BaseFeePerGas)
	if err != nil {
		return scale.BeaconBlock{}, err
	}

	bigInt := big.NewInt(int64(baseFeePerGasUint64))

	blockNumber, err := util.ToUint64(executionPayload.BlockNumber)
	if err != nil {
		return scale.BeaconBlock{}, err
	}

	gasLimit, err := util.ToUint64(executionPayload.GasLimit)
	if err != nil {
		return scale.BeaconBlock{}, err
	}

	gasUsed, err := util.ToUint64(executionPayload.GasUsed)
	if err != nil {
		return scale.BeaconBlock{}, err
	}

	timestamp, err := util.ToUint64(executionPayload.Timestamp)
	if err != nil {
		return scale.BeaconBlock{}, err
	}

	transactionsRoot, err := getTransactionsHashTreeRoot(executionPayload.Transactions)
	if err != nil {
		return scale.BeaconBlock{}, err
	}

	randaoReveal, err := util.HexStringToByteArray(body.RandaoReveal)
	if err != nil {
		return scale.BeaconBlock{}, err
	}

	feeRecipient, err := util.HexStringTo20Bytes(executionPayload.FeeRecipient)
	if err != nil {
		return scale.BeaconBlock{}, err
	}

	logsBloom, err := util.HexStringToByteArray(executionPayload.LogsBloom)
	if err != nil {
		return scale.BeaconBlock{}, err
	}

	extraData, err := util.HexStringToByteArray(executionPayload.ExtraData)
	if err != nil {
		return scale.BeaconBlock{}, err
	}

	return scale.BeaconBlock{
		Slot:          types.NewU64(slot),
		ProposerIndex: types.NewU64(proposerIndex),
		ParentRoot:    types.NewH256(common.HexToHash(dataMessage.ParentRoot).Bytes()),
		StateRoot:     types.NewH256(common.HexToHash(dataMessage.StateRoot).Bytes()),
		Body: scale.Body{
			RandaoReveal: randaoReveal,
			Eth1Data: scale.Eth1Data{
				DepositRoot:  types.NewH256(common.HexToHash(body.Eth1Data.DepositRoot).Bytes()),
				DepositCount: types.NewU64(depositCount),
				BlockHash:    types.NewH256(common.HexToHash(body.Eth1Data.BlockHash).Bytes()),
			},
			Graffiti:          types.NewH256(common.HexToHash(body.Graffiti).Bytes()),
			ProposerSlashings: proposerSlashings,
			AttesterSlashings: attesterSlashings,
			Attestations:      attestations,
			Deposits:          deposits,
			VoluntaryExits:    voluntaryExits,
			SyncAggregate:     syncAggregate,
			ExecutionPayload: scale.ExecutionPayloadHeaderCapella{
				ParentHash:       types.NewH256(common.HexToHash(executionPayload.ParentHash).Bytes()),
				FeeRecipient:     feeRecipient,
				StateRoot:        types.NewH256(common.HexToHash(executionPayload.StateRoot).Bytes()),
				ReceiptsRoot:     types.NewH256(common.HexToHash(executionPayload.ReceiptsRoot).Bytes()),
				LogsBloom:        logsBloom,
				PrevRandao:       types.NewH256(common.HexToHash(executionPayload.PrevRandao).Bytes()),
				BlockNumber:      types.NewU64(blockNumber),
				GasLimit:         types.NewU64(gasLimit),
				GasUsed:          types.NewU64(gasUsed),
				Timestamp:        types.NewU64(timestamp),
				ExtraData:        extraData,
				BaseFeePerGas:    types.NewU256(*bigInt),
				BlockHash:        types.NewH256(common.HexToHash(executionPayload.BlockHash).Bytes()),
				TransactionsRoot: transactionsRoot,
			},
		},
	}, nil
}

// ToFastSSZ can be removed once Lodestar supports returning block data as SSZ instead of JSON only.
// Because it only returns JSON, we need this interim step where we convert the block JSON to the data
// types that the FastSSZ lib expects. When Lodestar supports SSZ block response, we can remove all these
// and directly unmarshal SSZ bytes to state.BeaconBlock.
func (b BeaconBlockResponse) ToFastSSZ(activeSpec config.ActiveSpec, epoch uint64) (state.BeaconBlock, error) {
	data := b.Data.Message

	slot, err := util.ToUint64(data.Slot)
	if err != nil {
		return nil, err
	}

	proposerIndex, err := util.ToUint64(data.ProposerIndex)
	if err != nil {
		return nil, err
	}

	parentRoot, err := util.HexStringToByteArray(data.ParentRoot)
	if err != nil {
		return nil, err
	}

	stateRoot, err := util.HexStringToByteArray(data.StateRoot)
	if err != nil {
		return nil, err
	}

	body := data.Body

	randaoReveal, err := util.HexStringToByteArray(body.RandaoReveal)
	if err != nil {
		return nil, err
	}

	eth1DepositRoot, err := util.HexStringToByteArray(body.Eth1Data.DepositRoot)
	if err != nil {
		return nil, err
	}

	eth1DepositCount, err := util.ToUint64(body.Eth1Data.DepositCount)
	if err != nil {
		return nil, err
	}

	eth1BlockHash, err := util.HexStringToByteArray(body.Eth1Data.BlockHash)
	if err != nil {
		return nil, err
	}

	graffiti, err := util.HexStringTo32Bytes(body.Graffiti)
	if err != nil {
		return nil, err
	}

	proposerSlashings := []*state.ProposerSlashing{}
	for _, proposerSlashing := range body.ProposerSlashings {
		proposerSlashingSSZ, err := proposerSlashing.ToFastSSZ()
		if err != nil {
			return nil, err
		}

		proposerSlashings = append(proposerSlashings, proposerSlashingSSZ)
	}

	attesterSlashings := []*state.AttesterSlashing{}
	for _, attesterSlashing := range body.AttesterSlashings {
		attesterSlashingSSZ, err := attesterSlashing.ToFastSSZ()
		if err != nil {
			return nil, err
		}

		attesterSlashings = append(attesterSlashings, attesterSlashingSSZ)
	}

	attestations := []*state.Attestation{}
	for _, attestation := range body.Attestations {
		attestationSSZ, err := attestation.ToFastSSZ()
		if err != nil {
			return nil, err
		}

		attestations = append(attestations, attestationSSZ)
	}

	deposits := []*state.Deposit{}
	for _, deposit := range body.Deposits {
		depositScale, err := deposit.ToFastSSZ()
		if err != nil {
			return nil, err
		}

		deposits = append(deposits, depositScale)
	}

	voluntaryExits := []*state.SignedVoluntaryExit{}
	for _, voluntaryExit := range body.VoluntaryExits {
		voluntaryExitSSZ, err := voluntaryExit.ToFastSSZ()
		if err != nil {
			return nil, err
		}

		voluntaryExits = append(voluntaryExits, voluntaryExitSSZ)
	}

	executionPayload := body.ExecutionPayload
	parentHash, err := util.HexStringTo32Bytes(executionPayload.ParentHash)
	if err != nil {
		return nil, err
	}

	executionStateRoot, err := util.HexStringTo32Bytes(executionPayload.StateRoot)
	if err != nil {
		return nil, err
	}

	receiptsRoot, err := util.HexStringTo32Bytes(executionPayload.ReceiptsRoot)
	if err != nil {
		return nil, err
	}

	prevRando, err := util.HexStringTo32Bytes(executionPayload.PrevRandao)
	if err != nil {
		return nil, err
	}

	n := new(big.Int)
	n, ok := n.SetString(executionPayload.BaseFeePerGas, 10)
	if !ok {
		return nil, err
	}

	// FastSSZ expects a little endian byte array
	baseFeePerGas := util.ChangeByteOrder(n.Bytes())

	var baseFeePerGasBytes [32]byte
	copy(baseFeePerGasBytes[:], baseFeePerGas)

	blockHash, err := util.HexStringTo32Bytes(executionPayload.BlockHash)
	if err != nil {
		return nil, err
	}

	feeRecipient, err := util.HexStringTo20Bytes(executionPayload.FeeRecipient)
	if err != nil {
		return nil, err
	}

	logsBloom, err := util.HexStringTo256Bytes(executionPayload.LogsBloom)
	if err != nil {
		return nil, err
	}

	extraData, err := util.HexStringToByteArray(executionPayload.ExtraData)
	if err != nil {
		return nil, err
	}

	transactions := [][]byte{}
	for _, transaction := range executionPayload.Transactions {
		transactionSSZ, err := util.HexStringToByteArray(transaction)
		if err != nil {
			return nil, err
		}

		transactions = append(transactions, transactionSSZ)
	}

	withdrawals := []*state.Withdrawal{}
	for _, withdrawalResponse := range executionPayload.Withdrawals {
		withdrawalSSZ, err := withdrawalResponse.ToFastSSZ()
		if err != nil {
			return nil, err
		}
		withdrawals = append(withdrawals, withdrawalSSZ)
	}

	blockNumber, err := util.ToUint64(executionPayload.BlockNumber)
	if err != nil {
		return nil, err
	}

	gasLimit, err := util.ToUint64(executionPayload.GasLimit)
	if err != nil {
		return nil, err
	}

	gasUsed, err := util.ToUint64(executionPayload.GasUsed)
	if err != nil {
		return nil, err
	}

	timestamp, err := util.ToUint64(executionPayload.Timestamp)
	if err != nil {
		return nil, err
	}

	syncCommitteeBits, err := util.HexStringToByteArray(body.SyncAggregate.SyncCommitteeBits)
	if err != nil {
		return nil, err
	}

	syncCommitteeSignature, err := util.HexStringTo96Bytes(body.SyncAggregate.SyncCommitteeSignature)
	if err != nil {
		return nil, err
	}

	blsExecutionChanges := []*state.SignedBLSToExecutionChange{}

	for _, changeResponse := range body.BlsToExecutionChanges {
		changeSSZ, err := changeResponse.ToFastSSZ()
		if err != nil {
			return nil, err
		}
		blsExecutionChanges = append(blsExecutionChanges, changeSSZ)
	}

	if activeSpec == config.Minimal {
		return &state.BeaconBlockCapellaMinimal{
			Slot:          slot,
			ProposerIndex: proposerIndex,
			ParentRoot:    parentRoot,
			StateRoot:     stateRoot,
			Body: &state.BeaconBlockBodyCapellaMinimal{
				RandaoReveal: randaoReveal,
				Eth1Data: &state.Eth1Data{
					DepositRoot:  eth1DepositRoot,
					DepositCount: eth1DepositCount,
					BlockHash:    eth1BlockHash,
				},
				Graffiti:          graffiti,
				ProposerSlashings: proposerSlashings,
				AttesterSlashings: attesterSlashings,
				Attestations:      attestations,
				Deposits:          deposits,
				VoluntaryExits:    voluntaryExits,
				SyncAggregate: &state.SyncAggregateMinimal{
					SyncCommitteeBits:      syncCommitteeBits,
					SyncCommitteeSignature: syncCommitteeSignature,
				},
				ExecutionPayload: &state.ExecutionPayloadCapella{
					ParentHash:    parentHash,
					FeeRecipient:  feeRecipient,
					StateRoot:     executionStateRoot,
					ReceiptsRoot:  receiptsRoot,
					LogsBloom:     logsBloom,
					PrevRandao:    prevRando,
					BlockNumber:   blockNumber,
					GasLimit:      gasLimit,
					GasUsed:       gasUsed,
					Timestamp:     timestamp,
					ExtraData:     extraData,
					BaseFeePerGas: baseFeePerGasBytes,
					BlockHash:     blockHash,
					Transactions:  transactions,
					Withdrawals:   withdrawals, // new for Capella
				},
				BlsToExecutionChanges: blsExecutionChanges, // new for Capella
			},
		}, nil
	} else {
		return &state.BeaconBlockCapellaMainnet{
			Slot:          slot,
			ProposerIndex: proposerIndex,
			ParentRoot:    parentRoot,
			StateRoot:     stateRoot,
			Body: &state.BeaconBlockBodyCapellaMainnet{
				RandaoReveal: randaoReveal,
				Eth1Data: &state.Eth1Data{
					DepositRoot:  eth1DepositRoot,
					DepositCount: eth1DepositCount,
					BlockHash:    eth1BlockHash,
				},
				Graffiti:          graffiti,
				ProposerSlashings: proposerSlashings,
				AttesterSlashings: attesterSlashings,
				Attestations:      attestations,
				Deposits:          deposits,
				VoluntaryExits:    voluntaryExits,
				SyncAggregate: &state.SyncAggregateMainnet{
					SyncCommitteeBits:      syncCommitteeBits,
					SyncCommitteeSignature: syncCommitteeSignature,
				},
				ExecutionPayload: &state.ExecutionPayloadCapella{
					ParentHash:    parentHash,
					FeeRecipient:  feeRecipient,
					StateRoot:     executionStateRoot,
					ReceiptsRoot:  receiptsRoot,
					LogsBloom:     logsBloom,
					PrevRandao:    prevRando,
					BlockNumber:   blockNumber,
					GasLimit:      gasLimit,
					GasUsed:       gasUsed,
					Timestamp:     timestamp,
					ExtraData:     extraData,
					BaseFeePerGas: baseFeePerGasBytes,
					BlockHash:     blockHash,
					Transactions:  transactions,
					Withdrawals:   withdrawals, // new for Capella
				},
				BlsToExecutionChanges: blsExecutionChanges, // new for Capella
			},
		}, nil
	}
}

func (p ProposerSlashingResponse) ToScale() (scale.ProposerSlashing, error) {
	signedHeader1, err := p.SignedHeader1.ToScale()
	if err != nil {
		return scale.ProposerSlashing{}, err
	}

	signedHeader2, err := p.SignedHeader2.ToScale()
	if err != nil {
		return scale.ProposerSlashing{}, err
	}

	return scale.ProposerSlashing{
		SignedHeader1: signedHeader1,
		SignedHeader2: signedHeader2,
	}, nil
}

func (p ProposerSlashingResponse) ToFastSSZ() (*state.ProposerSlashing, error) {
	signedHeader1, err := p.SignedHeader1.ToFastSSZ()
	if err != nil {
		return nil, err
	}

	signedHeader2, err := p.SignedHeader2.ToFastSSZ()
	if err != nil {
		return nil, err
	}

	return &state.ProposerSlashing{
		Header1: signedHeader1,
		Header2: signedHeader2,
	}, nil
}

func (a AttesterSlashingResponse) ToScale() (scale.AttesterSlashing, error) {
	attestation1, err := a.Attestation1.ToScale()
	if err != nil {
		return scale.AttesterSlashing{}, err
	}

	attestation2, err := a.Attestation2.ToScale()
	if err != nil {
		return scale.AttesterSlashing{}, err
	}

	return scale.AttesterSlashing{
		Attestation1: attestation1,
		Attestation2: attestation2,
	}, nil
}

func (a AttesterSlashingResponse) ToFastSSZ() (*state.AttesterSlashing, error) {
	attestation1, err := a.Attestation1.ToFastSSZ()
	if err != nil {
		return nil, err
	}

	attestation2, err := a.Attestation2.ToFastSSZ()
	if err != nil {
		return nil, err
	}

	return &state.AttesterSlashing{
		Attestation1: attestation1,
		Attestation2: attestation2,
	}, nil
}

func (a AttestationResponse) ToScale() (scale.Attestation, error) {
	data, err := a.Data.ToScale()
	if err != nil {
		return scale.Attestation{}, err
	}

	aggregationBits, err := util.HexStringToByteArray(a.AggregationBits)
	if err != nil {
		return scale.Attestation{}, err
	}

	signature, err := util.HexStringToByteArray(a.Signature)
	if err != nil {
		return scale.Attestation{}, err
	}

	return scale.Attestation{
		AggregationBits: aggregationBits,
		Data:            data,
		Signature:       signature,
	}, nil
}

func (a AttestationResponse) ToFastSSZ() (*state.Attestation, error) {
	data, err := a.Data.ToFastSSZ()
	if err != nil {
		return nil, err
	}

	aggregationBits, err := util.HexStringToByteArray(a.AggregationBits)
	if err != nil {
		return nil, err
	}

	signature, err := util.HexStringTo96Bytes(a.Signature)
	if err != nil {
		return nil, err
	}

	return &state.Attestation{
		AggregationBits: aggregationBits,
		Data:            data,
		Signature:       signature,
	}, nil
}

func (d SignedVoluntaryExitResponse) ToScale() (scale.SignedVoluntaryExit, error) {
	epoch, err := util.ToUint64(d.Message.Epoch)
	if err != nil {
		return scale.SignedVoluntaryExit{}, err
	}

	validaterIndex, err := util.ToUint64(d.Message.ValidatorIndex)
	if err != nil {
		return scale.SignedVoluntaryExit{}, err
	}

	signature, err := util.HexStringToByteArray(d.Signature)
	if err != nil {
		return scale.SignedVoluntaryExit{}, err
	}

	return scale.SignedVoluntaryExit{
		Exit: scale.VoluntaryExit{
			Epoch:          types.NewU64(epoch),
			ValidaterIndex: types.NewU64(validaterIndex),
		},
		Signature: signature,
	}, nil
}

func (d SignedVoluntaryExitResponse) ToFastSSZ() (*state.SignedVoluntaryExit, error) {
	epoch, err := util.ToUint64(d.Message.Epoch)
	if err != nil {
		return nil, err
	}

	validaterIndex, err := util.ToUint64(d.Message.ValidatorIndex)
	if err != nil {
		return nil, err
	}

	signature, err := util.HexStringTo96Bytes(d.Signature)
	if err != nil {
		return nil, err
	}

	return &state.SignedVoluntaryExit{
		Exit: &state.VoluntaryExit{
			Epoch:          epoch,
			ValidatorIndex: validaterIndex,
		},
		Signature: signature,
	}, nil
}

func (d DepositResponse) ToScale() (scale.Deposit, error) {
	proofs := []types.H256{}

	for _, proofData := range d.Proof {
		proofs = append(proofs, types.NewH256(common.HexToHash(proofData).Bytes()))
	}

	amount, err := util.ToUint64(d.Data.Amount)
	if err != nil {
		return scale.Deposit{}, err
	}

	pubkey, err := util.HexStringToByteArray(d.Data.Pubkey)
	if err != nil {
		return scale.Deposit{}, err
	}

	signature, err := util.HexStringToByteArray(d.Data.Signature)
	if err != nil {
		return scale.Deposit{}, err
	}

	return scale.Deposit{
		Proof: proofs,
		Data: scale.DepositData{
			Pubkey:                pubkey,
			WithdrawalCredentials: types.NewH256(common.HexToHash(d.Data.WithdrawalCredentials).Bytes()),
			Amount:                types.NewU64(amount),
			Signature:             signature,
		},
	}, nil
}

func (d DepositResponse) ToFastSSZ() (*state.Deposit, error) {
	proofs := [][]byte{}
	for _, proofData := range d.Proof {
		proofs = append(proofs, common.HexToHash(proofData).Bytes())
	}

	amount, err := util.ToUint64(d.Data.Amount)
	if err != nil {
		return nil, err
	}

	pubkey, err := util.HexStringToPublicKey(d.Data.Pubkey)
	if err != nil {
		return nil, err
	}

	signature, err := util.HexStringToByteArray(d.Data.Signature)
	if err != nil {
		return nil, err
	}

	withdrawalCredentials, err := util.HexStringTo32Bytes(d.Data.WithdrawalCredentials)
	if err != nil {
		return nil, err
	}

	return &state.Deposit{
		Proof: proofs,
		Data: &state.DepositData{
			Pubkey:                pubkey,
			WithdrawalCredentials: withdrawalCredentials,
			Amount:                amount,
			Signature:             signature,
		},
	}, nil
}

func (s SignedHeaderResponse) ToScale() (scale.SignedHeader, error) {
	message, err := s.Message.ToScale()
	if err != nil {
		return scale.SignedHeader{}, err
	}
	signature, err := util.HexStringToByteArray(s.Signature)
	if err != nil {
		return scale.SignedHeader{}, err
	}

	return scale.SignedHeader{
		Message:   message,
		Signature: signature,
	}, nil
}

func (s SignedHeaderResponse) ToFastSSZ() (*state.SignedBeaconBlockHeader, error) {
	message, err := s.Message.ToFastSSZ()
	if err != nil {
		return nil, err
	}
	signature, err := util.HexStringToByteArray(s.Signature)
	if err != nil {
		return nil, err
	}

	return &state.SignedBeaconBlockHeader{
		Header:    message,
		Signature: signature,
	}, nil
}

func (i IndexedAttestationResponse) ToScale() (scale.IndexedAttestation, error) {
	data, err := i.Data.ToScale()
	if err != nil {
		return scale.IndexedAttestation{}, err
	}

	attestationIndexes := []types.U64{}

	for _, index := range i.AttestingIndices {
		indexInt, err := util.ToUint64(index)
		if err != nil {
			return scale.IndexedAttestation{}, err
		}

		attestationIndexes = append(attestationIndexes, types.NewU64(indexInt))
	}

	signature, err := util.HexStringToByteArray(i.Signature)
	if err != nil {
		return scale.IndexedAttestation{}, err
	}

	return scale.IndexedAttestation{
		AttestingIndices: attestationIndexes,
		Data:             data,
		Signature:        signature,
	}, nil
}

func (i IndexedAttestationResponse) ToFastSSZ() (*state.IndexedAttestation, error) {
	data, err := i.Data.ToFastSSZ()
	if err != nil {
		return nil, err
	}

	attestationIndexes := []uint64{}
	for _, index := range i.AttestingIndices {
		indexInt, err := util.ToUint64(index)
		if err != nil {
			return nil, err
		}

		attestationIndexes = append(attestationIndexes, indexInt)
	}

	signature, err := util.HexStringToByteArray(i.Signature)
	if err != nil {
		return nil, err
	}

	return &state.IndexedAttestation{
		AttestationIndices: attestationIndexes,
		Data:               data,
		Signature:          signature,
	}, nil
}

func (a AttestationDataResponse) ToScale() (scale.AttestationData, error) {
	slot, err := util.ToUint64(a.Slot)
	if err != nil {
		return scale.AttestationData{}, err
	}

	index, err := util.ToUint64(a.Index)
	if err != nil {
		return scale.AttestationData{}, err
	}

	source, err := a.Source.ToScale()
	if err != nil {
		return scale.AttestationData{}, err
	}

	target, err := a.Target.ToScale()
	if err != nil {
		return scale.AttestationData{}, err
	}

	return scale.AttestationData{
		Slot:            types.NewU64(slot),
		Index:           types.NewU64(index),
		BeaconBlockRoot: types.NewH256(common.HexToHash(a.BeaconBlockRoot).Bytes()),
		Source:          source,
		Target:          target,
	}, nil
}

func (a AttestationDataResponse) ToFastSSZ() (*state.AttestationData, error) {
	slot, err := util.ToUint64(a.Slot)
	if err != nil {
		return nil, err
	}

	index, err := util.ToUint64(a.Index)
	if err != nil {
		return nil, err
	}

	source, err := a.Source.ToFastSSZ()
	if err != nil {
		return nil, err
	}

	target, err := a.Target.ToFastSSZ()
	if err != nil {
		return nil, err
	}

	hash, err := util.HexStringTo32Bytes(a.BeaconBlockRoot)
	if err != nil {
		return nil, err
	}

	return &state.AttestationData{
		Slot:            state.Slot(slot),
		Index:           index,
		BeaconBlockHash: hash,
		Source:          source,
		Target:          target,
	}, nil
}

func (c CheckpointResponse) ToScale() (scale.Checkpoint, error) {
	epoch, err := util.ToUint64(c.Epoch)
	if err != nil {
		return scale.Checkpoint{}, err
	}

	return scale.Checkpoint{
		Epoch: types.NewU64(epoch),
		Root:  types.NewH256(common.HexToHash(c.Root).Bytes()),
	}, nil
}

func (c CheckpointResponse) ToFastSSZ() (*state.Checkpoint, error) {
	epoch, err := util.ToUint64(c.Epoch)
	if err != nil {
		return nil, err
	}

	return &state.Checkpoint{
		Epoch: epoch,
		Root:  common.HexToHash(c.Root).Bytes(),
	}, nil
}

func getTransactionsHashTreeRoot(transactions []string) (types.H256, error) {
	resultTransactions := [][]byte{}

	for _, trans := range transactions {
		decodeString, err := hex.DecodeString(strings.ReplaceAll(trans, "0x", ""))
		if err != nil {
			return types.H256{}, err
		}
		resultTransactions = append(resultTransactions, decodeString)
	}

	transactionsContainer := state.TransactionsRootContainer{}
	transactionsContainer.Transactions = resultTransactions

	transactionsRoot, err := transactionsContainer.HashTreeRoot()
	if err != nil {
		return types.H256{}, err
	}

	return types.NewH256(transactionsRoot[:]), nil
}

func CapellaExecutionPayloadToScale(e *state.ExecutionPayloadCapella, activeSpec config.ActiveSpec) (scale.ExecutionPayloadHeaderCapella, error) {
	transactionsContainer := state.TransactionsRootContainer{}
	transactionsContainer.Transactions = e.Transactions

	transactionsRoot, err := transactionsContainer.HashTreeRoot()
	if err != nil {
		return scale.ExecutionPayloadHeaderCapella{}, err
	}

	var withdrawalRoot types.H256

	if activeSpec == config.Minimal {
		withdrawalContainer := state.WithdrawalsRootContainerMinimal{}
		withdrawalContainer.Withdrawals = e.Withdrawals
		withdrawalRoot, err = withdrawalContainer.HashTreeRoot()
	} else {
		withdrawalContainer := state.WithdrawalsRootContainerMainnet{}
		withdrawalContainer.Withdrawals = e.Withdrawals
		withdrawalRoot, err = withdrawalContainer.HashTreeRoot()
	}
	if err != nil {
		return scale.ExecutionPayloadHeaderCapella{}, err
	}

	baseFeePerGas := big.Int{}
	// Change BaseFeePerGas back from little-endian to big-endian
	baseFeePerGas.SetBytes(util.ChangeByteOrder(e.BaseFeePerGas[:]))

	return scale.ExecutionPayloadHeaderCapella{
		ParentHash:       types.NewH256(e.ParentHash[:]),
		FeeRecipient:     e.FeeRecipient,
		StateRoot:        types.NewH256(e.StateRoot[:]),
		ReceiptsRoot:     types.NewH256(e.ReceiptsRoot[:]),
		LogsBloom:        e.LogsBloom[:],
		PrevRandao:       types.NewH256(e.PrevRandao[:]),
		BlockNumber:      types.NewU64(e.BlockNumber),
		GasLimit:         types.NewU64(e.GasLimit),
		GasUsed:          types.NewU64(e.GasUsed),
		Timestamp:        types.NewU64(e.Timestamp),
		ExtraData:        e.ExtraData,
		BaseFeePerGas:    types.NewU256(baseFeePerGas),
		BlockHash:        types.NewH256(e.BlockHash[:]),
		TransactionsRoot: transactionsRoot,
		WithdrawalsRoot:  withdrawalRoot,
	}, nil
}

func SyncAggregateToScale(s state.SyncAggregate) scale.SyncAggregate {
	aggregateSignature := s.GetSyncAggregateSignature()
	return scale.SyncAggregate{
		SyncCommitteeBits:      s.GetSyncAggregateBits(),
		SyncCommitteeSignature: aggregateSignature,
	}
}
