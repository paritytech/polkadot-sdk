package state

import (
	ssz "github.com/ferranbt/fastssz"
)

type Checkpoint struct {
	Epoch uint64 `json:"epoch"`
	Root  []byte `json:"root" ssz-size:"32"`
}

type Slot uint64 // alias from the same package

type Hash [32]byte

type AttestationData struct {
	Slot            Slot        `json:"slot"`
	Index           uint64      `json:"index"`
	BeaconBlockHash Hash        `json:"beacon_block_root" ssz-size:"32"`
	Source          *Checkpoint `json:"source"`
	Target          *Checkpoint `json:"target"`
}

type Attestation struct {
	AggregationBits []byte           `json:"aggregation_bits" ssz:"bitlist" ssz-max:"2048"`
	Data            *AttestationData `json:"data"`
	Signature       [96]byte         `json:"signature" ssz-size:"96"`
}

type DepositData struct {
	Pubkey                [48]byte `json:"pubkey" ssz-size:"48"`
	WithdrawalCredentials [32]byte `json:"withdrawal_credentials" ssz-size:"32"`
	Amount                uint64   `json:"amount"`
	Signature             []byte   `json:"signature" ssz-size:"96"`
	Root                  [32]byte `ssz:"-"`
}

type Deposit struct {
	Proof [][]byte `ssz-size:"33,32"`
	Data  *DepositData
}

type IndexedAttestation struct {
	AttestationIndices []uint64         `json:"attesting_indices" ssz-max:"2048"`
	Data               *AttestationData `json:"data"`
	Signature          []byte           `json:"signature" ssz-size:"96"`
}

type Fork struct {
	PreviousVersion []byte `json:"previous_version" ssz-size:"4"`
	CurrentVersion  []byte `json:"current_version" ssz-size:"4"`
	Epoch           uint64 `json:"epoch"`
}

type Validator struct {
	Pubkey                     []byte `json:"pubkey" ssz-size:"48"`
	WithdrawalCredentials      []byte `json:"withdrawal_credentials" ssz-size:"32"`
	EffectiveBalance           uint64 `json:"effective_balance"`
	Slashed                    bool   `json:"slashed"`
	ActivationEligibilityEpoch uint64 `json:"activation_eligibility_epoch"`
	ActivationEpoch            uint64 `json:"activation_epoch"`
	ExitEpoch                  uint64 `json:"exit_epoch"`
	WithdrawableEpoch          uint64 `json:"withdrawable_epoch"`
}

type VoluntaryExit struct {
	Epoch          uint64 `json:"epoch"`
	ValidatorIndex uint64 `json:"validator_index"`
}

type SignedVoluntaryExit struct {
	Exit      *VoluntaryExit `json:"message"`
	Signature [96]byte       `json:"signature" ssz-size:"96"`
}

type Eth1Data struct {
	DepositRoot  []byte `json:"deposit_root" ssz-size:"32"`
	DepositCount uint64 `json:"deposit_count"`
	BlockHash    []byte `json:"block_hash" ssz-size:"32"`
}

type ProposerSlashing struct {
	Header1 *SignedBeaconBlockHeader `json:"signed_header_1"`
	Header2 *SignedBeaconBlockHeader `json:"signed_header_2"`
}

type AttesterSlashing struct {
	Attestation1 *IndexedAttestation `json:"attestation_1"`
	Attestation2 *IndexedAttestation `json:"attestation_2"`
}

type BlockRootsContainerMainnet struct {
	BlockRoots [][]byte `json:"block_roots" ssz-size:"8192,32"`
}

type BlockRootsContainerMinimal struct {
	BlockRoots [][]byte `json:"block_roots" ssz-size:"64,32"`
}

type TransactionsRootContainer struct {
	Transactions [][]byte `ssz-max:"1048576,1073741824" ssz-size:"?,?" json:"transactions"`
}

type SignedBeaconBlockHeader struct {
	Header    *BeaconBlockHeader `json:"message"`
	Signature []byte             `json:"signature" ssz-size:"96"`
}

type BeaconBlockHeader struct {
	Slot          uint64 `json:"slot"`
	ProposerIndex uint64 `json:"proposer_index"`
	ParentRoot    []byte `json:"parent_root" ssz-size:"32"`
	StateRoot     []byte `json:"state_root" ssz-size:"32"`
	BodyRoot      []byte `json:"body_root" ssz-size:"32"`
}

