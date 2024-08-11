pub use super::*;
use crate::mock::*;
use frame_support::{assert_noop, assert_ok};
use frame_support::traits::OnIdle;

pub fn next_block() {
	System::set_block_number(<Test as pallet_distribution::Config>::BlockNumberProvider::current_block_number() + 1);
	AllPalletsWithSystem::on_initialize(<Test as pallet_distribution::Config>::BlockNumberProvider::current_block_number());
    AllPalletsWithSystem::on_idle(<Test as pallet_distribution::Config>::BlockNumberProvider::current_block_number(), Weight::MAX);
}

pub fn run_to_block(n: BlockNumberFor<Test>) {
	while <Test as pallet_distribution::Config>::BlockNumberProvider::current_block_number() < n {
		if <Test as pallet_distribution::Config>::BlockNumberProvider::current_block_number() > 1 {
			AllPalletsWithSystem::on_finalize(<Test as pallet_distribution::Config>::BlockNumberProvider::current_block_number());
		}
		next_block();
	}
}

pub fn create_project_list(){
    const max_number:u64 = <Test as Config>::MaxWhitelistedProjects::get() as u64;
    let mut bounded_vec = BoundedVec::<u64, <Test as Config>::MaxWhitelistedProjects>::new();
    for i in 0..max_number {
        let _= bounded_vec.try_push(i+100);
        
    }
    WhiteListedProjectAccounts::<Test>::mutate(|value|{
        *value = bounded_vec;
    });
    
}

#[test]
fn first_round_creation_works() {
    new_test_ext().execute_with(|| {

        // Creating whitelisted projects list succeeds
        create_project_list();
        let project_list = WhiteListedProjectAccounts::<Test>::get();
        let max_number:u64 = <Test as Config>::MaxWhitelistedProjects::get() as u64;
        assert_eq!(project_list.len(), max_number as usize);

        // First round is created
        next_block();
        let voting_period = <Test as Config>::VotingPeriod::get();
        let voting_lock_period = <Test as Config>::VoteLockingPeriod::get(); 
        let now = <Test as pallet_distribution::Config>::BlockNumberProvider::current_block_number();

        let round_ending_block = now.clone().saturating_add(voting_period.into());
        let voting_locked_block = round_ending_block.saturating_sub(voting_lock_period.into());

        let first_round_info:VotingRoundInfo<Test> =  VotingRoundInfo {
                round_number: 0,
                round_starting_block: now,
                voting_locked_block,
                round_ending_block,
            };

        // The righ event was emitted
        expect_events(vec![
            RuntimeEvent::Opf(Event::VotingRoundStarted{
                when: now,
                round_number: 0,
            })
        ]);

        // The storage infos are correct 
        let round_info = VotingRounds::<Test>::get(0).unwrap();
        assert_eq!(first_round_info, round_info);
    })
}

#[test]
fn voting_action_works() {
    new_test_ext().execute_with(||{
        
        create_project_list();
        next_block();

        

        // Bob nominate project_102 with an amount of 1000*BSX
        assert_ok!(Opf::vote(
            RawOrigin::Signed(BOB).into(),
            102,
            1000 * BSX,
            true,
        ));

        // expected event is emitted
        let voting_period = <Test as Config>::VotingPeriod::get();
        let voting_lock_period = <Test as Config>::VoteLockingPeriod::get(); 
        let now = <Test as pallet_distribution::Config>::BlockNumberProvider::current_block_number();

        let round_ending_block = now.clone().saturating_add(voting_period.into());
        let voting_locked_block = round_ending_block.saturating_sub(voting_lock_period.into());
        let first_round_info:VotingRoundInfo<Test> =  VotingRoundInfo {
            round_number: 0,
            round_starting_block: now,
            voting_locked_block,
            round_ending_block,
        };
        
        expect_events(vec!{
            RuntimeEvent::Opf(Event::VoteCasted{
                who: BOB,
                when: now,
                project_id:102,
            })
        });

        // The storage infos are correct 
        let first_vote_info: VoteInfo<Test> = VoteInfo { amount: 1000*BSX, round: first_round_info, is_fund:true};
        let vote_info = Votes::<Test>::get(102,BOB).unwrap();
        assert_eq!(first_vote_info,vote_info);

    })
}