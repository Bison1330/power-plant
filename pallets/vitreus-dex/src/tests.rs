//! Tests for the Vitreus DEX pallet.

use crate::mock::*;
use crate::{
    Error, Event, LiquidityPositions, PoolManager, Pools, TotalEnergySold, TotalLiquidity,
};
use frame_support::{assert_noop, assert_ok};
use vitreus_runtime_common::OnEnergySell;

fn native() -> NativeOrAssetId {
    NativeOrAssetId::Native
}
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
            10,
        ));

        let pool = Pools::<Test>::get(pair()).expect("pool stored");
        assert_eq!(pool.reserve_a, 0);
        assert_eq!(pool.reserve_b, 0);
        assert_eq!(pool.fee_tier, 10);
        assert_eq!(pool.total_fees_collected, 0);
        assert_eq!(TotalLiquidity::<Test>::get(pair()), Some(0));

        System::assert_has_event(
            Event::PoolCreated { asset_a: usdc(), asset_b: vnrg(), fee_tier: 10 }.into(),
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
            10,
        ));
        assert_noop!(
            VitreusDex::create_pool(RuntimeOrigin::root(), usdc(), vnrg(), 10),
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
            10,
        ));
        assert_ok!(VitreusDex::add_liquidity(
            RuntimeOrigin::signed(ALICE),
            usdc(),
            vnrg(),
            10_000,
            40_000,
            0,
            0,
        ));

        // shares = sqrt(10_000 * 40_000) = 20_000
        // MINIMUM_LIQUIDITY = 1_000 locked permanently
        // shares_to_mint = 20_000 - 1_000 = 19_000
        assert_eq!(TotalLiquidity::<Test>::get(pair()), Some(20_000));
        let pos = LiquidityPositions::<Test>::get(ALICE, pair()).unwrap();
        assert_eq!(pos.shares, 19_000);

        let pool = Pools::<Test>::get(pair()).unwrap();
        assert_eq!(pool.reserve_a, 10_000);
        assert_eq!(pool.reserve_b, 40_000);
    });
}

#[test]
fn test_add_liquidity_subsequent_deposit() {
    new_test_ext().execute_with(|| {
        assert_ok!(VitreusDex::create_pool(
            RuntimeOrigin::root(),
            usdc(),
            vnrg(),
            10,
        ));
        // First deposit: 10_000/40_000 → 19_000 user shares, 20_000 total.
        assert_ok!(VitreusDex::add_liquidity(
            RuntimeOrigin::signed(ALICE),
            usdc(),
            vnrg(),
            10_000,
            40_000,
            0,
            0,
        ));
        // Second deposit: proportional half (5_000/20_000) → 10_000 shares.
        // optimal_b = 5_000 * 40_000 / 10_000 = 20_000 ✓
        assert_ok!(VitreusDex::add_liquidity(
            RuntimeOrigin::signed(BOB),
            usdc(),
            vnrg(),
            5_000,
            20_000,
            0,
            0,
        ));

        assert_eq!(TotalLiquidity::<Test>::get(pair()), Some(30_000));
        let bob_pos = LiquidityPositions::<Test>::get(BOB, pair()).unwrap();
        assert_eq!(bob_pos.shares, 10_000);

        let pool = Pools::<Test>::get(pair()).unwrap();
        assert_eq!(pool.reserve_a, 15_000);
        assert_eq!(pool.reserve_b, 60_000);
    });
}

#[test]
fn test_remove_liquidity_full() {
    new_test_ext().execute_with(|| {
        assert_ok!(VitreusDex::create_pool(
            RuntimeOrigin::root(),
            usdc(),
            vnrg(),
            10,
        ));
        assert_ok!(VitreusDex::add_liquidity(
            RuntimeOrigin::signed(ALICE),
            usdc(),
            vnrg(),
            10_000,
            40_000,
            0,
            0,
        ));

        // Alice has 19_000 shares out of 20_000 total.
        // amount_a = 19_000 * 10_000 / 20_000 = 9_500
        // amount_b = 19_000 * 40_000 / 20_000 = 38_000
        assert_ok!(VitreusDex::remove_liquidity(
            RuntimeOrigin::signed(ALICE),
            usdc(),
            vnrg(),
            19_000,
            0,
            0,
        ));

        let pool = Pools::<Test>::get(pair()).unwrap();
        // MINIMUM_LIQUIDITY (1_000 shares) remains — reserves can't be fully drained.
        assert_eq!(pool.reserve_a, 500);
        assert_eq!(pool.reserve_b, 2_000);
        assert_eq!(TotalLiquidity::<Test>::get(pair()), Some(1_000));
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
            10,
        ));
        assert_ok!(VitreusDex::add_liquidity(
            RuntimeOrigin::signed(ALICE),
            usdc(),
            vnrg(),
            10_000,
            10_000,
            0,
            0,
        ));

        // amount_in = 100, fee_tier = 10 (1.0%)
        // fee = 100 * 10 / 1000 = 1
        // amount_in_after_fee = 99
        // amount_out = 10_000 * 99 / (10_000 + 99) = 990_000 / 10_099 = 98
        assert_ok!(VitreusDex::swap_exact_tokens_for_tokens(
            RuntimeOrigin::signed(BOB),
            usdc(),
            vnrg(),
            100,
            0,
            BOB,
        ));

        let pool = Pools::<Test>::get(pair()).unwrap();
        // Finding 3: reserves track after-fee amount only.
        assert_eq!(pool.reserve_a, 10_099);
        assert_eq!(pool.reserve_b, 9_902);
        assert_eq!(pool.total_fees_collected, 1);

        System::assert_has_event(
            Event::SwapExecuted {
                who: BOB,
                asset_in: usdc(),
                asset_out: vnrg(),
                amount_in: 100,
                amount_out: 98,
                fee: 1,
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
            10,
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
            10,
        ));
        assert_ok!(VitreusDex::add_liquidity(
            RuntimeOrigin::signed(ALICE),
            usdc(),
            vnrg(),
            10_000,
            10_000,
            0,
            0,
        ));

        // Actual output is 98; demand 500 to trigger slippage error.
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
        // Finding 12: verify cumulative counter.
        assert_eq!(TotalEnergySold::<Test>::get(), 42);
    });
}

// ---------------------------------------------------------------------------
// Internal helpers (do_* fns and the PoolManager trait), called directly —
// not through extrinsics. These are the entry points a graduating launchpad
// pallet will use.
// ---------------------------------------------------------------------------