type SyncCommitteeMinimal struct {
	PubKeys         [][]byte `json:"pubkeys" ssz-size:"32,48"`
	AggregatePubKey [48]byte `json:"aggregate_pubkey" ssz-size:"48"`
}

type SyncCommittee struct {
	PubKeys         [][]byte `json:"pubkeys" ssz-size:"512,48"`
	AggregatePubKey [48]byte `json:"aggregate_pubkey" ssz-size:"48"`
}

type SyncAggregateMainnet struct {
	SyncCommitteeBits      []byte   `json:"sync_committee_bits" ssz-size:"64"`
	SyncCommitteeSignature [96]byte `json:"sync_committee_signature" ssz-size:"96"`
}

type SyncAggregateMinimal struct {
	SyncCommitteeBits      []byte   `json:"sync_committee_bits" ssz-size:"4"`
	SyncCommitteeSignature [96]byte `json:"sync_committee_signature" ssz-size:"96"`
}

// Capella structures
type ExecutionPayloadCapella struct {
	ParentHash    [32]byte      `ssz-size:"32" json:"parent_hash"`
	FeeRecipient  [20]byte      `ssz-size:"20" json:"fee_recipient"`
	StateRoot     [32]byte      `ssz-size:"32" json:"state_root"`
	ReceiptsRoot  [32]byte      `ssz-size:"32" json:"receipts_root"`
	LogsBloom     [256]byte     `ssz-size:"256" json:"logs_bloom"`
	PrevRandao    [32]byte      `ssz-size:"32" json:"prev_randao"`
	BlockNumber   uint64        `json:"block_number"`
	GasLimit      uint64        `json:"gas_limit"`
	GasUsed       uint64        `json:"gas_used"`
	Timestamp     uint64        `json:"timestamp"`
	ExtraData     []byte        `ssz-max:"32" json:"extra_data"`
	BaseFeePerGas [32]byte      `ssz-size:"32" json:"base_fee_per_gas"`
	BlockHash     [32]byte      `ssz-size:"32" json:"block_hash"`
	Transactions  [][]byte      `ssz-max:"1048576,1073741824" ssz-size:"?,?" json:"transactions"`
	Withdrawals   []*Withdrawal `ssz-max:"16" json:"withdrawals"`
}

type ExecutionPayloadHeaderCapella struct {
	ParentHash       []byte `json:"parent_hash" ssz-size:"32"`
	FeeRecipient     []byte `json:"fee_recipient" ssz-size:"20"`
	StateRoot        []byte `json:"state_root" ssz-size:"32"`
	ReceiptsRoot     []byte `json:"receipts_root" ssz-size:"32"`
	LogsBloom        []byte `json:"logs_bloom" ssz-size:"256"`
	PrevRandao       []byte `json:"prev_randao" ssz-size:"32"`
	BlockNumber      uint64 `json:"block_number"`
	GasLimit         uint64 `json:"gas_limit"`
	GasUsed          uint64 `json:"gas_used"`
	Timestamp        uint64 `json:"timestamp"`
	ExtraData        []byte `json:"extra_data" ssz-max:"32"`
	BaseFeePerGas    []byte `json:"base_fee_per_gas" ssz-size:"32"`
	BlockHash        []byte `json:"block_hash" ssz-size:"32"`
	TransactionsRoot []byte `json:"transactions_root" ssz-size:"32"`
	WithdrawalsRoot  []byte `json:"withdrawals_root" ssz-size:"32"`
}

type BeaconState interface {
	UnmarshalSSZ(buf []byte) error
	GetSlot() uint64
	GetLatestBlockHeader() *BeaconBlockHeader
	GetBlockRoots() [][]byte
	GetTree() (*ssz.Node, error)
}

type SyncAggregate interface {
	GetSyncAggregateBits() []byte
	GetSyncAggregateSignature() [96]byte
}

type BeaconBlockBody interface {
	GetTree() (*ssz.Node, error)
}

type BlockRootsContainer interface {
	GetTree() (*ssz.Node, error)
	SetBlockRoots(blockRoots [][]byte)
}

