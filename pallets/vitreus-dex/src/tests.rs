//! Tests for the Vitreus DEX pallet.

use crate::mock::*;
use crate::{Error, Event, LiquidityPositions, Pools, TotalLiquidity};
use frame_support::{assert_noop, assert_ok};
use vitreus_runtime_common::OnEnergySell;

fn usdc() -> NativeOrAssetId {
    NativeOrAssetId::WithId(USDC_ID)
}
fn vnrg() -> NativeOrAssetId {
    NativeOrAssetId::WithId(VNRG_ID)
}
fn pair() -> (NativeOrAssetId, NativeOrAssetId) {
    (usdc(), vnrg())
}

#[test]
fn test_create_pool_success() {
    new_test_ext().execute_with(|| {
        assert_ok!(VitreusDex::create_pool(
            RuntimeOrigin::root(),
            usdc(),
            vnrg(),
            30,
        ));

        let pool = Pools::<Test>::get(pair()).expect("pool stored");
        assert_eq!(pool.reserve_a, 0);
        assert_eq!(pool.reserve_b, 0);
        assert_eq!(pool.fee_tier, 30);
        assert_eq!(pool.total_fees_collected, 0);
        assert_eq!(TotalLiquidity::<Test>::get(pair()), Some(0));

        System::assert_has_event(
            Event::PoolCreated { asset_a: usdc(), asset_b: vnrg(), fee_tier: 30 }.into(),
        );
    });
}

#[test]
fn test_create_pool_duplicate_fails() {
    new_test_ext().execute_with(|| {
        assert_ok!(VitreusDex::create_pool(
            RuntimeOrigin::root(),
            usdc(),
            vnrg(),
            30,
        ));
        assert_noop!(
            VitreusDex::create_pool(RuntimeOrigin::root(), usdc(), vnrg(), 30),
            Error::<Test>::PoolAlreadyExists
        );
    });
}

#[test]
fn test_add_liquidity_first_deposit() {
    new_test_ext().execute_with(|| {
        assert_ok!(VitreusDex::create_pool(
            RuntimeOrigin::root(),
            usdc(),
            vnrg(),
            30,
        ));
        assert_ok!(VitreusDex::add_liquidity(
            RuntimeOrigin::signed(ALICE),
            usdc(),
            vnrg(),
            100,
            400,
            0,
            0,
        ));

        // shares = sqrt(100 * 400) = 200
        assert_eq!(TotalLiquidity::<Test>::get(pair()), Some(200));
        let pos = LiquidityPositions::<Test>::get(ALICE, pair()).unwrap();
        assert_eq!(pos.shares, 200);

        let pool = Pools::<Test>::get(pair()).unwrap();
        assert_eq!(pool.reserve_a, 100);
        assert_eq!(pool.reserve_b, 400);
    });
}

#[test]
fn test_add_liquidity_subsequent_deposit() {
    new_test_ext().execute_with(|| {
        assert_ok!(VitreusDex::create_pool(
            RuntimeOrigin::root(),
            usdc(),
            vnrg(),
            30,
        ));
        // First deposit: 100/400 → 200 shares.
        assert_ok!(VitreusDex::add_liquidity(
            RuntimeOrigin::signed(ALICE),
            usdc(),
            vnrg(),
            100,
            400,
            0,
            0,
        ));
        // Second deposit: half the pool, 50/200 → 100 shares (min of proportional).
        assert_ok!(VitreusDex::add_liquidity(
            RuntimeOrigin::signed(BOB),
            usdc(),
            vnrg(),
            50,
            200,
            0,
            0,
        ));

        assert_eq!(TotalLiquidity::<Test>::get(pair()), Some(300));
        let bob_pos = LiquidityPositions::<Test>::get(BOB, pair()).unwrap();
        assert_eq!(bob_pos.shares, 100);

        let pool = Pools::<Test>::get(pair()).unwrap();
        assert_eq!(pool.reserve_a, 150);
        assert_eq!(pool.reserve_b, 600);
    });
}