#[test]
fn do_create_pool_direct_creates_pool_without_origin() {
    new_test_ext().execute_with(|| {
        assert!(!VitreusDex::pool_exists(usdc(), vnrg()));

        assert_ok!(VitreusDex::do_create_pool(usdc(), vnrg(), 3));

        assert!(VitreusDex::pool_exists(usdc(), vnrg()));
        // Pair order is irrelevant to the lookup.
        assert!(VitreusDex::pool_exists(vnrg(), usdc()));

        let pool = Pools::<Test>::get(pair()).expect("pool stored");
        assert_eq!(pool.reserve_a, 0);
        assert_eq!(pool.reserve_b, 0);
        assert_eq!(pool.fee_tier, 3);
        assert_eq!(TotalLiquidity::<Test>::get(pair()), Some(0));
        System::assert_has_event(
            Event::PoolCreated { asset_a: usdc(), asset_b: vnrg(), fee_tier: 3 }.into(),
        );

        // The extrinsic gate is untouched: a signed (non-Manage) origin is
        // still rejected even though the helper itself is origin-free.
        assert_noop!(
            VitreusDex::create_pool(RuntimeOrigin::signed(ALICE), usdc(), native(), 3),
            sp_runtime::DispatchError::BadOrigin
        );
        assert!(!VitreusDex::pool_exists(usdc(), native()));
    });
}

#[test]
fn do_create_pool_direct_enforces_fee_tier_and_uniqueness() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            VitreusDex::do_create_pool(usdc(), vnrg(), 5),
            Error::<Test>::InvalidFeeTier
        );
        assert_ok!(VitreusDex::do_create_pool(usdc(), vnrg(), 1));
        // Reversed order must still collide with the canonical pair.
        assert_noop!(
            VitreusDex::do_create_pool(vnrg(), usdc(), 1),
            Error::<Test>::PoolAlreadyExists
        );
    });
}

#[test]
fn do_add_liquidity_for_direct_credits_who_and_returns_shares() {
    new_test_ext().execute_with(|| {
        assert_ok!(VitreusDex::do_create_pool(usdc(), vnrg(), 10));

        let bob_usdc_before = Assets::balance(USDC_ID, &BOB);
        let bob_vnrg_before = Assets::balance(VNRG_ID, &BOB);

        // First deposit: sqrt(10_000 * 40_000) = 20_000 total, 1_000 locked.
        let minted =
            VitreusDex::do_add_liquidity_for(&BOB, usdc(), vnrg(), 10_000, 40_000, 0, 0)
                .expect("first deposit");
        assert_eq!(minted, 19_000);

        // Tokens came out of `who`, not the caller/anyone else.
        assert_eq!(Assets::balance(USDC_ID, &BOB), bob_usdc_before - 10_000);
        assert_eq!(Assets::balance(VNRG_ID, &BOB), bob_vnrg_before - 40_000);
        assert_eq!(Assets::balance(USDC_ID, &ALICE), 1_000_000);

        let pos = LiquidityPositions::<Test>::get(BOB, pair()).expect("position");
        assert_eq!(pos.shares, 19_000);
        assert_eq!(pos.entry_block, 1);
        assert_eq!(pos.locked_until, None);
        assert_eq!(TotalLiquidity::<Test>::get(pair()), Some(20_000));

        let pool = Pools::<Test>::get(pair()).unwrap();
        assert_eq!(pool.reserve_a, 10_000);
        assert_eq!(pool.reserve_b, 40_000);

        System::assert_has_event(
            Event::LiquidityAdded {
                provider: BOB,
                asset_a: usdc(),
                asset_b: vnrg(),
                amount_a: 10_000,
                amount_b: 40_000,
                shares_minted: 19_000,
            }
            .into(),
        );

        // Second deposit by a different account: proportional shares, and
        // the return value matches the storage delta.
        let minted2 =
            VitreusDex::do_add_liquidity_for(&CHARLIE, usdc(), vnrg(), 5_000, 20_000, 0, 0)
                .expect("second deposit");
        assert_eq!(minted2, 10_000);
        assert_eq!(LiquidityPositions::<Test>::get(CHARLIE, pair()).unwrap().shares, 10_000);
        assert_eq!(TotalLiquidity::<Test>::get(pair()), Some(30_000));
    });
}

#[test]
fn do_add_liquidity_for_direct_rejects_missing_pool_and_slippage() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            VitreusDex::do_add_liquidity_for(&BOB, usdc(), vnrg(), 10_000, 40_000, 0, 0),
            Error::<Test>::PoolNotFound
        );

        assert_ok!(VitreusDex::do_create_pool(usdc(), vnrg(), 10));
        assert_noop!(
            VitreusDex::do_add_liquidity_for(&BOB, usdc(), vnrg(), 0, 40_000, 0, 0),
            Error::<Test>::ZeroAmount
        );
        assert_ok!(VitreusDex::do_add_liquidity_for(&BOB, usdc(), vnrg(), 10_000, 40_000, 0, 0));

        // Imbalanced top-up: optimal_b = 5_000 * 40_000 / 10_000 = 20_000,
        // which is below the caller's 25_000 floor → slippage error, and
        // (via assert_noop) no state change.
        assert_noop!(
            VitreusDex::do_add_liquidity_for(&CHARLIE, usdc(), vnrg(), 5_000, 30_000, 0, 25_000),
            Error::<Test>::SlippageExceeded
        );
    });
}

#[test]
fn do_lock_liquidity_for_direct_locks_position() {
    new_test_ext().execute_with(|| {
        assert_ok!(VitreusDex::do_create_pool(usdc(), vnrg(), 10));
        assert_ok!(VitreusDex::do_add_liquidity_for(&BOB, usdc(), vnrg(), 10_000, 40_000, 0, 0));

        // No position → InsufficientShares.
        assert_noop!(
            VitreusDex::do_lock_liquidity_for(&CHARLIE, usdc(), vnrg(), 50),
            Error::<Test>::InsufficientShares
        );

        assert_ok!(VitreusDex::do_lock_liquidity_for(&BOB, usdc(), vnrg(), 50));
        assert_eq!(
            LiquidityPositions::<Test>::get(BOB, pair()).unwrap().locked_until,
            Some(50)
        );
        System::assert_has_event(
            Event::LiquidityLocked { who: BOB, pool: pair(), locked_until: 50 }.into(),
        );

        // The lock is honoured by remove_liquidity until the block is reached.
        assert_noop!(
            VitreusDex::remove_liquidity(RuntimeOrigin::signed(BOB), usdc(), vnrg(), 1_000, 0, 0),
            Error::<Test>::PoolLocked
        );
        System::set_block_number(50);
        assert_ok!(VitreusDex::remove_liquidity(
            RuntimeOrigin::signed(BOB),
            usdc(),
            vnrg(),
            1_000,
            0,
            0
        ));
    });
}

