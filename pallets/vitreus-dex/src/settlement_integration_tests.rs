//! Integration tests for Part 2a scope:
//! - Governance setter extrinsics (bid_window, settlement_window, solver_bond)
//! - Escrow account determinism
//! - Default fallback behavior on governance params
//!
//! Intent / solver / fill lifecycle tests arrive with Part 2b and 2c.

use crate::{
    mock::*,
    BidWindowBlocks, Error, Event, SettlementWindowBlocks, SolverBondAmount,
};
use frame_support::{assert_noop, assert_ok};

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
