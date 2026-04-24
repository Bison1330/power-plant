//! Integration tests for Part 2a and 2b scope:
//! - Governance setter extrinsics (bid_window, settlement_window, solver_bond)
//! - Escrow account determinism
//! - Default fallback behavior on governance params
//! - Solver registration / deregistration / re-registration
//! - Intent submission / cancellation
//!
//! Fill / settle / slash tests arrive with Part 2c.

use crate::{
    mock::*,
    settlement::IntentStatus,
    BidWindowBlocks, Error, Event, FillCommitments, IntentEscrowBalances,
    Intents, NextIntentId, NextSolverId, SettlementWindowBlocks,
    SolverAccountToId, SolverBondAmount, Solvers,
};
use frame_support::{assert_noop, assert_ok};

fn usdc() -> NativeOrAssetId {
    NativeOrAssetId::WithId(USDC_ID)
}
fn vnrg() -> NativeOrAssetId {
    NativeOrAssetId::WithId(VNRG_ID)
}

/// Register `who` as a solver and return the assigned id.
fn register_solver_for(who: u128) -> u64 {
    let id = NextSolverId::<Test>::get();
    assert_ok!(VitreusDex::register_solver(RuntimeOrigin::signed(who)));
    id
}

// ----------------------------------------------------------------------------
// Governance setters
// ----------------------------------------------------------------------------

#[test]
fn set_bid_window_works_under_manage_origin() {
    new_test_ext().execute_with(|| {
        assert_ok!(VitreusDex::set_bid_window(RuntimeOrigin::root(), 42));
        assert_eq!(BidWindowBlocks::<Test>::get(), Some(42));

        System::assert_last_event(RuntimeEvent::VitreusDex(
            Event::BidWindowUpdated { new_value: 42 },
        ));
    });
}

#[test]
fn set_bid_window_rejects_non_manage_origin() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            VitreusDex::set_bid_window(RuntimeOrigin::signed(ALICE), 42),
            sp_runtime::DispatchError::BadOrigin,
        );
    });
}

#[test]
fn set_bid_window_rejects_zero() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            VitreusDex::set_bid_window(RuntimeOrigin::root(), 0),
            Error::<Test>::InvalidAmount,
        );
    });
}

#[test]
fn set_settlement_window_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(VitreusDex::set_settlement_window(RuntimeOrigin::root(), 7));
        assert_eq!(SettlementWindowBlocks::<Test>::get(), Some(7));
    });
}

#[test]
fn set_solver_bond_amount_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(VitreusDex::set_solver_bond_amount(
            RuntimeOrigin::root(),
            2_000_000_000_000,
        ));
        assert_eq!(
            SolverBondAmount::<Test>::get(),
            Some(2_000_000_000_000),
        );
    });
}

// ----------------------------------------------------------------------------
// Default fallback
// ----------------------------------------------------------------------------

#[test]
fn current_bid_window_falls_back_to_default_when_unset() {
    new_test_ext().execute_with(|| {
        assert_eq!(BidWindowBlocks::<Test>::get(), None);
        assert_eq!(VitreusDex::current_bid_window(), 10);
    });
}

#[test]
fn current_bid_window_uses_storage_when_set() {
    new_test_ext().execute_with(|| {
        BidWindowBlocks::<Test>::put(99);
        assert_eq!(VitreusDex::current_bid_window(), 99);
    });
}

#[test]
fn current_settlement_window_falls_back_to_default() {
    new_test_ext().execute_with(|| {
        assert_eq!(VitreusDex::current_settlement_window(), 5);
    });
}

#[test]
fn current_solver_bond_falls_back_to_default() {
    new_test_ext().execute_with(|| {
        assert_eq!(VitreusDex::current_solver_bond(), 1_000_000_000_000);
    });
}

// ----------------------------------------------------------------------------
// Escrow account determinism
// ----------------------------------------------------------------------------

#[test]
fn solver_escrow_accounts_are_distinct_per_solver_id() {
    new_test_ext().execute_with(|| {
        let a = VitreusDex::solver_escrow_account(1);
        let b = VitreusDex::solver_escrow_account(2);
        assert_ne!(a, b, "distinct solver ids must yield distinct escrow accounts");
    });
}

#[test]
fn solver_escrow_account_is_deterministic() {
    new_test_ext().execute_with(|| {
        let first = VitreusDex::solver_escrow_account(42);
        let second = VitreusDex::solver_escrow_account(42);
        assert_eq!(first, second, "same solver id must yield same escrow account");
    });
}