#[test]
fn pool_manager_trait_supports_full_graduation_flow() {
    // Everything a graduating launchpad needs, exercised through the trait
    // surface only (as a dependent pallet would see it).
    type Dex = VitreusDex;
    fn dex_pool_exists(a: NativeOrAssetId, b: NativeOrAssetId) -> bool {
        <Dex as PoolManager<u128, NativeOrAssetId, u128, u64>>::pool_exists(a, b)
    }

    new_test_ext().execute_with(|| {
        assert!(!dex_pool_exists(native(), usdc()));

        assert_ok!(<Dex as PoolManager<u128, NativeOrAssetId, u128, u64>>::create_pool(
            native(),
            usdc(),
            3
        ));
        assert!(dex_pool_exists(native(), usdc()));

        // Native/USDC: 100_000 native (balances) + 40_000 USDC (assets) from CHARLIE.
        let native_before = Balances::free_balance(CHARLIE);
        let minted = <Dex as PoolManager<u128, NativeOrAssetId, u128, u64>>::add_liquidity_for(
            &CHARLIE,
            native(),
            usdc(),
            100_000,
            40_000,
            100_000,
            40_000,
        )
        .expect("seed liquidity");
        // sqrt(100_000 * 40_000) = 63_245; minus MINIMUM_LIQUIDITY.
        assert_eq!(minted, 63_245 - 1_000);
        assert_eq!(Balances::free_balance(CHARLIE), native_before - 100_000);

        assert_ok!(<Dex as PoolManager<u128, NativeOrAssetId, u128, u64>>::lock_liquidity_for(
            &CHARLIE,
            native(),
            usdc(),
            1_000
        ));

        let canonical = VitreusDex::canonical_pair(native(), usdc());
        let pos = LiquidityPositions::<Test>::get(CHARLIE, canonical.clone()).expect("position");
        assert_eq!(pos.shares, minted);
        assert_eq!(pos.locked_until, Some(1_000));
        assert_eq!(TotalLiquidity::<Test>::get(canonical), Some(63_245));
    });
}

// ============================================================================
// D1: 18-decimal scale. Reserve products at launchpad seed size
// (10^22 VTRS × 2·10^26 tokens ≈ 2·10^48) exceed u128::MAX ≈ 3.4·10^38, so
// every multiply-then-divide in the pool math must go through
// `HigherPrecisionBalance` (U256). These tests fail with `Error::Overflow`
// on the pre-D1 u128 arithmetic.
// ============================================================================

/// Asset id for a launchpad-style 18-decimal token in the mock registry.
const MEME_ID: u32 = 7;
/// 10^18 sub-units per whole unit, VTRS and launch tokens alike.
const UNIT: u128 = 1_000_000_000_000_000_000;
/// Seed amounts: 10_000 VTRS against 200_000_000 tokens.
const SEED_NATIVE: u128 = 10_000 * UNIT; // 10^22
const SEED_TOKEN: u128 = 200_000_000 * UNIT; // 2·10^26

fn meme() -> NativeOrAssetId {
    NativeOrAssetId::WithId(MEME_ID)
}

/// Funds ALICE (seeder) and BOB (trader) at 18-decimal scale and creates the
/// Native/MEME pool at the 0.3% tier. Returns the canonical pair key.
fn setup_scale_pool() -> (NativeOrAssetId, NativeOrAssetId) {
    // 10^27 native for the seeder and trader — well above u64 but far below u128::MAX.
    assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), ALICE, 1_000_000_000 * UNIT));
    assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), BOB, 1_000_000_000 * UNIT));
    assert_ok!(Assets::force_create(RuntimeOrigin::root(), MEME_ID, ALICE, false, 1));
    assert_ok!(Assets::mint(RuntimeOrigin::signed(ALICE), MEME_ID, ALICE, 1_000_000_000 * UNIT));
    assert_ok!(Assets::mint(RuntimeOrigin::signed(ALICE), MEME_ID, BOB, 1_000_000_000 * UNIT));
    assert_ok!(VitreusDex::create_pool(RuntimeOrigin::root(), native(), meme(), 3));
    VitreusDex::canonical_pair(native(), meme())
}

/// Reference: floor(sqrt(a * b)) computed in U256, narrowed.
fn isqrt_u256(a: u128, b: u128) -> u128 {
    let p = sp_core::U256::from(a) * sp_core::U256::from(b);
    let r: sp_core::U256 = p.integer_sqrt();
    r.try_into().expect("sqrt of a u256 product of two u128 fits u128")
}

/// Reference: floor(a * b / c) in U256, narrowed.
fn mul_div_u256(a: u128, b: u128, c: u128) -> u128 {
    let x = sp_core::U256::from(a) * sp_core::U256::from(b) / sp_core::U256::from(c);
    x.try_into().expect("reference result fits u128")
}

#[test]
fn scale_add_liquidity_first_deposit_at_seed_amounts() {
    new_test_ext().execute_with(|| {
        let key = setup_scale_pool();

        // Pre-D1: `amount_a.checked_mul(amount_b)` overflows u128 → Error::Overflow.
        assert_ok!(VitreusDex::add_liquidity(
            RuntimeOrigin::signed(ALICE),
            native(),
            meme(),
            SEED_NATIVE,
            SEED_TOKEN,
            SEED_NATIVE,
            SEED_TOKEN,
        ));

        let expected_total = isqrt_u256(SEED_NATIVE, SEED_TOKEN);
        assert_eq!(TotalLiquidity::<Test>::get(key.clone()), Some(expected_total));
        let pos = LiquidityPositions::<Test>::get(ALICE, key.clone()).expect("position");
        assert_eq!(pos.shares, expected_total - u128::from(crate::MINIMUM_LIQUIDITY));

        let pool = Pools::<Test>::get(key).unwrap();
        // canonical_pair orders Native before WithId, so reserve_a is VTRS.
        assert_eq!(pool.reserve_a, SEED_NATIVE);
        assert_eq!(pool.reserve_b, SEED_TOKEN);
    });
}

#[test]
fn scale_add_liquidity_subsequent_deposit_uses_optimal_amounts() {
    new_test_ext().execute_with(|| {
        let key = setup_scale_pool();
        assert_ok!(VitreusDex::add_liquidity(
            RuntimeOrigin::signed(ALICE),
            native(),
            meme(),
            SEED_NATIVE,
            SEED_TOKEN,
            0,
            0,
        ));
        let total_before = TotalLiquidity::<Test>::get(key.clone()).unwrap();

        // BOB offers 1_000 VTRS and far too many tokens; optimal_b = amount_a * reserve_b / reserve_a.
        // Pre-D1: `amount_a.checked_mul(&pool.reserve_b)` overflows.
        let offer_native = 1_000 * UNIT;
        let offer_token = 100_000_000 * UNIT;
        let optimal_token = mul_div_u256(offer_native, SEED_TOKEN, SEED_NATIVE);
        assert!(optimal_token < offer_token);

        let token_before = Assets::balance(MEME_ID, BOB);
        assert_ok!(VitreusDex::add_liquidity(
            RuntimeOrigin::signed(BOB),
            native(),
            meme(),
            offer_native,
            offer_token,
            offer_native,
            optimal_token,
        ));
        // Only the optimal token amount was pulled (no donation of excess).
        assert_eq!(token_before - Assets::balance(MEME_ID, BOB), optimal_token);

        // shares = min(actual_a * total / reserve_a, actual_b * total / reserve_b), floored.
        let share_a = mul_div_u256(offer_native, total_before, SEED_NATIVE);
        let share_b = mul_div_u256(optimal_token, total_before, SEED_TOKEN);
        let expected = share_a.min(share_b);
        let pos = LiquidityPositions::<Test>::get(BOB, key).expect("position");
        assert_eq!(pos.shares, expected);
    });
}