#[test]
fn test_remove_liquidity_full() {
    new_test_ext().execute_with(|| {
        assert_ok!(VitreusDex::create_pool(
            RuntimeOrigin::root(),
            usdc(),
            vnrg(),
            30,
        ));
        assert_ok!(VitreusDex::add_liquidity(
            RuntimeOrigin::signed(ALICE),
            usdc(),
            vnrg(),
            100,
            400,
            0,
            0,
        ));

        assert_ok!(VitreusDex::remove_liquidity(
            RuntimeOrigin::signed(ALICE),
            usdc(),
            vnrg(),
            200,
            0,
            0,
        ));

        let pool = Pools::<Test>::get(pair()).unwrap();
        assert_eq!(pool.reserve_a, 0);
        assert_eq!(pool.reserve_b, 0);
        assert_eq!(TotalLiquidity::<Test>::get(pair()), Some(0));
        assert!(LiquidityPositions::<Test>::get(ALICE, pair()).is_none());
    });
}

#[test]
fn test_swap_exact_tokens() {
    new_test_ext().execute_with(|| {
        assert_ok!(VitreusDex::create_pool(
            RuntimeOrigin::root(),
            usdc(),
            vnrg(),
            30,
        ));
        assert_ok!(VitreusDex::add_liquidity(
            RuntimeOrigin::signed(ALICE),
            usdc(),
            vnrg(),
            1_000,
            1_000,
            0,
            0,
        ));

        // amount_in = 100, fee_tier = 30 (3%)
        // fee = 100 * 30 / 1000 = 3
        // amount_in_after_fee = 97
        // amount_out = 1000 * 97 / (1000 + 97) = 97000 / 1097 = 88 (integer division)
        assert_ok!(VitreusDex::swap_exact_tokens_for_tokens(
            RuntimeOrigin::signed(BOB),
            usdc(),
            vnrg(),
            100,
            0,
            BOB,
        ));

        let pool = Pools::<Test>::get(pair()).unwrap();
        assert_eq!(pool.reserve_a, 1_100);
        assert_eq!(pool.reserve_b, 912);
        assert_eq!(pool.total_fees_collected, 3);

        System::assert_has_event(
            Event::SwapExecuted {
                who: BOB,
                asset_in: usdc(),
                asset_out: vnrg(),
                amount_in: 100,
                amount_out: 88,
                fee: 3,
            }
            .into(),
        );
    });
}

#[test]
fn test_swap_insufficient_liquidity() {
    new_test_ext().execute_with(|| {
        assert_ok!(VitreusDex::create_pool(
            RuntimeOrigin::root(),
            usdc(),
            vnrg(),
            30,
        ));
        // Pool exists but has no liquidity.
        assert_noop!(
            VitreusDex::swap_exact_tokens_for_tokens(
                RuntimeOrigin::signed(BOB),
                usdc(),
                vnrg(),
                100,
                0,
                BOB,
            ),
            Error::<Test>::InsufficientLiquidity
        );
    });
}

#[test]
fn test_swap_slippage_protection() {
    new_test_ext().execute_with(|| {
        assert_ok!(VitreusDex::create_pool(
            RuntimeOrigin::root(),
            usdc(),
            vnrg(),
            30,
        ));
        assert_ok!(VitreusDex::add_liquidity(
            RuntimeOrigin::signed(ALICE),
            usdc(),
            vnrg(),
            1_000,
            1_000,
            0,
            0,
        ));

        // Actual output is ~88; demand 500 to trigger slippage error.
        assert_noop!(
            VitreusDex::swap_exact_tokens_for_tokens(
                RuntimeOrigin::signed(BOB),
                usdc(),
                vnrg(),
                100,
                500,
                BOB,
            ),
            Error::<Test>::SlippageExceeded
        );
    });
}

#[test]
fn test_on_energy_sell_hook() {
    new_test_ext().execute_with(|| {
        <VitreusDex as OnEnergySell<u128>>::on_energy_sell(42);
        System::assert_has_event(Event::EnergySold { amount: 42 }.into());
    });
}
