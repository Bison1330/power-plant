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