#[test]
fn scale_swap_native_for_token_at_seed_amounts() {
    new_test_ext().execute_with(|| {
        let key = setup_scale_pool();
        assert_ok!(VitreusDex::add_liquidity(
            RuntimeOrigin::signed(ALICE),
            native(),
            meme(),
            SEED_NATIVE,
            SEED_TOKEN,
            0,
            0,
        ));

        // 100 VTRS in at 0.3%: fee = floor(amount_in * 3 / 1000).
        let amount_in = 100 * UNIT;
        let fee = amount_in * 3 / 1_000;
        let after_fee = amount_in - fee;
        // amount_out = floor(reserve_out * after_fee / (reserve_in + after_fee)).
        // Pre-D1: `reserve_out.checked_mul(&amount_in_after_fee)` overflows.
        let expected_out = mul_div_u256(SEED_TOKEN, after_fee, SEED_NATIVE + after_fee);
        assert!(expected_out > 0);

        let token_before = Assets::balance(MEME_ID, BOB);
        assert_ok!(VitreusDex::swap_exact_tokens_for_tokens(
            RuntimeOrigin::signed(BOB),
            native(),
            meme(),
            amount_in,
            expected_out,
            BOB,
        ));
        assert_eq!(Assets::balance(MEME_ID, BOB) - token_before, expected_out);

        let pool = Pools::<Test>::get(key).unwrap();
        assert_eq!(pool.reserve_a, SEED_NATIVE + after_fee);
        assert_eq!(pool.reserve_b, SEED_TOKEN - expected_out);
        assert_eq!(pool.total_fees_collected, fee);

        // Constant product must not decrease (floor on amount_out favours the pool).
        let k_before = sp_core::U256::from(SEED_NATIVE) * sp_core::U256::from(SEED_TOKEN);
        let k_after = sp_core::U256::from(pool.reserve_a) * sp_core::U256::from(pool.reserve_b);
        assert!(k_after >= k_before);
    });
}

#[test]
fn scale_swap_token_for_native_at_seed_amounts() {
    new_test_ext().execute_with(|| {
        let key = setup_scale_pool();
        assert_ok!(VitreusDex::add_liquidity(
            RuntimeOrigin::signed(ALICE),
            native(),
            meme(),
            SEED_NATIVE,
            SEED_TOKEN,
            0,
            0,
        ));

        // 1_000_000 tokens in (0.5% of the token reserve).
        let amount_in = 1_000_000 * UNIT;
        let fee = amount_in * 3 / 1_000;
        let after_fee = amount_in - fee;
        // Flipped direction: reserve_in is the token side, reserve_out the native side.
        let expected_out = mul_div_u256(SEED_NATIVE, after_fee, SEED_TOKEN + after_fee);
        assert!(expected_out > 0);

        let native_before = Balances::free_balance(BOB);
        assert_ok!(VitreusDex::swap_exact_tokens_for_tokens(
            RuntimeOrigin::signed(BOB),
            meme(),
            native(),
            amount_in,
            expected_out,
            BOB,
        ));
        assert_eq!(Balances::free_balance(BOB) - native_before, expected_out);

        let pool = Pools::<Test>::get(key).unwrap();
        assert_eq!(pool.reserve_a, SEED_NATIVE - expected_out);
        assert_eq!(pool.reserve_b, SEED_TOKEN + after_fee);

        let k_before = sp_core::U256::from(SEED_NATIVE) * sp_core::U256::from(SEED_TOKEN);
        let k_after = sp_core::U256::from(pool.reserve_a) * sp_core::U256::from(pool.reserve_b);
        assert!(k_after >= k_before);
    });
}

#[test]
fn scale_remove_liquidity_at_seed_amounts() {
    new_test_ext().execute_with(|| {
        let key = setup_scale_pool();
        assert_ok!(VitreusDex::add_liquidity(
            RuntimeOrigin::signed(ALICE),
            native(),
            meme(),
            SEED_NATIVE,
            SEED_TOKEN,
            0,
            0,
        ));
        let total = TotalLiquidity::<Test>::get(key.clone()).unwrap();
        let pos = LiquidityPositions::<Test>::get(ALICE, key.clone()).unwrap();
        // Withdraw half of ALICE's shares.
        let shares = pos.shares / 2;

        // amount = floor(shares * reserve / total_shares).
        // Pre-D1: `shares.checked_mul(&pool.reserve_a)` overflows.
        let expected_native = mul_div_u256(shares, SEED_NATIVE, total);
        let expected_token = mul_div_u256(shares, SEED_TOKEN, total);

        let native_before = Balances::free_balance(ALICE);
        let token_before = Assets::balance(MEME_ID, ALICE);
        assert_ok!(VitreusDex::remove_liquidity(
            RuntimeOrigin::signed(ALICE),
            native(),
            meme(),
            shares,
            expected_native,
            expected_token,
        ));
        assert_eq!(Balances::free_balance(ALICE) - native_before, expected_native);
        assert_eq!(Assets::balance(MEME_ID, ALICE) - token_before, expected_token);

        let pool = Pools::<Test>::get(key.clone()).unwrap();
        assert_eq!(pool.reserve_a, SEED_NATIVE - expected_native);
        assert_eq!(pool.reserve_b, SEED_TOKEN - expected_token);
        assert_eq!(TotalLiquidity::<Test>::get(key), Some(total - shares));
    });
}

