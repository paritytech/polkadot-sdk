package cmd

import (
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"time"

	"github.com/cbroglie/mustache"
	"github.com/snowfork/go-substrate-rpc-client/v4/types"

	log "github.com/sirupsen/logrus"
	"github.com/snowfork/snowbridge/relayer/relays/beacon/cache"
	"github.com/snowfork/snowbridge/relayer/relays/beacon/config"
	"github.com/snowfork/snowbridge/relayer/relays/beacon/header/syncer"
	beaconjson "github.com/snowfork/snowbridge/relayer/relays/beacon/header/syncer/json"
	"github.com/spf13/cobra"
	"github.com/spf13/viper"
)

func generateBeaconDataCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "generate-beacon-data",
		Short: "Generate beacon data.",
		Args:  cobra.ExactArgs(0),
		RunE:  generateBeaconData,
	}

	cmd.Flags().String("spec", "", "Valid values are mainnet or minimal")
	err := cmd.MarkFlagRequired("spec")
	if err != nil {
		return nil
	}

	cmd.Flags().String("url", "http://127.0.0.1:9596", "Beacon URL")
	if err != nil {
		return nil
	}

	return cmd
}

func generateBeaconCheckpointCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "generate-beacon-checkpoint",
		Short: "Generate beacon checkpoint.",
		Args:  cobra.ExactArgs(0),
		RunE:  generateBeaconCheckpoint,
	}

	cmd.Flags().String("spec", "", "Valid values are mainnet or minimal")
	err := cmd.MarkFlagRequired("spec")
	if err != nil {
		return nil
	}

	cmd.Flags().String("url", "http://127.0.0.1:9596", "Beacon URL")
	if err != nil {
		return nil
	}

	cmd.Flags().Bool("export-json", true, "Export Json")
	if err != nil {
		return nil
	}

	return cmd
}

func generateExecutionUpdateCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "generate-execution-update",
		Short: "Generate execution update.",
		Args:  cobra.ExactArgs(0),
		RunE:  generateExecutionUpdate,
	}

	cmd.Flags().String("spec", "", "Valid values are mainnet or minimal")
	err := cmd.MarkFlagRequired("spec")
	if err != nil {
		return nil
	}

	cmd.Flags().Uint32("slot", 1, "slot number")
	err = cmd.MarkFlagRequired("slot")
	if err != nil {
		return nil
	}

	cmd.Flags().String("url", "http://127.0.0.1:9596", "Beacon URL")
	if err != nil {
		return nil
	}

	return cmd
}

type Data struct {
	CheckpointUpdate      beaconjson.CheckPoint
	SyncCommitteeUpdate   beaconjson.Update
	FinalizedHeaderUpdate beaconjson.Update
	HeaderUpdate          beaconjson.HeaderUpdate
}

const (
	pathToBeaconBenchmarkData    = "parachain/pallets/ethereum-beacon-client/src/benchmarking"
	pathToBenchmarkDataTemplate  = "parachain/templates/benchmarking-fixtures.mustache"
	pathToBeaconTestFixtureFiles = "parachain/pallets/ethereum-beacon-client/tests/fixtures"
)

func generateBeaconCheckpoint(cmd *cobra.Command, _ []string) error {
	err := func() error {
		spec, err := cmd.Flags().GetString("spec")
		if err != nil {
			return fmt.Errorf("get active spec: %w", err)
		}

		activeSpec, err := config.ToSpec(spec)
		if err != nil {
			return fmt.Errorf("get spec: %w", err)
		}

		endpoint, err := cmd.Flags().GetString("url")

		viper.SetConfigFile("web/packages/test/config/beacon-relay.json")

		if err := viper.ReadInConfig(); err != nil {
			return err
		}

		var conf config.Config
		err = viper.Unmarshal(&conf)
		if err != nil {
			return err
		}

		specSettings := conf.GetSpecSettingsBySpec(activeSpec)

		s := syncer.New(endpoint, specSettings, activeSpec)

		checkPointScale, err := s.GetCheckpoint()
		if err != nil {
			return fmt.Errorf("get initial sync: %w", err)
		}
		exportJson, err := cmd.Flags().GetBool("export_json")
		if exportJson {
			initialSync := checkPointScale.ToJSON()
			err = writeJSONToFile(initialSync, "dump-initial-checkpoint.json")
			if err != nil {
				return fmt.Errorf("write initial sync to file: %w", err)
			}
		}
		checkPointBytes, _ := types.EncodeToBytes(checkPointScale)
		// Call index for EthereumBeaconClient.force_checkpoint
		checkPointCallIndex := "0x5200"
		checkPointUpdateCall := checkPointCallIndex + hex.EncodeToString(checkPointBytes)
		fmt.Println(checkPointUpdateCall)
		return nil
	}()
	if err != nil {
		log.WithError(err).Error("error generating beacon checkpoint")
	}

	return nil
}