type BeaconBlock interface {
	UnmarshalSSZ(buf []byte) error
	GetBeaconSlot() uint64
	GetExecutionPayload() *ExecutionPayloadCapella
	GetTree() (*ssz.Node, error)
	GetBlockBodyTree() (*ssz.Node, error)
}

func (b *BlockRootsContainerMainnet) SetBlockRoots(blockRoots [][]byte) {
	b.BlockRoots = blockRoots
}

func (b *BlockRootsContainerMinimal) SetBlockRoots(blockRoots [][]byte) {
	b.BlockRoots = blockRoots
}

func (s *SyncAggregateMinimal) GetSyncAggregateBits() []byte {
	return s.SyncCommitteeBits
}

func (s *SyncAggregateMinimal) GetSyncAggregateSignature() [96]byte {
	return s.SyncCommitteeSignature
}

func (s *SyncAggregateMainnet) GetSyncAggregateBits() []byte {
	return s.SyncCommitteeBits
}

func (s *SyncAggregateMainnet) GetSyncAggregateSignature() [96]byte {
	return s.SyncCommitteeSignature
}

type BLSToExecutionChange struct {
	ValidatorIndex     uint64 `json:"validator_index,omitempty"`
	FromBlsPubkey      []byte `json:"from_bls_pubkey,omitempty" ssz-size:"48"`
	ToExecutionAddress []byte `json:"to_execution_address,omitempty" ssz-size:"20"`
}

type SignedBLSToExecutionChange struct {
	Message   *BLSToExecutionChange `json:"message,omitempty"`
	Signature []byte                `json:"signature,omitempty" ssz-size:"96"`
}

type BeaconBlockCapellaMinimal struct {
	Slot          uint64                         `json:"slot"`
	ProposerIndex uint64                         `json:"proposer_index"`
	ParentRoot    []byte                         `json:"parent_root" ssz-size:"32"`
	StateRoot     []byte                         `json:"state_root" ssz-size:"32"`
	Body          *BeaconBlockBodyCapellaMinimal `json:"body"`
}

type BeaconBlockCapellaMainnet struct {
	Slot          uint64                         `json:"slot"`
	ProposerIndex uint64                         `json:"proposer_index"`
	ParentRoot    []byte                         `json:"parent_root" ssz-size:"32"`
	StateRoot     []byte                         `json:"state_root" ssz-size:"32"`
	Body          *BeaconBlockBodyCapellaMainnet `json:"body"`
}

type BeaconBlockBodyCapellaMinimal struct {
	RandaoReveal          []byte                        `json:"randao_reveal" ssz-size:"96"`
	Eth1Data              *Eth1Data                     `json:"eth1_data"`
	Graffiti              [32]byte                      `json:"graffiti" ssz-size:"32"`
	ProposerSlashings     []*ProposerSlashing           `json:"proposer_slashings" ssz-max:"16"`
	AttesterSlashings     []*AttesterSlashing           `json:"attester_slashings" ssz-max:"2"`
	Attestations          []*Attestation                `json:"attestations" ssz-max:"128"`
	Deposits              []*Deposit                    `json:"deposits" ssz-max:"16"`
	VoluntaryExits        []*SignedVoluntaryExit        `json:"voluntary_exits" ssz-max:"16"`
	SyncAggregate         *SyncAggregateMinimal         `json:"sync_aggregate"`
	ExecutionPayload      *ExecutionPayloadCapella      `json:"execution_payload"`
	BlsToExecutionChanges []*SignedBLSToExecutionChange `json:"bls_to_execution_changes,omitempty" ssz-max:"16"`
}

type BeaconBlockBodyCapellaMainnet struct {
	RandaoReveal          []byte                        `json:"randao_reveal" ssz-size:"96"`
	Eth1Data              *Eth1Data                     `json:"eth1_data"`
	Graffiti              [32]byte                      `json:"graffiti" ssz-size:"32"`
	ProposerSlashings     []*ProposerSlashing           `json:"proposer_slashings" ssz-max:"16"`
	AttesterSlashings     []*AttesterSlashing           `json:"attester_slashings" ssz-max:"2"`
	Attestations          []*Attestation                `json:"attestations" ssz-max:"128"`
	Deposits              []*Deposit                    `json:"deposits" ssz-max:"16"`
	VoluntaryExits        []*SignedVoluntaryExit        `json:"voluntary_exits" ssz-max:"16"`
	SyncAggregate         *SyncAggregateMainnet         `json:"sync_aggregate"`
	ExecutionPayload      *ExecutionPayloadCapella      `json:"execution_payload"`
	BlsToExecutionChanges []*SignedBLSToExecutionChange `json:"bls_to_execution_changes,omitempty" ssz-max:"16"`
}