#[test]
fn scale_narrowing_errors_instead_of_truncating() {
    new_test_ext().execute_with(|| {
        // The only pool-math result that can exceed u128 after the product is
        // computed in U256 is the matched deposit amount
        // `optimal_b = amount_a * reserve_b / reserve_a`, when a depositor offers
        // far more of asset A than the pool ratio supports. With reserves at
        // 10^22 : 2·10^26 (ratio 2·10^4) and amount_a = 10^38, optimal_b ≈ 2·10^42
        // does not fit u128. The narrowing must surface as `Error::Overflow`
        // and leave state untouched — never truncate to a wrong amount.
        let key = setup_scale_pool();
        assert_ok!(VitreusDex::add_liquidity(
            RuntimeOrigin::signed(ALICE),
            native(),
            meme(),
            SEED_NATIVE,
            SEED_TOKEN,
            0,
            0,
        ));
        assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), BOB, u128::MAX));
        let huge_native: u128 = 100_000_000_000_000_000_000_000_000_000_000_000_000; // 10^38
        let total_before = TotalLiquidity::<Test>::get(key.clone()).unwrap();

        assert_noop!(
            VitreusDex::add_liquidity(
                RuntimeOrigin::signed(BOB),
                native(),
                meme(),
                huge_native,
                u128::MAX,
                0,
                0,
            ),
            Error::<Test>::Overflow
        );
        assert_eq!(TotalLiquidity::<Test>::get(key), Some(total_before));
        assert!(LiquidityPositions::<Test>::get(BOB, VitreusDex::canonical_pair(native(), meme())).is_none());
    });
}

// ============================================================================
// D2: reserved-asset guard + ReservedPoolSeeder. D3: monotone locks.
//
// Reserved ids in the mock are `WithId(id)` with id >= RESERVED_ASSET_BASE.
// LAUNCH_ID is an 18-decimal, non-sufficient asset so the pool sub-account
// needs a native provider before it can hold it — which is what makes the
// quote-first transfer order observable.
// ============================================================================

const LAUNCH_ID: u32 = RESERVED_ASSET_BASE + 1;

fn launch() -> NativeOrAssetId {
    NativeOrAssetId::WithId(LAUNCH_ID)
}

/// The escrow-like account that funds a seed; deliberately not ALICE/BOB.
const ESCROW: u128 = 42;

/// Mints the reserved asset and funds ESCROW at seed scale. Does NOT create a pool.
fn setup_reserved_asset() {
    assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), ESCROW, 1_000_000 * UNIT));
    assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), BOB, 1_000_000 * UNIT));
    assert_ok!(Assets::force_create(RuntimeOrigin::root(), LAUNCH_ID, ALICE, false, 1));
    assert_ok!(Assets::mint(RuntimeOrigin::signed(ALICE), LAUNCH_ID, ESCROW, 1_000_000_000 * UNIT));
    assert_ok!(Assets::mint(RuntimeOrigin::signed(ALICE), LAUNCH_ID, BOB, 1_000_000_000 * UNIT));
}

fn seed(who: u128) -> Result<u128, sp_runtime::DispatchError> {
    <VitreusDex as crate::ReservedPoolSeeder<u128, NativeOrAssetId, u128, u64>>::seed_reserved_pool_for(
        &who,
        launch(),
        native(),
        SEED_TOKEN,
        SEED_NATIVE,
        3,
    )
}

#[test]
fn canonical_pair_orders_native_before_with_id() {
    // `seed_reserved_pool_for` does not depend on this, but `do_add_liquidity_for`
    // does (it transfers pair.0 first). Pin it so an encoding change is noticed.
    let (a, b) = VitreusDex::canonical_pair(launch(), native());
    assert_eq!(a, native());
    assert_eq!(b, launch());
    let (a, b) = VitreusDex::canonical_pair(native(), launch());
    assert_eq!(a, native());
    assert_eq!(b, launch());
}

#[test]
fn reserved_asset_rejected_by_create_pool_for_every_caller() {
    new_test_ext().execute_with(|| {
        setup_reserved_asset();

        // ManageOrigin (root) via the extrinsic, both pair orderings.
        assert_noop!(
            VitreusDex::create_pool(RuntimeOrigin::root(), native(), launch(), 3),
            Error::<Test>::ReservedAsset
        );
        assert_noop!(
            VitreusDex::create_pool(RuntimeOrigin::root(), launch(), native(), 3),
            Error::<Test>::ReservedAsset
        );
        // Reserved against a non-native, non-reserved asset too.
        assert_noop!(
            VitreusDex::create_pool(RuntimeOrigin::root(), usdc(), launch(), 3),
            Error::<Test>::ReservedAsset
        );
        // Signed origin is rejected before it reaches the guard (BadOrigin), so
        // check the origin-free paths explicitly: the PoolManager trait ...
        assert_noop!(
            <VitreusDex as PoolManager<u128, NativeOrAssetId, u128, u64>>::create_pool(
                native(),
                launch(),
                3
            ),
            Error::<Test>::ReservedAsset
        );
        // ... and the bare helper.
        assert_noop!(
            VitreusDex::do_create_pool(launch(), native(), 3),
            Error::<Test>::ReservedAsset
        );
        // The guard runs before the fee-tier check, so a bad tier does not mask it.
        assert_noop!(
            VitreusDex::do_create_pool(launch(), native(), 7),
            Error::<Test>::ReservedAsset
        );

        assert!(!VitreusDex::pool_exists(native(), launch()));
        assert!(Pools::<Test>::get(VitreusDex::canonical_pair(native(), launch())).is_none());

        // Non-reserved pairs are unaffected.
        assert_ok!(VitreusDex::create_pool(RuntimeOrigin::root(), usdc(), vnrg(), 3));
    });
}

