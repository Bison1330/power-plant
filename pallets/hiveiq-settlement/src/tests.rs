//! Unit tests for `pallet-hiveiq-settlement`.

use crate::{
    mock::{
        new_test_ext, task_id, Balances, HiveIQSettlement, RuntimeOrigin, System, Test, ALICE,
        ORACLE, ORACLE_ENDOWMENT, PROVIDER,
    },
    Error, EscrowBalance, Event, ProviderStatsMap, ReceiptStatus, TaskReceipts,
    INITIAL_REPUTATION, REPUTATION_PENALTY, REPUTATION_REWARD,
};
use frame_support::{assert_noop, assert_ok, BoundedVec};
use sp_runtime::DispatchError;

const COST: u64 = 1_000;

fn pallet_account() -> u64 {
    HiveIQSettlement::account_id()
}

fn last_events() -> Vec<crate::mock::RuntimeEvent> {
    System::events().into_iter().map(|r| r.event).collect()
}

#[test]
fn submit_receipt_works_and_escrows_funds() {
    new_test_ext().execute_with(|| {
        let id = task_id(1);
        let oracle_before = Balances::free_balance(ORACLE);

        assert_ok!(HiveIQSettlement::submit_receipt(
            RuntimeOrigin::signed(ORACLE),
            id,
            PROVIDER,
            COST,
            0, // public
        ));

        // Funds moved from oracle into the pallet escrow account.
        assert_eq!(Balances::free_balance(ORACLE), oracle_before - COST);
        assert_eq!(Balances::free_balance(pallet_account()), COST);

        // Receipt and escrow rows are populated.
        let r = TaskReceipts::<Test>::get(id).expect("receipt should exist");
        assert_eq!(r.provider_id, PROVIDER);
        assert_eq!(r.cost_vtrs, COST);
        assert_eq!(r.status, ReceiptStatus::Pending);
        assert_eq!(EscrowBalance::<Test>::get(id), Some(COST));

        // Both the receipt and escrow events were emitted.
        let evs = last_events();
        assert!(evs.iter().any(|e| matches!(
            e,
            crate::mock::RuntimeEvent::HiveIQSettlement(Event::TaskReceiptSubmitted { .. })
        )));
        assert!(evs.iter().any(|e| matches!(
            e,
            crate::mock::RuntimeEvent::HiveIQSettlement(Event::EscrowHeld { .. })
        )));
    });
}

#[test]
fn settle_task_releases_escrow_and_bumps_reputation() {
    new_test_ext().execute_with(|| {
        let id = task_id(2);
        let provider_before = Balances::free_balance(PROVIDER);

        assert_ok!(HiveIQSettlement::submit_receipt(
            RuntimeOrigin::signed(ORACLE),
            id,
            PROVIDER,
            COST,
            2, // tee
        ));
        assert_ok!(HiveIQSettlement::settle_task(
            RuntimeOrigin::signed(ALICE),
            id
        ));

        // Provider was paid; pallet account was drained back to zero.
        assert_eq!(Balances::free_balance(PROVIDER), provider_before + COST);
        assert_eq!(Balances::free_balance(pallet_account()), 0);

        // Receipt is now Settled, escrow row is gone.
        let r = TaskReceipts::<Test>::get(id).unwrap();
        assert_eq!(r.status, ReceiptStatus::Settled);
        assert!(EscrowBalance::<Test>::get(id).is_none());

        // Provider stats reflect the settlement.
        let stats = ProviderStatsMap::<Test>::get(PROVIDER);
        assert_eq!(stats.total_tasks, 1);
        assert_eq!(stats.total_earned, COST);
        assert_eq!(stats.reputation_score, INITIAL_REPUTATION + REPUTATION_REWARD);
        assert_eq!(stats.slash_count, 0);
    });
}

#[test]
fn settle_task_fails_if_not_found() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            HiveIQSettlement::settle_task(RuntimeOrigin::signed(ALICE), task_id(99)),
            Error::<Test>::TaskNotFound
        );
    });
}

#[test]
fn settle_task_fails_if_already_settled() {
    new_test_ext().execute_with(|| {
        let id = task_id(3);
        assert_ok!(HiveIQSettlement::submit_receipt(
            RuntimeOrigin::signed(ORACLE),
            id,
            PROVIDER,
            COST,
            1, // semi-private
        ));
        assert_ok!(HiveIQSettlement::settle_task(
            RuntimeOrigin::signed(ALICE),
            id
        ));
        assert_noop!(
            HiveIQSettlement::settle_task(RuntimeOrigin::signed(ALICE), id),
            Error::<Test>::AlreadySettled
        );
    });
}