func generateBeaconData(cmd *cobra.Command, _ []string) error {
	err := func() error {
		spec, err := cmd.Flags().GetString("spec")
		if err != nil {
			return fmt.Errorf("get active spec: %w", err)
		}

		activeSpec, err := config.ToSpec(spec)
		if err != nil {
			return fmt.Errorf("get spec: %w", err)
		}

		endpoint, _ := cmd.Flags().GetString("url")

		viper.SetConfigFile("web/packages/test/config/beacon-relay.json")
		if err := viper.ReadInConfig(); err != nil {
			return err
		}

		var conf config.Config
		err = viper.Unmarshal(&conf)
		if err != nil {
			return err
		}

		// ETH_FAST_MODE hack for fast slot period
		SlotTimeDuration := 6 * time.Second
		if os.Getenv("ETH_FAST_MODE") == "true" {
			SlotTimeDuration = 1 * time.Second
		}

		specSettings := conf.GetSpecSettingsBySpec(activeSpec)
		log.WithFields(log.Fields{"spec": activeSpec, "endpoint": endpoint}).Info("connecting to beacon API")
		s := syncer.New(endpoint, specSettings, activeSpec)

		// generate InitialUpdate
		initialSyncScale, err := s.GetCheckpoint()
		if err != nil {
			return fmt.Errorf("get initial sync: %w", err)
		}
		initialSync := initialSyncScale.ToJSON()
		writeJSONToFile(initialSync, fmt.Sprintf("initial-checkpoint.%s.json", activeSpec.ToString()))
		initialSyncHeaderSlot := initialSync.Header.Slot
		initialSyncPeriod := s.ComputeSyncPeriodAtSlot(initialSyncHeaderSlot)
		initialEpoch := s.ComputeEpochAtSlot(initialSyncHeaderSlot)
		log.WithFields(log.Fields{
			"epoch":  initialEpoch,
			"period": initialSyncPeriod,
		}).Info("created initial sync file")

		// generate FinalizedUpdate for next epoch
		log.Info("waiting for a new finalized_update in next epoch and in current sync period,several seconds required...")
		elapseEpochs := uint64(1)
		waitIntervalForNextEpoch := elapseEpochs * specSettings.SlotsInEpoch
		time.Sleep(time.Duration(waitIntervalForNextEpoch) * SlotTimeDuration)
		finalizedUpdateScale, err := s.GetFinalizedUpdate()
		if err != nil {
			return fmt.Errorf("get finalized header update: %w", err)
		}
		finalizedUpdate := finalizedUpdateScale.Payload.ToJSON()
		if finalizedUpdate.AttestedHeader.Slot <= initialSyncHeaderSlot {
			return fmt.Errorf("AttestedHeader slot should be greater than initialSyncHeaderSlot")
		}
		finalizedEpoch := s.ComputeEpochAtSlot(finalizedUpdate.AttestedHeader.Slot)
		if finalizedEpoch <= initialEpoch {
			return fmt.Errorf("epoch in FinalizedUpdate should be greater than initialEpoch")
		}
		finalizedPeriod := s.ComputeSyncPeriodAtSlot(finalizedUpdate.FinalizedHeader.Slot)
		if initialSyncPeriod != finalizedPeriod {
			return fmt.Errorf("initialSyncPeriod should be consistent with finalizedUpdatePeriod")
		}
		writeJSONToFile(finalizedUpdate, fmt.Sprintf("finalized-header-update.%s.json", activeSpec.ToString()))
		log.WithFields(log.Fields{
			"epoch":  finalizedEpoch,
			"period": finalizedPeriod,
		}).Info("created finalized header update file")

		// generate SyncCommitteeUpdate same as InitialUpdate for filling NextSyncCommittee
		syncCommitteeUpdateScale, err := s.GetSyncCommitteePeriodUpdate(initialSyncPeriod)
		if err != nil {
			return fmt.Errorf("get sync committee update: %w", err)
		}
		syncCommitteeUpdate := syncCommitteeUpdateScale.Payload.ToJSON()
		writeJSONToFile(syncCommitteeUpdate, fmt.Sprintf("sync-committee-update.%s.json", activeSpec.ToString()))
		log.Info("created sync committee update file")

		// generate executionUpdate
		blockUpdateSlot := uint64(finalizedUpdateScale.Payload.FinalizedHeader.Slot - 2)
		checkPoint := cache.Proof{
			FinalizedBlockRoot: finalizedUpdateScale.FinalizedHeaderBlockRoot,
			BlockRootsTree:     finalizedUpdateScale.BlockRootsTree,
			Slot:               uint64(finalizedUpdateScale.Payload.FinalizedHeader.Slot),
		}
		headerUpdateScale, err := s.GetNextHeaderUpdateBySlotWithCheckpoint(blockUpdateSlot, &checkPoint)
		if err != nil {
			return fmt.Errorf("get header update: %w", err)
		}
		headerUpdate := headerUpdateScale.ToJSON()
		writeJSONToFile(headerUpdate, fmt.Sprintf("execution-header-update.%s.json", activeSpec.ToString()))
		log.Info("created execution update file")

		if activeSpec.IsMinimal() {
			// generate FinalizedUpdate for next period
			log.Info("waiting until next sync period,several minutes required...")
			time.Sleep(time.Duration(specSettings.SlotsInEpoch*(specSettings.EpochsPerSyncCommitteePeriod-elapseEpochs)) * SlotTimeDuration)
			nextFinalizedUpdateScale, err := s.GetFinalizedUpdate()
			if err != nil {
				return fmt.Errorf("get next finalized header update: %w", err)
			}
			nextFinalizedUpdate := nextFinalizedUpdateScale.Payload.ToJSON()
			nextFinalizedUpdatePeriod := s.ComputeSyncPeriodAtSlot(nextFinalizedUpdate.FinalizedHeader.Slot)
			if initialSyncPeriod+1 != nextFinalizedUpdatePeriod {
				return fmt.Errorf("nextFinalizedUpdatePeriod should be 1 period ahead of initialSyncPeriod")
			}
			writeJSONToFile(nextFinalizedUpdate, fmt.Sprintf("next-finalized-header-update.%s.json", activeSpec.ToString()))
			log.Info("created next finalized header update file")

			// generate nextSyncCommitteeUpdate
			nextSyncCommitteeUpdateScale, err := s.GetSyncCommitteePeriodUpdate(initialSyncPeriod + 1)
			if err != nil {
				return fmt.Errorf("get sync committee update: %w", err)
			}
			nextSyncCommitteeUpdate := nextSyncCommitteeUpdateScale.Payload.ToJSON()
			writeJSONToFile(nextSyncCommitteeUpdate, fmt.Sprintf("next-sync-committee-update.%s.json", activeSpec.ToString()))
			log.Info("created next sync committee update file")
		}

		if !activeSpec.IsMinimal() {
			log.Info("now updating benchmarking data files")

			// Rust file hexes require the 0x of hashes to be removed
			initialSync.RemoveLeadingZeroHashes()
			syncCommitteeUpdate.RemoveLeadingZeroHashes()
			finalizedUpdate.RemoveLeadingZeroHashes()
			headerUpdate.RemoveLeadingZeroHashes()

			data := Data{
				CheckpointUpdate:      initialSync,
				SyncCommitteeUpdate:   syncCommitteeUpdate,
				FinalizedHeaderUpdate: finalizedUpdate,
				HeaderUpdate:          headerUpdate,
			}

			log.WithFields(log.Fields{
				"location": pathToBeaconTestFixtureFiles,
				"template": pathToBenchmarkDataTemplate,
				"spec":     activeSpec,
			}).Info("rendering file using mustache")

			rendered, err := mustache.RenderFile(pathToBenchmarkDataTemplate, data)
			if err != nil {
				return fmt.Errorf("render benchmark fixture: %w", err)
			}
			filename := "fixtures.rs"

			log.WithFields(log.Fields{
				"location": pathToBeaconBenchmarkData,
				"filename": filename,
			}).Info("writing result file")

			err = writeBenchmarkDataFile(filename, rendered)
			if err != nil {
				return err
			}
		}

		log.WithField("spec", activeSpec).Info("done")

		return nil
	}()
	if err != nil {
		log.WithError(err).Error("error generating beacon data")
	}

	return nil
}