#[test]
fn seed_reserved_pool_creates_pool_deposits_stored_amounts_and_locks_forever() {
    new_test_ext().execute_with(|| {
        setup_reserved_asset();
        let key = VitreusDex::canonical_pair(native(), launch());
        let pool_account = VitreusDex::pool_account_for(native(), launch());
        // Fresh pool account: no native balance, so a token-first transfer
        // would fail for this non-sufficient asset. The seed must go quote-first.
        assert_eq!(Balances::free_balance(pool_account), 0);

        let native_before = Balances::free_balance(ESCROW);
        let token_before = Assets::balance(LAUNCH_ID, ESCROW);

        let shares = seed(ESCROW).expect("seed");

        let expected_total = isqrt_u256(SEED_NATIVE, SEED_TOKEN);
        assert_eq!(shares, expected_total - u128::from(crate::MINIMUM_LIQUIDITY));
        assert_eq!(TotalLiquidity::<Test>::get(key.clone()), Some(expected_total));

        // Exactly the stored amounts left the escrow and sit in the pool account.
        assert_eq!(native_before - Balances::free_balance(ESCROW), SEED_NATIVE);
        assert_eq!(token_before - Assets::balance(LAUNCH_ID, ESCROW), SEED_TOKEN);
        assert_eq!(Balances::free_balance(pool_account), SEED_NATIVE);
        assert_eq!(Assets::balance(LAUNCH_ID, pool_account), SEED_TOKEN);

        let pool = Pools::<Test>::get(key.clone()).unwrap();
        assert_eq!(pool.pool_account, pool_account);
        assert_eq!(pool.fee_tier, 3);
        assert_eq!((pool.reserve_a, pool.reserve_b), (SEED_NATIVE, SEED_TOKEN));

        // Position: owned by the seeder, locked to the max block.
        let pos = LiquidityPositions::<Test>::get(ESCROW, key.clone()).expect("position");
        assert_eq!(pos.shares, shares);
        assert_eq!(pos.locked_until, Some(u64::MAX));

        // Nobody else holds a position in this pool.
        assert_eq!(LiquidityPositions::<Test>::iter_prefix(ALICE).count(), 0);
        assert_eq!(LiquidityPositions::<Test>::iter_prefix(BOB).count(), 0);

        // The lock holds at the last representable block.
        System::set_block_number(u64::MAX - 1);
        assert_noop!(
            VitreusDex::remove_liquidity(RuntimeOrigin::signed(ESCROW), native(), launch(), 1, 0, 0),
            Error::<Test>::PoolLocked
        );
        // ... and cannot be shortened through either lock path (D3).
        assert_noop!(
            VitreusDex::lock_liquidity(RuntimeOrigin::signed(ESCROW), native(), launch(), 10),
            Error::<Test>::LockCannotBeShortened
        );
        assert_noop!(
            <VitreusDex as PoolManager<u128, NativeOrAssetId, u128, u64>>::lock_liquidity_for(
                &ESCROW,
                native(),
                launch(),
                u64::MAX - 1
            ),
            Error::<Test>::LockCannotBeShortened
        );
        assert_eq!(
            LiquidityPositions::<Test>::get(ESCROW, key.clone()).unwrap().locked_until,
            Some(u64::MAX)
        );

        System::assert_has_event(
            Event::ReservedPoolSeeded {
                who: ESCROW,
                asset: launch(),
                quote: native(),
                amount_asset: SEED_TOKEN,
                amount_quote: SEED_NATIVE,
                shares,
            }
            .into(),
        );
        System::assert_has_event(
            Event::LiquidityLocked { who: ESCROW, pool: key, locked_until: u64::MAX }.into(),
        );

        // Post-seed, ordinary LPs may join through the normal path.
        assert_ok!(VitreusDex::add_liquidity(
            RuntimeOrigin::signed(BOB),
            native(),
            launch(),
            UNIT,
            u128::MAX / 4,
            0,
            0,
        ));
    });
}

#[test]
fn seed_sweeps_pre_seed_donations_so_opening_price_is_stored_ratio() {
    new_test_ext().execute_with(|| {
        setup_reserved_asset();
        let key = VitreusDex::canonical_pair(native(), launch());
        let pool_account = VitreusDex::pool_account_for(native(), launch());

        // The treasury exists (has a provider) so it can receive the token.
        assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), TREASURY, UNIT));

        // FM-02: park balances at the predictable pool address before seeding.
        // Token-heavy donation would push the opening price DOWN if absorbed;
        // native would push it UP. Use both.
        let donated_token = 3 * SEED_TOKEN; // 6·10^26, would triple the token reserve if absorbed
        let donated_native = 5 * UNIT;
        assert_ok!(Balances::transfer_allow_death(
            RuntimeOrigin::signed(BOB),
            pool_account,
            donated_native
        ));
        assert_ok!(Assets::transfer(RuntimeOrigin::signed(BOB), LAUNCH_ID, pool_account, donated_token));
        assert_eq!(Balances::free_balance(pool_account), donated_native);
        assert_eq!(Assets::balance(LAUNCH_ID, pool_account), donated_token);
        assert_eq!(Balances::free_balance(TREASURY), UNIT);
        assert_eq!(Assets::balance(LAUNCH_ID, TREASURY), 0);

        let shares = seed(ESCROW).expect("seed");

        // Donations landed in Treasury, not in the pool.
        assert_eq!(Balances::free_balance(TREASURY), UNIT + donated_native);
        assert_eq!(Assets::balance(LAUNCH_ID, TREASURY), donated_token);
        assert_eq!(Balances::free_balance(pool_account), SEED_NATIVE);
        assert_eq!(Assets::balance(LAUNCH_ID, pool_account), SEED_TOKEN);
        System::assert_has_event(
            Event::PreSeedBalanceSwept {
                pool: key.clone(),
                asset: launch(),
                amount: donated_token,
                to: TREASURY,
            }
            .into(),
        );
        System::assert_has_event(
            Event::PreSeedBalanceSwept {
                pool: key.clone(),
                asset: native(),
                amount: donated_native,
                to: TREASURY,
            }
            .into(),
        );

        // Tracked reserves and shares reflect only the stored amounts.
        let pool = Pools::<Test>::get(key.clone()).unwrap();
        assert_eq!((pool.reserve_a, pool.reserve_b), (SEED_NATIVE, SEED_TOKEN));
        assert_eq!(shares, isqrt_u256(SEED_NATIVE, SEED_TOKEN) - u128::from(crate::MINIMUM_LIQUIDITY));

        // The decisive check: the first swap runs `sync_reserves`, which reads
        // the pool account's live balances. If the donation had been left in
        // place it would be absorbed here and the fill would differ from the
        // constant-product quote on the stored reserves.
        let amount_in = 100 * UNIT;
        let after_fee = amount_in - amount_in * 3 / 1_000;
        let expected_out = mul_div_u256(SEED_TOKEN, after_fee, SEED_NATIVE + after_fee);
        let bob_before = Assets::balance(LAUNCH_ID, BOB);
        assert_ok!(VitreusDex::swap_exact_tokens_for_tokens(
            RuntimeOrigin::signed(BOB),
            native(),
            launch(),
            amount_in,
            expected_out,
            BOB,
        ));
        assert_eq!(Assets::balance(LAUNCH_ID, BOB) - bob_before, expected_out);
        let pool = Pools::<Test>::get(key).unwrap();
        assert_eq!(pool.reserve_a, SEED_NATIVE + after_fee);
        assert_eq!(pool.reserve_b, SEED_TOKEN - expected_out);
    });
}

#[test]
fn seed_is_transactional_when_sweep_or_deposit_fails() {
    new_test_ext().execute_with(|| {
        setup_reserved_asset();
        let pool_account = VitreusDex::pool_account_for(native(), launch());
        assert_ok!(Balances::transfer_allow_death(RuntimeOrigin::signed(BOB), pool_account, 5 * UNIT));

        // An escrow that cannot fund the deposit: the sweep must not stick.
        assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), ESCROW, 1));
        assert!(seed(ESCROW).is_err());
        assert_eq!(Balances::free_balance(pool_account), 5 * UNIT);
        assert_eq!(Balances::free_balance(TREASURY), 0);
        assert!(!VitreusDex::pool_exists(native(), launch()));
        assert!(Pools::<Test>::get(VitreusDex::canonical_pair(native(), launch())).is_none());
    });
}