type BeaconStateCapellaMainnet struct {
	GenesisTime                  uint64                         `json:"genesis_time"`
	GenesisValidatorsRoot        []byte                         `json:"genesis_validators_root" ssz-size:"32"`
	Slot                         uint64                         `json:"slot"`
	Fork                         *Fork                          `json:"fork"`
	LatestBlockHeader            *BeaconBlockHeader             `json:"latest_block_header"`
	BlockRoots                   [][]byte                       `json:"block_roots" ssz-size:"8192,32"`
	StateRoots                   [][]byte                       `json:"state_roots" ssz-size:"8192,32"`
	HistoricalRoots              [][]byte                       `json:"historical_roots" ssz-max:"16777216" ssz-size:"?,32"`
	Eth1Data                     *Eth1Data                      `json:"eth1_data"`
	Eth1DataVotes                []*Eth1Data                    `json:"eth1_data_votes" ssz-max:"2048"`
	Eth1DepositIndex             uint64                         `json:"eth1_deposit_index"`
	Validators                   []*Validator                   `json:"validators" ssz-max:"1099511627776"`
	Balances                     []uint64                       `json:"balances" ssz-max:"1099511627776"`
	RandaoMixes                  [][]byte                       `json:"randao_mixes" ssz-size:"65536,32"`
	Slashings                    []uint64                       `json:"slashings" ssz-size:"8192"`
	PreviousEpochParticipation   []byte                         `json:"previous_epoch_participation" ssz-max:"1099511627776"`
	CurrentEpochParticipation    []byte                         `json:"current_epoch_participation" ssz-max:"1099511627776"`
	JustificationBits            []byte                         `json:"justification_bits" cast-type:"github.com/prysmaticlabs/go-bitfield.Bitvector4" ssz-size:"1"`
	PreviousJustifiedCheckpoint  *Checkpoint                    `json:"previous_justified_checkpoint"`
	CurrentJustifiedCheckpoint   *Checkpoint                    `json:"current_justified_checkpoint"`
	FinalizedCheckpoint          *Checkpoint                    `json:"finalized_checkpoint"`
	InactivityScores             []uint64                       `json:"inactivity_scores" ssz-max:"1099511627776"`
	CurrentSyncCommittee         *SyncCommittee                 `json:"current_sync_committee"`
	NextSyncCommittee            *SyncCommittee                 `json:"next_sync_committee"`
	LatestExecutionPayloadHeader *ExecutionPayloadHeaderCapella `json:"latest_execution_payload_header"`
	NextWithdrawalIndex          uint64                         `json:"next_withdrawal_index,omitempty"`
	NextWithdrawalValidatorIndex uint64                         `json:"next_withdrawal_validator_index,omitempty"`
	HistoricalSummaries          []*HistoricalSummary           `json:"historical_summaries,omitempty" ssz-max:"16777216"`
}

type HistoricalSummary struct {
	BlockSummaryRoot []byte `json:"block_summary_root,omitempty" ssz-size:"32"`
	StateSummaryRoot []byte `json:"state_summary_root,omitempty" ssz-size:"32"`
}