func writeJSONToFile(data interface{}, filename string) error {
	file, _ := json.MarshalIndent(data, "", "  ")

	f, err := os.OpenFile(fmt.Sprintf("%s/%s", pathToBeaconTestFixtureFiles, filename), os.O_RDWR|os.O_CREATE|os.O_TRUNC, 0755)

	if err != nil {
		return fmt.Errorf("create file: %w", err)
	}

	defer f.Close()

	_, err = f.Write(file)

	if err != nil {
		return fmt.Errorf("write to file: %w", err)
	}

	return nil
}

func writeBenchmarkDataFile(filename, fileContents string) error {
	f, err := os.OpenFile(fmt.Sprintf("%s/%s", pathToBeaconBenchmarkData, filename), os.O_RDWR|os.O_CREATE|os.O_TRUNC, 0755)

	if err != nil {
		return fmt.Errorf("create file: %w", err)
	}

	defer f.Close()

	_, err = f.Write([]byte(fileContents))

	if err != nil {
		return fmt.Errorf("write to file: %w", err)
	}

	return nil
}

func generateExecutionUpdate(cmd *cobra.Command, _ []string) error {
	err := func() error {
		spec, err := cmd.Flags().GetString("spec")
		if err != nil {
			return fmt.Errorf("get active spec: %w", err)
		}
		activeSpec, err := config.ToSpec(spec)
		if err != nil {
			return fmt.Errorf("get spec: %w", err)
		}

		endpoint, _ := cmd.Flags().GetString("url")
		beaconSlot, _ := cmd.Flags().GetUint32("slot")

		viper.SetConfigFile("web/packages/test/config/beacon-relay.json")
		if err := viper.ReadInConfig(); err != nil {
			return err
		}
		var conf config.Config
		err = viper.Unmarshal(&conf)
		if err != nil {
			return err
		}
		specSettings := conf.GetSpecSettingsBySpec(activeSpec)
		log.WithFields(log.Fields{"spec": activeSpec, "endpoint": endpoint}).Info("connecting to beacon API")

		// generate executionUpdate
		s := syncer.New(endpoint, specSettings, activeSpec)
		blockRoot, err := s.Client.GetBeaconBlockRoot(uint64(beaconSlot))
		if err != nil {
			return fmt.Errorf("fetch block: %w", err)
		}
		headerUpdateScale, err := s.GetHeaderUpdate(blockRoot, nil)
		if err != nil {
			return fmt.Errorf("get header update: %w", err)
		}
		headerUpdate := headerUpdateScale.ToJSON()
		writeJSONToFile(headerUpdate, fmt.Sprintf("execution-header-update.%s.json", activeSpec.ToString()))
		log.Info("created execution update file")

		return nil
	}()
	if err != nil {
		log.WithError(err).Error("error generating beacon execution update")
	}

	return nil
}