#[test]
fn seed_rejects_wrong_assets_double_seed_and_mismatched_adoption() {
    new_test_ext().execute_with(|| {
        setup_reserved_asset();
        let s = |asset, quote, fee| {
            <VitreusDex as crate::ReservedPoolSeeder<u128, NativeOrAssetId, u128, u64>>::seed_reserved_pool_for(
                &ESCROW, asset, quote, SEED_TOKEN, SEED_NATIVE, fee,
            )
        };

        // Non-reserved asset: the seeder must not be a way around ManageOrigin.
        assert_noop!(s(usdc(), native(), 3), Error::<Test>::NotReservedAsset);
        // Reserved quote.
        assert_noop!(s(launch(), NativeOrAssetId::WithId(RESERVED_ASSET_BASE + 2), 3), Error::<Test>::NotReservedAsset);
        // Same asset twice.
        assert_noop!(s(launch(), launch(), 3), Error::<Test>::NotReservedAsset);
        // Bad fee tier.
        assert_noop!(s(launch(), native(), 7), Error::<Test>::InvalidFeeTier);
        // Zero amounts.
        assert_noop!(
            <VitreusDex as crate::ReservedPoolSeeder<u128, NativeOrAssetId, u128, u64>>::seed_reserved_pool_for(
                &ESCROW, launch(), native(), 0, SEED_NATIVE, 3,
            ),
            Error::<Test>::ZeroAmount
        );

        // Adoption of an empty pool record (can only arise from storage that
        // predates the guard; simulate it directly). Fee tier must match.
        let key = VitreusDex::canonical_pair(native(), launch());
        Pools::<Test>::insert(
            key.clone(),
            crate::PoolInfo {
                reserve_a: 0,
                reserve_b: 0,
                fee_tier: 10,
                total_fees_collected: 0,
                pool_account: VitreusDex::pool_account_for(native(), launch()),
            },
        );
        TotalLiquidity::<Test>::insert(key.clone(), 0u128);
        assert_noop!(s(launch(), native(), 3), Error::<Test>::InvalidFeeTier);
        assert_ok!(s(launch(), native(), 10));
        assert_eq!(Pools::<Test>::get(key.clone()).unwrap().reserve_b, SEED_TOKEN);

        // Second seed: the pool has shares now.
        assert_noop!(s(launch(), native(), 10), Error::<Test>::PoolAlreadySeeded);
    });
}

#[test]
fn lock_can_be_extended_but_never_shortened() {
    new_test_ext().execute_with(|| {
        assert_ok!(VitreusDex::do_create_pool(usdc(), vnrg(), 10));
        assert_ok!(VitreusDex::do_add_liquidity_for(&BOB, usdc(), vnrg(), 10_000, 40_000, 0, 0));

        // Unlocked → 50.
        assert_ok!(VitreusDex::lock_liquidity(RuntimeOrigin::signed(BOB), usdc(), vnrg(), 50));
        assert_eq!(LiquidityPositions::<Test>::get(BOB, pair()).unwrap().locked_until, Some(50));
        // Same block: idempotent.
        assert_ok!(VitreusDex::lock_liquidity(RuntimeOrigin::signed(BOB), usdc(), vnrg(), 50));
        // Extend.
        assert_ok!(VitreusDex::lock_liquidity(RuntimeOrigin::signed(BOB), usdc(), vnrg(), 80));
        assert_eq!(LiquidityPositions::<Test>::get(BOB, pair()).unwrap().locked_until, Some(80));
        // Shorten via extrinsic and via trait: rejected, state unchanged.
        assert_noop!(
            VitreusDex::lock_liquidity(RuntimeOrigin::signed(BOB), usdc(), vnrg(), 79),
            Error::<Test>::LockCannotBeShortened
        );
        assert_noop!(
            <VitreusDex as PoolManager<u128, NativeOrAssetId, u128, u64>>::lock_liquidity_for(
                &BOB,
                usdc(),
                vnrg(),
                0
            ),
            Error::<Test>::LockCannotBeShortened
        );
        assert_eq!(LiquidityPositions::<Test>::get(BOB, pair()).unwrap().locked_until, Some(80));

        // Once the lock has expired it can still only move forward, not back
        // to a value below the old one.
        System::set_block_number(100);
        assert_noop!(
            VitreusDex::lock_liquidity(RuntimeOrigin::signed(BOB), usdc(), vnrg(), 60),
            Error::<Test>::LockCannotBeShortened
        );
        assert_ok!(VitreusDex::lock_liquidity(RuntimeOrigin::signed(BOB), usdc(), vnrg(), 200));
    });
}

#[test]
fn seed_burns_pre_seed_donation_when_recipient_cannot_receive_it() {
    // Name kept from the earlier burn-fallback design; the behaviour is now
    // the opposite: a recipient that cannot receive the swept asset makes the
    // seed fail loudly, so a mis-wired `ExcessRecipient` cannot pass as
    // correct operation. Nothing is burned, nothing is created.
    new_test_ext().execute_with(|| {
        setup_reserved_asset();
        let key = VitreusDex::canonical_pair(native(), launch());
        let pool_account = VitreusDex::pool_account_for(native(), launch());
        // TREASURY has no native balance, so it cannot open an account for the
        // non-sufficient launch asset.
        assert_eq!(Balances::free_balance(TREASURY), 0);
        // A donor has to give the pool account a provider before it can hold
        // the non-sufficient token, so native goes in first (as an attacker would).
        let donated_native = 5 * UNIT;
        let donated_token = 3 * SEED_TOKEN;
        assert_ok!(Balances::transfer_allow_death(RuntimeOrigin::signed(BOB), pool_account, donated_native));
        assert_ok!(Assets::transfer(RuntimeOrigin::signed(BOB), LAUNCH_ID, pool_account, donated_token));
        let issuance_before = Assets::total_supply(LAUNCH_ID);
        let escrow_native_before = Balances::free_balance(ESCROW);
        let escrow_token_before = Assets::balance(LAUNCH_ID, ESCROW);

        assert_noop!(seed(ESCROW), Error::<Test>::ExcessRecipientCannotReceive);

        // No pool, no position, nothing burned, nothing moved.
        assert!(Pools::<Test>::get(key.clone()).is_none());
        assert!(TotalLiquidity::<Test>::get(key.clone()).is_none());
        assert!(LiquidityPositions::<Test>::get(ESCROW, key).is_none());
        assert_eq!(Assets::total_supply(LAUNCH_ID), issuance_before);
        assert_eq!(Assets::balance(LAUNCH_ID, pool_account), donated_token);
        assert_eq!(Balances::free_balance(pool_account), donated_native);
        assert_eq!(Assets::balance(LAUNCH_ID, TREASURY), 0);
        assert_eq!(Balances::free_balance(TREASURY), 0);
        assert_eq!(Balances::free_balance(ESCROW), escrow_native_before);
        assert_eq!(Assets::balance(LAUNCH_ID, ESCROW), escrow_token_before);

        // FM-11 recovery: fix the recipient (give it a provider) and retry —
        // permissionlessly, with no other state change needed.
        assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), TREASURY, UNIT));
        assert_ok!(seed(ESCROW));
        assert_eq!(Assets::balance(LAUNCH_ID, TREASURY), donated_token);
        assert_eq!(Balances::free_balance(TREASURY), UNIT + donated_native);
        assert_eq!(Assets::balance(LAUNCH_ID, pool_account), SEED_TOKEN);
        assert_eq!(Balances::free_balance(pool_account), SEED_NATIVE);
    });
}