#[test]
fn submit_receipt_rejects_invalid_privacy_tier() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            HiveIQSettlement::submit_receipt(
                RuntimeOrigin::signed(ORACLE),
                task_id(4),
                PROVIDER,
                COST,
                7, // not in {0, 1, 2}
            ),
            Error::<Test>::InvalidPrivacyTier
        );
    });
}

#[test]
fn submit_receipt_rejects_duplicate_task_id() {
    new_test_ext().execute_with(|| {
        let id = task_id(5);
        assert_ok!(HiveIQSettlement::submit_receipt(
            RuntimeOrigin::signed(ORACLE),
            id,
            PROVIDER,
            COST,
            0,
        ));
        assert_noop!(
            HiveIQSettlement::submit_receipt(
                RuntimeOrigin::signed(ORACLE),
                id,
                PROVIDER,
                COST,
                0,
            ),
            Error::<Test>::DuplicateReceipt
        );
    });
}

#[test]
fn submit_receipt_rejects_non_oracle_origin() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            HiveIQSettlement::submit_receipt(
                RuntimeOrigin::signed(ALICE),
                task_id(6),
                PROVIDER,
                COST,
                0,
            ),
            DispatchError::BadOrigin
        );
    });
}

#[test]
fn slash_provider_burns_share_and_decrements_reputation() {
    new_test_ext().execute_with(|| {
        let id = task_id(7);
        let issuance_before = Balances::total_issuance();

        assert_ok!(HiveIQSettlement::submit_receipt(
            RuntimeOrigin::signed(ORACLE),
            id,
            PROVIDER,
            COST,
            0,
        ));
        // Pre-seed stats so we can observe the penalty cleanly.
        assert_ok!(HiveIQSettlement::update_provider_stats(
            RuntimeOrigin::root(),
            PROVIDER,
            true,
            COST,
        ));
        let pre = ProviderStatsMap::<Test>::get(PROVIDER);
        assert_eq!(pre.reputation_score, INITIAL_REPUTATION + REPUTATION_REWARD);

        let reason: BoundedVec<u8, _> = b"bad output".to_vec().try_into().unwrap();
        assert_ok!(HiveIQSettlement::slash_provider(
            RuntimeOrigin::root(),
            PROVIDER,
            id,
            reason,
        ));

        // 50% of escrow burned -> total issuance dropped by COST/2.
        // The other 50% stays in the pallet account.
        assert_eq!(Balances::total_issuance(), issuance_before - COST / 2);
        assert_eq!(Balances::free_balance(pallet_account()), COST / 2);

        // Receipt marked Slashed; escrow row removed.
        let r = TaskReceipts::<Test>::get(id).unwrap();
        assert_eq!(r.status, ReceiptStatus::Slashed);
        assert!(EscrowBalance::<Test>::get(id).is_none());

        // Reputation dropped by REPUTATION_PENALTY; slash_count incremented.
        let post = ProviderStatsMap::<Test>::get(PROVIDER);
        assert_eq!(post.slash_count, 1);
        assert_eq!(
            post.reputation_score,
            pre.reputation_score - REPUTATION_PENALTY
        );

        // Settled flow on the same task is now blocked.
        assert_noop!(
            HiveIQSettlement::settle_task(RuntimeOrigin::signed(ALICE), id),
            Error::<Test>::AlreadySettled
        );
    });
}

#[test]
fn slash_provider_requires_root() {
    new_test_ext().execute_with(|| {
        let id = task_id(8);
        assert_ok!(HiveIQSettlement::submit_receipt(
            RuntimeOrigin::signed(ORACLE),
            id,
            PROVIDER,
            COST,
            0,
        ));
        let reason: BoundedVec<u8, _> = b"nope".to_vec().try_into().unwrap();
        assert_noop!(
            HiveIQSettlement::slash_provider(
                RuntimeOrigin::signed(ALICE),
                PROVIDER,
                id,
                reason,
            ),
            DispatchError::BadOrigin
        );
    });
}

#[test]
fn update_provider_stats_initializes_reputation() {
    new_test_ext().execute_with(|| {
        assert_ok!(HiveIQSettlement::update_provider_stats(
            RuntimeOrigin::root(),
            PROVIDER,
            true,
            500,
        ));
        let stats = ProviderStatsMap::<Test>::get(PROVIDER);
        assert_eq!(stats.total_tasks, 1);
        assert_eq!(stats.total_earned, 500);
        assert_eq!(stats.reputation_score, INITIAL_REPUTATION + REPUTATION_REWARD);
    });
}

#[test]
fn oracle_endowment_sanity() {
    // Lightweight sanity check that the mock setup is what we expect — saves
    // future debugging if someone changes the genesis.
    new_test_ext().execute_with(|| {
        assert_eq!(Balances::free_balance(ORACLE), ORACLE_ENDOWMENT);
    });
}