type BeaconStateCapellaMinimal struct {
	GenesisTime                  uint64                         `json:"genesis_time"`
	GenesisValidatorsRoot        []byte                         `json:"genesis_validators_root" ssz-size:"32"`
	Slot                         uint64                         `json:"slot"`
	Fork                         *Fork                          `json:"fork"`
	LatestBlockHeader            *BeaconBlockHeader             `json:"latest_block_header"`
	BlockRoots                   [][]byte                       `json:"block_roots" ssz-size:"64,32"`
	StateRoots                   [][]byte                       `json:"state_roots" ssz-size:"64,32"`
	HistoricalRoots              [][]byte                       `json:"historical_roots" ssz-max:"16777216" ssz-size:"?,32"`
	Eth1Data                     *Eth1Data                      `json:"eth1_data"`
	Eth1DataVotes                []*Eth1Data                    `json:"eth1_data_votes" ssz-max:"32"`
	Eth1DepositIndex             uint64                         `json:"eth1_deposit_index"`
	Validators                   []*Validator                   `json:"validators" ssz-max:"1099511627776"`
	Balances                     []uint64                       `json:"balances" ssz-max:"1099511627776"`
	RandaoMixes                  [][]byte                       `json:"randao_mixes" ssz-size:"64,32"`
	Slashings                    []uint64                       `json:"slashings" ssz-size:"64"`
	PreviousEpochParticipation   []byte                         `json:"previous_epoch_participation" ssz-max:"1099511627776"`
	CurrentEpochParticipation    []byte                         `json:"current_epoch_participation" ssz-max:"1099511627776"`
	JustificationBits            []byte                         `json:"justification_bits" ssz-size:"1"`
	PreviousJustifiedCheckpoint  *Checkpoint                    `json:"previous_justified_checkpoint"`
	CurrentJustifiedCheckpoint   *Checkpoint                    `json:"current_justified_checkpoint"`
	FinalizedCheckpoint          *Checkpoint                    `json:"finalized_checkpoint"`
	InactivityScores             []uint64                       `json:"inactivity_scores" ssz-max:"1099511627776"`
	CurrentSyncCommittee         *SyncCommitteeMinimal          `json:"current_sync_committee"`
	NextSyncCommittee            *SyncCommitteeMinimal          `json:"next_sync_committee"`
	LatestExecutionPayloadHeader *ExecutionPayloadHeaderCapella `json:"latest_execution_payload_header"`
	NextWithdrawalIndex          uint64                         `json:"next_withdrawal_index,omitempty"`
	NextWithdrawalValidatorIndex uint64                         `json:"next_withdrawal_validator_index,omitempty"`
	HistoricalSummaries          []*HistoricalSummary           `json:"historical_summaries,omitempty" ssz-max:"16777216"`
}

type Withdrawal struct {
	Index          uint64   `json:"index"`
	ValidatorIndex uint64   `json:"validator_index"`
	Address        [20]byte `json:"address" ssz-size:"20"`
	Amount         uint64   `json:"amount"`
}

type WithdrawalsRootContainerMinimal struct {
	Withdrawals []*Withdrawal `ssz-max:"4" json:"withdrawals"`
}

type WithdrawalsRootContainerMainnet struct {
	Withdrawals []*Withdrawal `ssz-max:"16" json:"withdrawals"`
}

func (b *BeaconBlockCapellaMinimal) GetBeaconSlot() uint64 {
	return b.Slot
}

func (b *BeaconBlockCapellaMinimal) GetExecutionPayload() *ExecutionPayloadCapella {
	return b.Body.ExecutionPayload
}

func (b *BeaconBlockCapellaMinimal) GetBlockBodyTree() (*ssz.Node, error) {
	return b.Body.GetTree()
}

func (b *BeaconBlockCapellaMainnet) GetBeaconSlot() uint64 {
	return b.Slot
}

func (b *BeaconBlockCapellaMainnet) GetExecutionPayload() *ExecutionPayloadCapella {
	return b.Body.ExecutionPayload
}

func (b *BeaconBlockCapellaMainnet) GetBlockBodyTree() (*ssz.Node, error) {
	return b.Body.GetTree()
}

func (b *BeaconStateCapellaMinimal) GetSlot() uint64 {
	return b.Slot
}

func (b *BeaconStateCapellaMinimal) GetLatestBlockHeader() *BeaconBlockHeader {
	return b.LatestBlockHeader
}

func (b *BeaconStateCapellaMinimal) GetBlockRoots() [][]byte {
	return b.BlockRoots
}

func (b *BeaconStateCapellaMainnet) GetSlot() uint64 {
	return b.Slot
}

func (b *BeaconStateCapellaMainnet) GetLatestBlockHeader() *BeaconBlockHeader {
	return b.LatestBlockHeader
}

func (b *BeaconStateCapellaMainnet) GetBlockRoots() [][]byte {
	return b.BlockRoots
}

func (b *BeaconStateCapellaMainnet) SetBlockRoots(blockRoots [][]byte) {
	b.BlockRoots = blockRoots
}