#[test]
fn intent_escrow_is_distinct_from_solver_escrows() {
    new_test_ext().execute_with(|| {
        let intent_acc = VitreusDex::intent_escrow_account();
        let solver_acc = VitreusDex::solver_escrow_account(0);
        assert_ne!(intent_acc, solver_acc);
    });
}

#[test]
fn protocol_treasury_is_distinct_from_escrows() {
    new_test_ext().execute_with(|| {
        let treasury = VitreusDex::protocol_treasury_account();
        assert_ne!(treasury, VitreusDex::intent_escrow_account());
        assert_ne!(treasury, VitreusDex::solver_escrow_account(0));
    });
}

// ============================================================================
// Part 2b: solver registration and intent lifecycle
// ============================================================================

#[test]
fn register_solver_assigns_sequential_ids_and_escrows_bond() {
    new_test_ext().execute_with(|| {
        let alice_id = register_solver_for(ALICE);
        assert_eq!(alice_id, 0);
        let bob_id = register_solver_for(BOB);
        assert_eq!(bob_id, 1);

        let alice_solver = Solvers::<Test>::get(0).expect("registered");
        assert!(alice_solver.active);
        assert_eq!(alice_solver.account, ALICE);
        assert_eq!(alice_solver.bond, 1_000_000_000_000);
        assert_eq!(alice_solver.active_commitments, 0);
        assert_eq!(alice_solver.reputation, 0);
        assert_eq!(alice_solver.fills_completed, 0);
        assert_eq!(alice_solver.fills_slashed, 0);

        assert_eq!(SolverAccountToId::<Test>::get(ALICE), Some(0));
        assert_eq!(SolverAccountToId::<Test>::get(BOB), Some(1));
        assert_eq!(NextSolverId::<Test>::get(), 2);
    });
}

#[test]
fn register_solver_rejects_duplicate_while_active() {
    new_test_ext().execute_with(|| {
        register_solver_for(ALICE);
        assert_noop!(
            VitreusDex::register_solver(RuntimeOrigin::signed(ALICE)),
            Error::<Test>::SolverAlreadyRegistered,
        );
    });
}

#[test]
fn deregister_solver_refunds_bond_and_deactivates() {
    new_test_ext().execute_with(|| {
        let id = register_solver_for(ALICE);
        assert_ok!(VitreusDex::deregister_solver(RuntimeOrigin::signed(ALICE)));

        let solver = Solvers::<Test>::get(id).expect("still indexed after deregister");
        assert!(!solver.active);
        assert_eq!(solver.bond, 0);

        System::assert_has_event(RuntimeEvent::VitreusDex(
            Event::SolverDeregistered {
                solver_id: id,
                account: ALICE,
                bond_refunded: 1_000_000_000_000,
            },
        ));
    });
}

#[test]
fn deregister_solver_fails_if_not_registered() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            VitreusDex::deregister_solver(RuntimeOrigin::signed(ALICE)),
            Error::<Test>::SolverNotRegistered,
        );
    });
}

#[test]
fn register_after_deregister_gives_fresh_solver_id() {
    new_test_ext().execute_with(|| {
        let first_id = register_solver_for(ALICE);
        assert_ok!(VitreusDex::deregister_solver(RuntimeOrigin::signed(ALICE)));
        let second_id = register_solver_for(ALICE);

        assert_ne!(first_id, second_id);
        assert_eq!(second_id, 1);

        let new_solver = Solvers::<Test>::get(second_id).expect("registered");
        assert!(new_solver.active);
        assert_eq!(new_solver.reputation, 0);
        assert_eq!(new_solver.fills_completed, 0);
        assert_eq!(new_solver.bond, 1_000_000_000_000);

        // Account index updated to the new id.
        assert_eq!(SolverAccountToId::<Test>::get(ALICE), Some(second_id));
    });
}

#[test]
fn submit_intent_escrows_token_in_and_stores_intent() {
    new_test_ext().execute_with(|| {
        let now = System::block_number();
        let deadline = now + 100;

        assert_ok!(VitreusDex::submit_intent(
            RuntimeOrigin::signed(ALICE),
            usdc(),
            vnrg(),
            500_000,
            450_000,
            deadline,
        ));

        let intent = Intents::<Test>::get(0).expect("stored");
        assert_eq!(intent.user, ALICE);
        assert_eq!(intent.token_in, usdc());
        assert_eq!(intent.token_out, vnrg());
        assert_eq!(intent.amount_in, 500_000);
        assert_eq!(intent.min_amount_out, 450_000);
        assert_eq!(intent.deadline, deadline);
        assert_eq!(intent.status, IntentStatus::Open);

        assert_eq!(
            IntentEscrowBalances::<Test>::get(0),
            Some((usdc(), 500_000)),
        );
        assert_eq!(NextIntentId::<Test>::get(), 1);
    });
}