// ============================================================================
// D5: amounts and minimums follow the CANONICAL pair order, not the caller's.
//
// `canonical_pair` sorts the pair (Native before WithId, lower id first) but
// the pre-D5 code left amount_a/amount_b and amount_a_min/amount_b_min bound
// to the caller's argument positions, so `add_liquidity(USDC, VTRS, 1000, 1)`
// deposited 1000 on the VTRS side and 1 on the USDC side. Same for the
// minimums of `remove_liquidity`. These tests fail on the pre-D5 code.
// ============================================================================

#[test]
fn d5_add_liquidity_non_canonical_order_maps_amounts_to_assets() {
    new_test_ext().execute_with(|| {
        // Canonical order is (Native, USDC); the caller passes (USDC, Native).
        assert_ok!(VitreusDex::create_pool(RuntimeOrigin::root(), usdc(), native(), 3));
        let key = VitreusDex::canonical_pair(usdc(), native());
        assert_eq!(key, (native(), usdc()));

        let native_before = Balances::free_balance(ALICE);
        let usdc_before = Assets::balance(USDC_ID, ALICE);
        // 100_000 USDC and 40_000 VTRS, given in the caller's (USDC, VTRS) order,
        // with exact minimums in the same order.
        assert_ok!(VitreusDex::add_liquidity(
            RuntimeOrigin::signed(ALICE),
            usdc(),
            native(),
            100_000,
            40_000,
            100_000,
            40_000,
        ));
        assert_eq!(native_before - Balances::free_balance(ALICE), 40_000, "VTRS taken");
        assert_eq!(usdc_before - Assets::balance(USDC_ID, ALICE), 100_000, "USDC taken");
        let pool = Pools::<Test>::get(key.clone()).unwrap();
        assert_eq!(pool.reserve_a, 40_000, "reserve_a is the Native side");
        assert_eq!(pool.reserve_b, 100_000, "reserve_b is the USDC side");
        // The event reports canonical order.
        System::assert_has_event(
            Event::LiquidityAdded {
                provider: ALICE,
                asset_a: native(),
                asset_b: usdc(),
                amount_a: 40_000,
                amount_b: 100_000,
                shares_minted: 63_245 - 1_000,
            }
            .into(),
        );

        // Subsequent deposit, still in the caller's order: the optimal-amount
        // calculation must use the USDC offer against the USDC reserve.
        let native_before = Balances::free_balance(BOB);
        let usdc_before = Assets::balance(USDC_ID, BOB);
        assert_ok!(VitreusDex::add_liquidity(
            RuntimeOrigin::signed(BOB),
            usdc(),
            native(),
            50_000, // USDC offered
            30_000, // VTRS offered; only 20_000 is needed at the pool ratio
            50_000,
            20_000,
        ));
        assert_eq!(usdc_before - Assets::balance(USDC_ID, BOB), 50_000);
        assert_eq!(native_before - Balances::free_balance(BOB), 20_000);
        let pool = Pools::<Test>::get(key).unwrap();
        assert_eq!((pool.reserve_a, pool.reserve_b), (60_000, 150_000));
    });
}

#[test]
fn d5_remove_liquidity_non_canonical_order_maps_minimums() {
    new_test_ext().execute_with(|| {
        assert_ok!(VitreusDex::create_pool(RuntimeOrigin::root(), native(), usdc(), 3));
        assert_ok!(VitreusDex::add_liquidity(
            RuntimeOrigin::signed(ALICE),
            native(),
            usdc(),
            40_000,
            100_000,
            0,
            0,
        ));
        let key = VitreusDex::canonical_pair(native(), usdc());
        let pos = LiquidityPositions::<Test>::get(ALICE, key.clone()).unwrap();
        let total = TotalLiquidity::<Test>::get(key.clone()).unwrap();
        let shares = pos.shares / 2;
        let expect_native = shares * 40_000 / total;
        let expect_usdc = shares * 100_000 / total;
        assert!(expect_usdc > expect_native);

        // Caller's order is (USDC, Native): the first minimum is the USDC one.
        // Pre-D5 the (large) USDC minimum was compared against the (small)
        // Native payout and the call failed with SlippageExceeded.
        let native_before = Balances::free_balance(ALICE);
        let usdc_before = Assets::balance(USDC_ID, ALICE);
        assert_ok!(VitreusDex::remove_liquidity(
            RuntimeOrigin::signed(ALICE),
            usdc(),
            native(),
            shares,
            expect_usdc,
            expect_native,
        ));
        assert_eq!(Assets::balance(USDC_ID, ALICE) - usdc_before, expect_usdc);
        assert_eq!(Balances::free_balance(ALICE) - native_before, expect_native);

        // And a minimum that is genuinely too high on the USDC side still
        // rejects (recomputed: the remaining position pays a hair more per
        // share because MINIMUM_LIQUIDITY stays burned).
        let pool = Pools::<Test>::get(key.clone()).unwrap();
        let total = TotalLiquidity::<Test>::get(key).unwrap();
        let payout_usdc = shares * pool.reserve_b / total;
        assert_noop!(
            VitreusDex::remove_liquidity(
                RuntimeOrigin::signed(ALICE),
                usdc(),
                native(),
                shares,
                payout_usdc + 1,
                0,
            ),
            Error::<Test>::SlippageExceeded
        );
    });
}

#[test]
fn d5_pool_manager_add_liquidity_for_non_canonical_order() {
    new_test_ext().execute_with(|| {
        // The in-runtime trait path (what the launchpad's rescue uses) is the
        // same body; check it through the trait with a reversed pair.
        assert_ok!(VitreusDex::do_create_pool(native(), usdc(), 3));
        let minted = <VitreusDex as PoolManager<u128, NativeOrAssetId, u128, u64>>::add_liquidity_for(
            &CHARLIE,
            usdc(),
            native(),
            100_000,
            40_000,
            100_000,
            40_000,
        )
        .expect("deposit");
        assert_eq!(minted, 63_245 - 1_000);
        let pool = Pools::<Test>::get(VitreusDex::canonical_pair(native(), usdc())).unwrap();
        assert_eq!((pool.reserve_a, pool.reserve_b), (40_000, 100_000));
    });
}