#[test]
fn submit_intent_rejects_zero_amount_in() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            VitreusDex::submit_intent(
                RuntimeOrigin::signed(ALICE),
                usdc(),
                vnrg(),
                0,
                0,
                System::block_number() + 100,
            ),
            Error::<Test>::InvalidAmount,
        );
    });
}

#[test]
fn submit_intent_rejects_same_token() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            VitreusDex::submit_intent(
                RuntimeOrigin::signed(ALICE),
                usdc(),
                usdc(),
                500_000,
                400_000,
                System::block_number() + 100,
            ),
            Error::<Test>::InvalidAmount,
        );
    });
}

#[test]
fn submit_intent_rejects_past_deadline() {
    new_test_ext().execute_with(|| {
        let now = System::block_number();
        System::set_block_number(now + 10);
        assert_noop!(
            VitreusDex::submit_intent(
                RuntimeOrigin::signed(ALICE),
                usdc(),
                vnrg(),
                500_000,
                450_000,
                now,
            ),
            Error::<Test>::InvalidDeadline,
        );
    });
}

#[test]
fn cancel_intent_refunds_and_marks_cancelled() {
    new_test_ext().execute_with(|| {
        assert_ok!(VitreusDex::submit_intent(
            RuntimeOrigin::signed(ALICE),
            usdc(),
            vnrg(),
            500_000,
            450_000,
            System::block_number() + 100,
        ));

        assert_ok!(VitreusDex::cancel_intent(RuntimeOrigin::signed(ALICE), 0));

        let intent = Intents::<Test>::get(0).expect("still indexed");
        assert_eq!(intent.status, IntentStatus::Cancelled);
        assert!(IntentEscrowBalances::<Test>::get(0).is_none());
    });
}

#[test]
fn cancel_intent_rejects_non_owner() {
    new_test_ext().execute_with(|| {
        assert_ok!(VitreusDex::submit_intent(
            RuntimeOrigin::signed(ALICE),
            usdc(),
            vnrg(),
            500_000,
            450_000,
            System::block_number() + 100,
        ));

        assert_noop!(
            VitreusDex::cancel_intent(RuntimeOrigin::signed(BOB), 0),
            Error::<Test>::NotIntentOwner,
        );
    });
}

#[test]
fn cancel_intent_rejects_unknown_intent() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            VitreusDex::cancel_intent(RuntimeOrigin::signed(ALICE), 999),
            Error::<Test>::IntentNotFound,
        );
    });
}

#[test]
fn events_for_register_intent_cancel_have_correct_payloads() {
    new_test_ext().execute_with(|| {
        assert_ok!(VitreusDex::register_solver(RuntimeOrigin::signed(ALICE)));
        System::assert_has_event(RuntimeEvent::VitreusDex(
            Event::SolverRegistered {
                solver_id: 0,
                account: ALICE,
                bond: 1_000_000_000_000,
            },
        ));

        let deadline = System::block_number() + 100;
        assert_ok!(VitreusDex::submit_intent(
            RuntimeOrigin::signed(ALICE),
            usdc(),
            vnrg(),
            500_000,
            450_000,
            deadline,
        ));
        System::assert_has_event(RuntimeEvent::VitreusDex(
            Event::IntentSubmitted {
                intent_id: 0,
                user: ALICE,
                token_in: usdc(),
                token_out: vnrg(),
                amount_in: 500_000,
                min_amount_out: 450_000,
                deadline,
            },
        ));

        assert_ok!(VitreusDex::cancel_intent(RuntimeOrigin::signed(ALICE), 0));
        System::assert_has_event(RuntimeEvent::VitreusDex(
            Event::IntentCancelled { intent_id: 0, user: ALICE },
        ));
    });
}

#[test]
fn fill_commitments_storage_is_empty_after_registration() {
    new_test_ext().execute_with(|| {
        register_solver_for(ALICE);
        assert!(FillCommitments::<Test>::iter().next().is_none());
    });
}
