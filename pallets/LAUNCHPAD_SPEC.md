# pallet-launchpad — Design Specification

**Status:** design, v1 · **Target branch:** `feature/solver-marketplace` (DEX at `c7d3752`; PoolManager trait from `f3350e4`, D1 in `ce34a05`, D2+D3 in `5ea3008`, D5 in `c7d3752`) · **Date:** 2026-09-13

A bonding-curve token launchpad as a FRAME pallet. Each launch mints a fixed-supply `pallet_assets` token into a pallet-owned escrow, sells 80% of it along a constant-product curve quoted in VTRS, and on sell-out seeds a permanently locked VitreusDEX pool with the raised VTRS and the remaining 20%. No party — creator, governance, or the pallet itself — has a path to withdraw curve or pool funds.

This document is self-contained; it does not require the research brief. Where a design choice comes from a documented incident, it is tagged `FM-nn` (failure mode) and the corresponding test in §6 is named after it.

Scope decisions fixed by the owner:

- **Anti-snipe is out of v1.** §2.7 defines the hook it will plug into; nothing else in this spec references timing rules.
- **Graduation target `T` is governance-set and snapshotted per launch.** This spec fixes its type, bounds and authority (§1.4), not its value.

---

## 0. Notation, units, constants

| Symbol | Meaning | Value / type |
|---|---|---|
| `Balance` | `u128`, VTRS and launch tokens share it | runtime `Balance` |
| `S` | total supply of every launch token, base units (18 dec) | `1_000_000_000 × 10^18 = 10^27` |
| `SELLABLE` | tokens sold on the curve | `800_000_000 × 10^18` |
| `RESERVED` | tokens seeded into the pool at graduation | `200_000_000 × 10^18` (= `S − SELLABLE`) |
| `VT_FLOOR` | virtual token reserve remaining when the curve sells out | `266_666_667 × 10^18` |
| `V_t` | initial virtual token reserve | `VT_FLOOR + SELLABLE = 1_066_666_667 × 10^18` |
| `T` | graduation target, VTRS base units, per launch | governance, snapshotted |
| `V_q` | initial virtual quote reserve | `T / 3` (floor) |
| `fee_bps` | curve trading fee on the VTRS leg | governance, snapshotted |
| `BPS` | basis-point denominator | `10_000` |
| `LAUNCH_ASSET_BASE` | first asset id of the reserved range | `1u128 << 64` |
| `PALLET_ID` | `PalletId(*b"vtrs/lpd")` | |

All of `S`, `SELLABLE`, `RESERVED`, `VT_FLOOR`, `LAUNCH_ASSET_BASE` are `#[pallet::constant]` items of `Config`, so they are visible in metadata and can only change by runtime upgrade. `V_t` is not stored; it is `VT_FLOOR + SELLABLE`.

Why these numbers: with sellable fraction `s = 0.8` and a start-to-graduation price multiple `m = 16`, `V_t = s·S·√m/(√m−1)` and `V_q = T/(√m−1)`. At sell-out the marginal price is `(V_q + R)/VT_FLOOR` and the pool opens at `R/RESERVED`; with `R ≈ 3V_q` these agree to within `1.25 × 10^-9` relative (`V_t/VT_FLOOR = 4 − 1/266_666_667`; the residue is the integer rounding of `VT_FLOOR`). Invariant I7 pins a `10^-6` tolerance, comfortably above that.

**Escrow account** for launch `id`: `PALLET_ID.into_sub_account_truncating(id)` — the bare 8-byte id, not a labelled tuple: on the production 20-byte `AccountId` the derivation keeps `"modl"` + 8-byte PalletId + only 8 bytes of seed, so `("launch", id)` would leave two bytes of the id and collide after 65 536 launches. It holds the launch's VTRS (raised amount + unclaimed creator fees + ED) and its unsold tokens. It is the `owner`, `issuer`, `admin` and `freezer` of the launch asset. It has no signing key.

**Asset id** for launch `id`: `LAUNCH_ASSET_BASE + id`. The range `[2^64, 2^65)` is reserved for this pallet (§5, I12).

---

## 1. Storage

### 1.1 Governance parameters (live)

```rust
#[pallet::storage]
pub type Params<T: Config> = StorageValue<_, LaunchParams<BalanceOf<T>>, ValueQuery, DefaultParams<T>>;

pub struct LaunchParams<Balance> {
    /// Graduation target in VTRS base units. Snapshotted into each launch.
    pub graduation_target: Balance,        // T
    /// Curve trading fee on the VTRS leg, both directions. Snapshotted.
    pub curve_fee_bps: u16,
    /// Share of `curve_fee` that goes to the treasury; the remainder accrues to the creator. Snapshotted.
    pub protocol_share_bps: u16,
    /// VitreusDEX fee tier for the graduated pool (1 | 3 | 10, tenths of a percent). Snapshotted.
    pub pool_fee_tier: u32,
    /// One-off fee charged at `create_launch`, paid to treasury after funding escrow deposits. Read live.
    pub creation_fee: Balance,
}

#[pallet::storage]
pub type CreationPaused<T: Config> = StorageValue<_, bool, ValueQuery>;   // read live; affects create_launch only
```

**Bounds (enforced in `set_params`, also asserted on `DefaultParams` in tests):**

| Field | Bound | Reason |
|---|---|---|
| `graduation_target` | `MinGraduationTarget ≤ T ≤ MaxGraduationTarget`; constants `3 × 10^18 ≤ T ≤ 3 × 10^27` | lower: `V_q ≥ 1 VTRS` keeps the first buy quotable (§3.6); upper: keeps `k < 2^255` (§3.2) |
| `curve_fee_bps` | `0 ≤ fee ≤ MaxCurveFeeBps = 500` | 5% cap; peers run 1–2% |
| `protocol_share_bps` | `MinProtocolShareBps = 5_000 ≤ x ≤ 10_000` | creator share ≤ protocol share so wash-trading to farm creator fees is net-negative (FM-12) |
| `pool_fee_tier` | `∈ {1, 3, 10}` | DEX whitelist |
| `creation_fee` | `≥ MinCreationFee` where `MinCreationFee = 2 × ExistentialDeposit + MetadataDepositBase + 2 × StringLimit × MetadataDepositPerByte` | must fund escrow ED, pool-account ED headroom, and the `pallet_assets` metadata deposits paid by escrow (FM-15). No `AssetDeposit`: `fungibles::Create::create` is the force-create path and reserves none. |

**Authority:** `T::ManageOrigin` (runtime: `EnsureRoot` or `MoreThanHalfCouncil`, same as `pallet_vitreus_dex::ManageOrigin`). Changes affect only launches created afterwards (FM-10).

### 1.2 Launch registry (cold, immutable after create)

```rust
#[pallet::storage]
pub type NextLaunchId<T: Config> = StorageValue<_, LaunchId, ValueQuery>;   // LaunchId = u64, monotone, never reused

#[pallet::storage]
pub type Launches<T: Config> = StorageMap<_, Blake2_128Concat, LaunchId, Launch<T>>;

pub struct Launch<T: Config> {
    pub asset_id: AssetIdOf<T>,           // == LAUNCH_ASSET_BASE + launch_id, stored for readability
    pub creator: T::AccountId,            // informational; carries no privileges
    pub creator_fee_recipient: T::AccountId,   // mutable only via set_creator_fee_recipient
    pub escrow: T::AccountId,             // derived; stored to avoid recomputing in hot paths
    pub created_at: BlockNumberFor<T>,
    /// Everything that determines pricing and fee routing, copied from Params at create (FM-10).
    pub curve: CurveParams<BalanceOf<T>>,
    /// Hash of `curve` and the runtime constants, so a creator can pin the terms they signed for.
    pub params_hash: T::Hash,
}

pub struct CurveParams<Balance> {
    pub graduation_target: Balance,   // T
    pub virtual_quote: Balance,       // V_q = T / 3
    pub curve_fee_bps: u16,
    pub protocol_share_bps: u16,
    pub pool_fee_tier: u32,
}

#[pallet::storage]
pub type AssetToLaunch<T: Config> = StorageMap<_, Blake2_128Concat, AssetIdOf<T>, LaunchId>;   // reverse index
```

`params_hash = blake2_256(encode((curve, S, SELLABLE, RESERVED, VT_FLOOR)))`.

### 1.3 Curve state (hot, per launch)

```rust
#[pallet::storage]
pub type Curves<T: Config> = StorageMap<_, Blake2_128Concat, LaunchId, CurveState<T>>;

pub struct CurveState<T: Config> {
    pub phase: Phase,                              // see §4
    /// VTRS held for the curve, excluding fees. Tracked, never read from the escrow balance (FM-04).
    pub real_quote: BalanceOf<T>,
    /// Sellable tokens still in escrow. Tracked. SELLABLE − tokens_remaining == tokens sold.
    pub tokens_remaining: BalanceOf<T>,
    /// Creator fee accrued in escrow and not yet claimed.
    pub creator_fees_unclaimed: BalanceOf<T>,
    /// Lifetime protocol fees forwarded to treasury (informational).
    pub protocol_fees_paid: BalanceOf<T>,
    /// Set when phase becomes Complete; drives RescueDelay (§4.4).
    pub completed_at: Option<BlockNumberFor<T>>,
    /// Set when phase becomes Graduated.
    pub graduated_at: Option<BlockNumberFor<T>>,
    /// LP shares the escrow received at seeding (informational; the position itself lives in the DEX).
    pub lp_shares: BalanceOf<T>,
}

pub enum Phase { Trading, Complete, Graduated }
```

There is no per-user storage. Token holdings are `pallet_assets` balances.

### 1.4 What snapshots vs what reads live

| Item | Snapshot (in `Launch.curve`) | Live |
|---|---|---|
| `T`, `V_q` | ✔ | |
| `curve_fee_bps`, `protocol_share_bps` | ✔ | |
| `pool_fee_tier` | ✔ | |
| `creation_fee` | | ✔ (charged once at create) |
| `CreationPaused` | | ✔ (create only) |
| `S`, `SELLABLE`, `RESERVED`, `VT_FLOOR` | runtime constants; hashed into `params_hash` | |
| Treasury account | | ✔ (`T::Treasury`) |
| Rescue delay | | ✔ (`T::RescueDelay` constant) |

### 1.5 Config

The pallet is bound on `pallet_vitreus_dex::Config` so that DEX errors can be matched by type (the rescue path maps the DEX's `SlippageExceeded` to `PriceOutOfTolerance`) and so `Balance` / `AssetKind` are the DEX's own types rather than redeclared. Two associated types are renamed to avoid clashing with the DEX Config's `Assets` (its native-or-asset union) and `ManageOrigin`.

```rust
pub type BalanceOf<T>   = <T as pallet_vitreus_dex::Config>::Balance;
pub type AssetKindOf<T> = <T as pallet_vitreus_dex::Config>::AssetKind;

pub trait Config: frame_system::Config + pallet_vitreus_dex::Config<Balance: From<u128> + Into<u128>> {
    type RuntimeEvent: ...;
    type LaunchManageOrigin: EnsureOrigin<Self::RuntimeOrigin>;
    type AssetId: Parameter + MaxEncodedLen + Copy + AtLeast32BitUnsigned;   // runtime: u128

    /// VTRS.
    type Currency: fungible::Inspect<Self::AccountId, Balance = BalanceOf<Self>> + fungible::Mutate<Self::AccountId>;
    /// pallet_assets main instance.
    type LaunchAssets: fungibles::Inspect<Self::AccountId, AssetId = Self::AssetId, Balance = BalanceOf<Self>>
                     + fungibles::Mutate<Self::AccountId>
                     + fungibles::Create<Self::AccountId>
                     + fungibles::metadata::Mutate<Self::AccountId>;

    type NativeAssetKind: Get<AssetKindOf<Self>>;
    type IntoAssetKind: Convert<Self::AssetId, AssetKindOf<Self>>;
    /// Trait-typed: the launchpad only calls the PoolManager / ReservedPoolSeeder surface.
    type Dex: PoolManager<Self::AccountId, AssetKindOf<Self>, BalanceOf<Self>, BlockNumberFor<Self>>
            + ReservedPoolSeeder<Self::AccountId, AssetKindOf<Self>, BalanceOf<Self>, BlockNumberFor<Self>>;

    type Treasury: Get<Self::AccountId>;
    type PalletId: Get<PalletId>;

    #[pallet::constant] type TotalSupply: Get<BalanceOf<Self>>;         // S
    #[pallet::constant] type Sellable: Get<BalanceOf<Self>>;            // SELLABLE
    #[pallet::constant] type VirtualTokenFloor: Get<BalanceOf<Self>>;   // VT_FLOOR
    #[pallet::constant] type LaunchAssetBase: Get<Self::AssetId>;       // 1 << 64
    #[pallet::constant] type MinGraduationTarget: Get<BalanceOf<Self>>;
    #[pallet::constant] type MaxGraduationTarget: Get<BalanceOf<Self>>;
    #[pallet::constant] type MaxCurveFeeBps: Get<u16>;                 // 500
    #[pallet::constant] type MinProtocolShareBps: Get<u16>;            // 5_000
    #[pallet::constant] type MinCreationFee: Get<BalanceOf<Self>>;
    #[pallet::constant] type RescueDelay: Get<BlockNumberFor<Self>>;   // 7 days = 100_800 blocks at 6 s
    #[pallet::constant] type StringLimit: Get<u32>;                    // ≤ pallet_assets StringLimit (50)
    #[pallet::constant] type DefaultLaunchParams: Get<LaunchParams<BalanceOf<Self>>>;

    /// Anti-snipe hook, v1 = (). See §2.7.
    type BuyHook: OnCurveBuy<Self::AccountId, BalanceOf<Self>, BlockNumberFor<Self>>;
}
```

`RESERVED` is derived: `TotalSupply − Sellable`; a `const_assert`-style check in `integrity_test` requires `Sellable < TotalSupply`, `VirtualTokenFloor > 0`, `LaunchAssetBase ≥ 2^64`.

---

## 2. Extrinsics

Common error set: `LaunchNotFound`, `WrongPhase`, `ZeroAmount`, `SlippageExceeded`, `ArithmeticOverflow`, `Unquotable`, `NotFeeRecipient`, `CreationPaused`, `AssetIdTaken`, `ParamsMismatch`, `ParamsOutOfBounds`, `PoolAlreadySeeded`, `RescueNotDue`, `PriceOutOfTolerance`, `InvalidMetadata`.

All extrinsics are `#[transactional]` (default in FRAME ≥ v0.9.x) except where a nested storage layer is stated explicitly (§2.2, §4.3).

### 2.1 `create_launch`

```rust
pub fn create_launch(
    origin: OriginFor<T>,                // Signed(creator)
    name: BoundedVec<u8, T::StringLimit>,
    symbol: BoundedVec<u8, T::StringLimit>,
    creator_fee_recipient: Option<T::AccountId>,   // None => creator
    initial_buy: BalanceOf<T>,           // VTRS, may be 0
    min_tokens_out: BalanceOf<T>,        // slippage for the initial buy; ignored when initial_buy == 0
    expected_params_hash: Option<T::Hash>, // FM-10 pin; None waives
) -> DispatchResult
```

Preconditions:
1. `!CreationPaused`.
2. `name`, `symbol` non-empty.
3. `id = NextLaunchId`; `asset_id = LaunchAssetBase + id`; `!T::Assets::asset_exists(asset_id)` else `AssetIdTaken` (FM-14).
4. `params = Params::get()`; compute `curve` and `params_hash`; if `expected_params_hash.is_some()` it must equal `params_hash` else `ParamsMismatch`.
5. Creator can pay `creation_fee + initial_buy` (checked by the transfers).
6. `!T::Dex::pool_exists(NativeAssetKind, IntoAssetKind(asset_id))` else `PoolAlreadySeeded` (FM-01 preflight; §4.4).
7. Seedability preflight (FM-11): `seed_would_succeed(T, RESERVED)` — with the DEX at ce34a05 this means `isqrt(T × RESERVED) > MINIMUM_LIQUIDITY (1000)` computed in U256, which is trivially true for `T ≥ MinGraduationTarget`. Kept as an explicit check so a future DEX minimum cannot strand a launch.

Effects (in order):
1. `Currency::transfer(creator → escrow, creation_fee, Preserve)`.
2. `Assets::create(asset_id, owner = escrow, is_sufficient = false, min_balance = 1)`.
3. `Assets::set(asset_id, from = escrow, name, symbol, decimals = 18)` — `pallet_assets` reserves `MetadataDepositBase + per_byte` from escrow.
4. `Assets::mint_into(asset_id, escrow, S)`.
5. Move `creation_fee − (ED + deposits actually reserved)` from escrow to `Treasury`, `Preserve` — escrow keeps exactly ED + reserved deposits.
6. Insert `Launches[id]`, `Curves[id] = { phase: Trading, real_quote: 0, tokens_remaining: SELLABLE, creator_fees_unclaimed: 0, ... }`, `AssetToLaunch[asset_id] = id`, `NextLaunchId = id + 1`.
7. Emit `LaunchCreated { id, asset_id, creator, params_hash }`.
8. If `initial_buy > 0`: `do_buy(creator, id, initial_buy, min_tokens_out)` (§2.2) in the same extrinsic. A first buy large enough to complete the curve is legal and graduates immediately.

Post-state: escrow VTRS balance `== ED + reserved_deposits + real_quote + creator_fees_unclaimed`; escrow token balance `== tokens_remaining + RESERVED`; `total_issuance(asset_id) == S`.

Notes: the pallet never calls `mint_into` again for this asset (I11). No admin of the asset exists outside the escrow account. Governance `pallet_assets::force_*` calls remain possible — that is equivalent to a runtime upgrade and is documented, not prevented.

### 2.2 `buy`

```rust
pub fn buy(
    origin: OriginFor<T>,          // Signed(who)
    launch_id: LaunchId,
    quote_in: BalanceOf<T>,        // exact VTRS input, gross of fee
    min_tokens_out: BalanceOf<T>,
) -> DispatchResult
```

Preconditions: launch exists; `phase == Trading`; `quote_in > 0`.

Effects (`do_buy`):
1. `(quote_in', hook_fee) = T::BuyHook::on_buy(launch, who, now, quote_in)` — v1 `()` returns `(quote_in, 0)`.
2. Compute `(tokens_out, quote_used, fee)` by §3.3 with partial-fill capping.
3. `ensure!(tokens_out ≥ min_tokens_out)`; `ensure!(tokens_out > 0)` (`Unquotable`).
4. Transfer VTRS: `who → escrow: quote_used` (`Preserve` on `who`). `quote_used = quote_net + fee` where `quote_net` is what the curve absorbs; on a partial fill `quote_used < quote_in` and the difference is simply never taken.
5. Split fee: `protocol_fee = fee × protocol_share_bps / BPS` (floor), `creator_fee = fee − protocol_fee`. `Currency::transfer(escrow → Treasury, protocol_fee, Preserve)`; `creator_fees_unclaimed += creator_fee`.
6. `real_quote += quote_net`; `tokens_remaining −= tokens_out`.
7. `Assets::transfer(asset_id, escrow → who, tokens_out, Preserve)`.
8. Emit `Bought { launch_id, who, quote_used, fee, tokens_out }`.
9. If `tokens_remaining == 0`: `phase = Complete; completed_at = now`; emit `CurveCompleted`; then attempt seeding in a **nested storage layer** (§4.3). Seeding failure does not fail the buy.

Post-state: I1–I6 hold; if the buy crossed, `phase ∈ {Complete, Graduated}`.

### 2.3 `sell`

```rust
pub fn sell(
    origin: OriginFor<T>,          // Signed(who)
    launch_id: LaunchId,
    tokens_in: BalanceOf<T>,
    min_quote_out: BalanceOf<T>,
) -> DispatchResult
```

Preconditions: launch exists; `phase == Trading` (sells revert in `Complete` — FM-08 — and in `Graduated`, where the pool is the venue); `tokens_in > 0`; `tokens_in ≤ SELLABLE − tokens_remaining` (cannot sell more than the curve has sold; `ArithmeticOverflow` otherwise, which can only happen if I3 is broken).

Effects:
1. Compute `(quote_gross, fee, quote_out)` by §3.4. `ensure!(quote_out > 0)` (`Unquotable`); `ensure!(quote_out ≥ min_quote_out)`.
2. `Assets::transfer(asset_id, who → escrow, tokens_in, Preserve)`.
3. Split fee as in buy: `protocol_fee` to Treasury now, `creator_fee` accrues.
4. `real_quote −= quote_gross` (checked); `tokens_remaining += tokens_in`.
5. `Currency::transfer(escrow → who, quote_out, Preserve)`.
6. Emit `Sold { launch_id, who, tokens_in, fee, quote_out }`.

Note the fee is taken out of the gross proceeds: `quote_out = quote_gross − fee`, and the escrow's VTRS decreases by `quote_gross` while `real_quote` decreases by `quote_gross` too — the fee portion stays in escrow as `creator_fee` plus the protocol transfer. I1 holds.

### 2.4 `graduate`

```rust
pub fn graduate(origin: OriginFor<T>, launch_id: LaunchId) -> DispatchResult   // Signed(anyone)
```

Permissionless retry of seeding (FM-08, FM-11). Preconditions: `phase == Complete`. Effect: `do_seed(launch_id)` (§4.3) **not** in a nested layer — if it fails the extrinsic fails and the error is visible to the caller. Success moves `phase → Graduated`.

### 2.5 `claim_creator_fees`

```rust
pub fn claim_creator_fees(origin: OriginFor<T>, launch_id: LaunchId) -> DispatchResult
```

Origin must be `launch.creator_fee_recipient`. Effect: `amount = creator_fees_unclaimed; creator_fees_unclaimed = 0; Currency::transfer(escrow → recipient, amount, Preserve)`. Allowed in every phase. Emits `CreatorFeesClaimed`.

### 2.6 `set_creator_fee_recipient`

```rust
pub fn set_creator_fee_recipient(origin: OriginFor<T>, launch_id: LaunchId, new: T::AccountId) -> DispatchResult
```

Origin must be the current `creator_fee_recipient`. No governance override in v1 (an override would need a timelock; pump.fun's `admin_set_creator` is the trust surface we choose not to have). Unclaimed fees stay in escrow and become claimable by `new`.

### 2.7 Anti-snipe hook (v1: no-op)

```rust
pub trait OnCurveBuy<AccountId, Balance, BlockNumber> {
    /// Called at the top of do_buy, before pricing. May reject (Err), may reduce the amount that
    /// reaches the curve, and may name an extra amount to route to the treasury. `launch_created_at`
    /// and `now` are block numbers — never timestamps.
    fn on_buy(
        launch_id: LaunchId,
        launch_created_at: BlockNumber,
        now: BlockNumber,
        who: &AccountId,
        is_creator: bool,
        quote_in: Balance,
    ) -> Result<(Balance /* to curve */, Balance /* extra to treasury */), DispatchError>;
}
impl<A, B: Zero, N> OnCurveBuy<A, B, N> for () { fn on_buy(.., q) -> Ok((q, B::zero())) }
```

Every entry path (`buy`, the initial buy in `create_launch`, any future precompile) goes through `do_buy` and therefore through the hook — this is the single-choke-point requirement from FM-05. `Launch.created_at` exists in v1 so a later hook has what it needs without a storage migration. Whatever v2 does here, the hook must be pure in `who`/`now`/`quote_in` and must not read escrow balances.

### 2.8 Governance

```rust
pub fn set_params(origin, new: LaunchParams<BalanceOf<T>>) -> DispatchResult    // ManageOrigin; bounds §1.1; emits ParamsUpdated
pub fn set_creation_paused(origin, paused: bool) -> DispatchResult             // ManageOrigin
pub fn force_seed_into_existing_pool(origin, launch_id, max_price_deviation_bps: u16) -> DispatchResult   // ManageOrigin; §4.4
```

There is deliberately **no** `force_withdraw`, `force_refund`, `force_cancel`, `force_set_phase` or `force_mint` (FM-03).

---

## 3. Curve math

### 3.1 State → virtual reserves

```
Q  = V_q + real_quote                       // virtual quote reserve, u128 (≤ 4·V_q + ε, fits)
Tk = VT_FLOOR + tokens_remaining            // virtual token reserve, u128 (≤ V_t ≈ 1.07·10^27, fits)
k  = Q × Tk                                 // U256 ONLY. Never in u128.
```

`k` is not stored; it is recomputed per trade from the tracked state. With `T ≤ 3·10^27`: `Q ≤ 1.2·10^28`, `Tk ≤ 1.07·10^27`, `k ≤ 1.3·10^55 < 2^184`. `U256` has 72 bits of headroom; `u128` overflows by 10^17×.

### 3.2 Where U256 is required, where u128 is safe

| Operation | Type | Why |
|---|---|---|
| `Q`, `Tk`, all reserves, balances, fees | `u128` (`checked_*`) | ≤ 10^28 |
| `q_in × fee_bps`, `q_gross × fee_bps` | `u128` | ≤ 3.4·10^38 / 10^4 ⇒ inputs must be `< 3.4·10^34`; enforce `quote_in ≤ MaxTradeIn = 10^34` (≈ 10^16 VTRS, never binding) and reject larger with `ArithmeticOverflow` |
| `k = Q × Tk` | `U256` | 10^55 |
| `k / (Q + q_net)`, `k / (Tk + t_in)` | `U256` division, result converted with `try_into::<u128>()` | result ≤ `Tk` resp. `Q`, always fits; a failed conversion is a bug ⇒ `ArithmeticOverflow` |
| `Q + q_net`, `Tk + t_in` | `u128` `checked_add` | |
| Seed preflight `isqrt(T × RESERVED)` | `U256` | 10^54 (the DEX side of this was D1, fixed in `ce34a05`) |

Use `sp_core::U256` (`primitive-types`), already a dependency of the DEX. Helper:

```rust
fn ceil_div_u256(n: U256, d: U256) -> U256 { (n + d - 1) / d }    // d > 0 guaranteed by callers
```

### 3.3 Buy (exact VTRS in)

Inputs: `q_in > 0`, state `(real_quote, tokens_remaining)`, params `(V_q, fee_bps)`.

```
1. fee      = ceil(q_in × fee_bps / BPS)                      // u128; rounds UP — trader pays more
2. q_net    = q_in − fee
3. Q, Tk, k as §3.1
4. Tk_new   = ceil_div_u256(k, U256(Q) + U256(q_net))          // rounds UP — fewer tokens leave the curve
   Tk_new   = min(Tk_new, Tk)                                  // guard: a tiny q_net can round Tk_new above Tk
5. t_out    = Tk − Tk_new                                      // u128; may be 0 ⇒ Unquotable

6. if t_out < tokens_remaining:                                // normal fill
       quote_net_used = q_net; quote_used = q_in
   else:                                                       // partial fill / crossing
       t_out          = tokens_remaining
       Q_end          = ceil_div_u256(k, U256(VT_FLOOR))       // rounds UP — trader pays more for the last tokens
       quote_net_used = Q_end − Q                              // u128
       fee            = ceil(quote_net_used × fee_bps / (BPS − fee_bps))   // fee on the used portion only, rounded UP
       quote_used     = quote_net_used + fee
       assert quote_used ≤ q_in                                // by construction (fill was capped because q_net was too large)

7. return (t_out, quote_used, quote_net_used, fee)
```

State update: `real_quote += quote_net_used; tokens_remaining −= t_out`.

Rounding summary — every rounding favours the pool: fee up; `Tk_new` up (tokens out down); on crossing, `Q_end` up (quote in up). Consequence: `k` after the trade ≥ `k` before (I4).

The fee-on-partial formula `fee = ceil(net × f / (BPS − f))` inverts `net = used − ceil(used × f / BPS)` to within one unit in the pool's favour; implementations must use exactly this form so the partial-fill fee equals what a normal fill of `quote_used` would have charged, ±1 unit rounded up.

### 3.4 Sell (exact tokens in)

Inputs: `t_in > 0`, `t_in ≤ SELLABLE − tokens_remaining`.

```
1. Q, Tk, k as §3.1
2. Q_new    = ceil_div_u256(k, U256(Tk) + U256(t_in))          // rounds UP — less VTRS leaves the curve
   Q_new    = max(Q_new, V_q)                                  // guard (never binding with correct state, see I5)
3. q_gross  = Q − Q_new                                        // u128; 0 ⇒ Unquotable
4. fee      = ceil(q_gross × fee_bps / BPS)                    // rounds UP
5. q_out    = q_gross − fee                                    // 0 ⇒ Unquotable
6. return (q_gross, fee, q_out)
```

State update: `real_quote −= q_gross` (checked_sub; failure ⇒ I1/I5 broken ⇒ `ArithmeticOverflow`); `tokens_remaining += t_in`.

### 3.5 Derived quantities (for events/RPC, not consensus-critical)

```
spot_price_q_per_token (18-dec fixed) = Q × 10^18 / Tk            // U256 mul then div, floor
raise_at_sellout       R  = ceil_div(V_q × V_t, VT_FLOOR) − V_q    // ≈ T·(1 − 4.2e-10)
progress_bps              = (SELLABLE − tokens_remaining) × BPS / SELLABLE
```

Expose `quote_buy(launch_id, q_in)` and `quote_sell(launch_id, t_in)` as runtime API calls that run §3.3/§3.4 without side effects; the frontend must never reimplement the math.

### 3.6 Quotability at the bounds

With a non-zero fee, a 1-unit buy is consumed entirely by the rounded-up fee (`fee = ceil(1 × f / BPS) = 1`, `q_net = 0`) and is `Unquotable` at any `T` — the pool-favouring answer, not free tokens. With `T = MinGraduationTarget = 3·10^18`, `V_q = 10^18`, `Tk = V_t = 1.067·10^27`, a 100-unit buy (fee 1, net 99) delivers `≈ 10^11` token units. A 1-unit sell of tokens returns `q_gross = 0` ⇒ `Unquotable`; that is correct (the tokens are worth less than 1 VTRS-wei) and rejecting it is the FM-09 rule "never 0 out for >0 in on the pool's side; never >0 out for 0 in on the trader's side".

---

## 4. Graduation state machine

### 4.1 States

```
Trading    — buys and sells against the curve
Complete   — tokens_remaining == 0; no buys, no sells; seeding pending or failed; permissionless retry
Graduated  — pool seeded and LP locked; terminal. Escrow holds only ED + deposits + unclaimed creator fees.
```

There is no `Cancelled`/`Expired`. A curve that never sells out trades forever, as on every live pad.

### 4.2 Transitions

| From | Event | To | Guard |
|---|---|---|---|
| Trading | `buy` with `t_out == tokens_remaining` (partial-fill branch, §3.3) | Complete, then immediately attempt → Graduated | inside the same extrinsic |
| Complete | `graduate` (anyone) | Graduated | `do_seed` succeeds |
| Complete | `graduate` | Complete | `do_seed` fails; extrinsic errors |
| Complete | `force_seed_into_existing_pool` (ManageOrigin) | Graduated | `now ≥ completed_at + RescueDelay` and price tolerance (§4.4) |
| any | `claim_creator_fees`, `set_creator_fee_recipient` | same | |

No transition out of Graduated. No transition from Complete back to Trading.

### 4.3 `do_seed(launch_id)` — the only path that moves curve funds to the pool

```
pre:  phase == Complete
      real_quote  == R  (all raised VTRS; checked ≥ 0, no other assumption)
      escrow token balance ≥ RESERVED
1. shares = T::Dex::seed_reserved_pool_for(           // DEX D2 (5ea3008), §5.2 — atomic in the DEX:
       who        = escrow,
       asset      = IntoAssetKind(asset_id),
       quote      = NativeAssetKind,
       amount_asset = RESERVED,
       amount_quote = real_quote,
       fee_tier   = curve.pool_fee_tier,
   )?
   // The DEX: creates the pool (or reuses an existing pool that has TotalLiquidity == 0), sweeps any pre-existing
   // balance in the pool account to Treasury, pulls exactly (RESERVED, real_quote) from escrow as the first deposit,
   // sets the escrow's LP position locked_until = BlockNumber::MAX, returns shares.
2. real_quote = 0; lp_shares = shares; phase = Graduated; graduated_at = now
3. emit Graduated { launch_id, pool: (asset, native), quote_seeded: R, tokens_seeded: RESERVED, shares }
post: escrow VTRS == ED + deposits + creator_fees_unclaimed   (donations excepted, I1 is ≥)
      escrow tokens == 0 (+ donations)
      Dex::pool_exists(asset, native) && position(escrow).locked_until == MAX && shares > 0
```

**Inside the crossing buy** (§2.2 step 9), `do_seed` runs as:

```rust
let res = frame_support::storage::with_storage_layer(|| Self::do_seed(launch_id));
match res {
    Ok(()) => {},
    Err(e) => Self::deposit_event(Event::GraduationDeferred { launch_id, error: e }),
}
```

so a seeding failure rolls back only the seeding, the buyer keeps their tokens, and the launch sits in `Complete` until `graduate` succeeds (FM-08: no frozen holder funds are at risk — the VTRS is in escrow, the tokens are with holders; only trading is paused, and only until anyone calls `graduate`). The crossing buy must be weighed for the seeding path (`WeightInfo::buy_crossing()`), and the weight refunded to `WeightInfo::buy()` when the branch is not taken.

Why not fail the buy instead: a revert would let a buyer probe for a DEX condition that makes seeding fail and grief every other buyer's crossing attempt; a deferred seed cannot be griefed because `graduate` is open to anyone.

### 4.4 When seeding cannot succeed (FM-11)

`seed_reserved_pool_for` can fail for exactly these reasons:

| Cause | Can it happen with the reserved-asset guard (§5.2) in place? | Recovery |
|---|---|---|
| Pool for `(asset, native)` exists **with** liquidity | Only via governance misuse of the DEX (root `create_pool` is rejected for reserved assets by D2, so it would require a runtime change) | `force_seed_into_existing_pool` after `RescueDelay`; deposits `(RESERVED, real_quote)` as a normal `add_liquidity_for` with `amount_min` set so the realised opening price is within `max_price_deviation_bps` of `real_quote / RESERVED`; locks; excess tokens/VTRS the DEX did not consume stay in escrow and are swept to Treasury. The DEX's `SlippageExceeded` is matched by type (hence the `pallet_vitreus_dex::Config` bound) and surfaced as `PriceOutOfTolerance`; the call can be retried with a different tolerance — never with a bypass. |
| Pool exists with zero liquidity | Yes (someone created it via a future DEX path) | `do_seed` handles it: D2 treats a zero-share pool as fresh after sweeping its account |
| DEX `InsufficientInitialLiquidity` | No — `create_launch` preflight (§2.1.7) uses the same formula | n/a; would indicate DEX constant changed under a live launch ⇒ governance can bump the DEX minimum only after `RescueDelay`-style review, out of scope |
| DEX arithmetic overflow | No since `ce34a05` (D1) | Regression guard: test `fm11_seed_overflow_is_impossible` |
| Escrow below ED after transfers | No — escrow keeps `ED + deposits` outside `real_quote` | n/a |

`RescueDelay` exists so that a "stuck" launch is observably stuck (many failed public `graduate` attempts) before governance can touch it, and so that the only governance action is still "seed the pool at the curve price", never "move funds elsewhere" (FM-03).

---

## 5. Invariants

Written as assertions over storage after every extrinsic. `try-runtime` hook `try_state` checks I1–I3, I5, I6, I11, I12 for every launch; the property tests in §6 check the rest.

```
I1  Currency::balance(escrow) ≥ ED + reserved_deposits + real_quote + creator_fees_unclaimed
    (equality when no one has donated VTRS to the escrow; donations are inert, FM-04)

I2  phase ∈ {Trading, Complete} ⇒ Assets::balance(asset, escrow) ≥ tokens_remaining + RESERVED
    phase == Graduated          ⇒ Assets::balance(asset, escrow) ≥ 0 and tokens_remaining == 0
    (≥ rather than == for the same donation reason)

I3  Σ_{a ≠ escrow, a ≠ pool_account} Assets::balance(asset, a) == SELLABLE − tokens_remaining   (pre-graduation)

I4  k' ≥ k across every buy and sell, where k = (V_q + real_quote) × (VT_FLOOR + tokens_remaining) in U256.
    Equivalently: the curve never pays out more than the constant-product price, in either direction.

I5  V_q + real_quote ≤ ceil((V_q × V_t) / VT_FLOOR)   and   0 ≤ tokens_remaining ≤ SELLABLE

I6  phase == Complete  ⇔  (tokens_remaining == 0 ∧ graduated_at == None)
    phase == Graduated ⇒  tokens_remaining == 0 ∧ real_quote == 0 ∧ lp_shares > 0

I7  phase == Graduated ⇒
        Dex::pool_exists(asset, native)
      ∧ Dex::position(escrow, pair).shares == lp_shares ∧ .locked_until == Some(BlockNumber::MAX)
      ∧ |pool_price_at_seed − p_end| / p_end ≤ 10^-6
        where p_end = (V_q + R) / VT_FLOOR and pool_price_at_seed = quote_seeded / tokens_seeded

I8  phase ∈ {Trading, Complete} ⇒ ¬ Dex::pool_exists(asset, native)   — unless the launch is in the
    governance-rescue case of §4.4, which is observable as completed_at + RescueDelay ≤ now.

I9  The set of code paths that decrease Currency::balance(escrow) is exactly
    { sell (quote_out + protocol_fee), buy (protocol_fee), claim_creator_fees, do_seed, create_launch step 5 }.
    The set that decreases Assets::balance(asset, escrow) is exactly { buy, do_seed }.
    (Enforced by review + the FM-03 test that greps the call graph; there is no storage encoding of this.)

I10 Launches[id] is write-once except creator_fee_recipient. Params changes never alter Launches[·].curve.

I11 Assets::total_issuance(asset) == S for every launch, forever. Assets::owner(asset) == escrow.

I12 NextLaunchId is strictly increasing; asset_id(id) == LaunchAssetBase + id; AssetToLaunch is a bijection
    onto Launches; no asset in [LaunchAssetBase, LaunchAssetBase + 2^64) exists that is not in AssetToLaunch.

I13 For every buy:  tokens_out × (Q_before + q_net) ≤ q_net × Tk_before   (trader never gets more than the constant-product price; ⇔ k does not decrease)
    For every sell: q_gross × (Tk_before + t_in) ≤ t_in × Q_before
```

### 5.1 The DEX-side invariant that collapses FM-01 and FM-02

> A pool whose pair contains an asset in the reserved range can be created only by the launchpad's seeding path, and its first deposit is exactly the amounts the launchpad has stored. No account other than the launchpad's escrow can ever hold LP shares from that first deposit, and that position is locked until `BlockNumber::MAX`.

This is enforced in `pallets/vitreus-dex`, not in the launchpad, because the launchpad cannot see or prevent what other callers do to the DEX. Changes beyond `f3350e4`. D1 (`ce34a05`), D2 and D3 (`5ea3008`) are landed and kept here for the record; only D4 remains open.

### 5.2 Required changes to `pallets/vitreus-dex`

**D1 — DONE in `ce34a05`: `u128` overflow at 18-decimal scale.**
Kept for the record; the description below is of the bug as it existed at `f3350e4`.
`do_add_liquidity_for` computes `amount_a.checked_mul(&amount_b)?.integer_sqrt()`; `do_swap` computes `reserve_out × amount_in_after_fee`; `remove_liquidity` computes `shares × reserve`; the optimal-amount branch computes `amount_a × reserve_b`. With the seed amounts of any launch (`real_quote ≈ 10^22`, `RESERVED = 2·10^26`) the product is `2·10^48 > u128::MAX ≈ 3.4·10^38`, so every one of these returns `Error::Overflow`. Even a 1-VTRS pool against 18-decimal tokens overflows (`10^18 × 2·10^26`). Every multiply-then-divide in the DEX must go through `U256` (`primitive-types` is already a dependency) and convert back with `try_into`. This affects all pools, not only launchpad ones; the existing energy pools presumably use small integers, which is why the tests pass. Fixed by routing every multiply-then-divide through `Config::HigherPrecisionBalance` (`sp_core::U256`) with floor rounding preserved at every site; covered by the six `scale_*` tests in `pallets/vitreus-dex/src/tests.rs`. Launchpad test `fm11_seed_overflow_is_impossible` stays as a regression guard.

**D2 — DONE in `5ea3008`: reserved-asset guard and atomic seeding.**
Implemented as specified below, with one deliberate deviation noted at the end. Runtime wiring: `LaunchpadReservedAssets` reserves `WithId(id)` for `id ∈ [2^64, 2^65)` (`LAUNCHPAD_ASSET_ID_START/END` in `runtime/vitreus/src/lib.rs`), `ExcessRecipient = xcm_config::TreasuryAccount`. `pallet_vitreus_dex::Pallet::pool_account_for(a, b)` is public so the launchpad can derive the sub-account before the pool exists.

```rust
// Config additions
type ReservedAssets: Contains<Self::AssetKind>;      // runtime: WithId(id) where id ∈ [2^64, 2^65)
type ExcessRecipient: Get<Self::AccountId>;          // runtime: Treasury

// do_create_pool: reject reserved assets for every caller (extrinsic and PoolManager)
ensure!(!T::ReservedAssets::contains(&asset_a) && !T::ReservedAssets::contains(&asset_b), Error::ReservedAsset);

// New trait, implemented for Pallet<T>, bound only by the launchpad's Config
pub trait ReservedPoolSeeder<AccountId, AssetKind, Balance, BlockNumber> {
    /// Create-or-adopt the pool for (asset, quote), sweep any pre-existing balance the pool account holds
    /// to ExcessRecipient, deposit exactly (amount_asset, amount_quote) from `who` as the first deposit,
    /// lock `who`'s position until BlockNumber::MAX, return shares. Fails with PoolAlreadySeeded if the pool
    /// exists with TotalLiquidity > 0. Whole body is transactional.
    fn seed_reserved_pool_for(who: &AccountId, asset: AssetKind, quote: AssetKind,
                              amount_asset: Balance, amount_quote: Balance, fee_tier: u32)
        -> Result<Balance, DispatchError>;
}
```

The sweep step is what closes FM-02 in *our* DEX: `do_swap` calls `sync_reserves`, which reads the pool account's live balances, so tokens or VTRS parked at the (predictable) pool address before seeding would otherwise be absorbed into the reserves on the first swap and move the opening price. Sweeping to Treasury before the first deposit makes the opening price exactly `amount_quote / amount_asset`. Post-seed donations still get absorbed by `sync_reserves`; that is Uniswap-v2 behaviour and is acceptable (the donor gives value to a locked pool).

Transfer order inside the seed must be **quote first, then asset**: the pool sub-account has no native balance until it receives VTRS, and `pallet_assets` will not open a non-sufficient asset account for an account with no provider. With `NativeOrWithId` encoding, `canonical_pair` already orders `Native` before `WithId`, so `do_add_liquidity_for`'s `pair.0` transfer is the VTRS one — but D2 must assert this rather than rely on it.

Who may call `seed_reserved_pool_for` is a runtime-wiring invariant: only `pallet_launchpad::Config::Dex` binds the trait, and `pallets/vitreus-dex` exposes no extrinsic that reaches it. Test `fm01_only_launchpad_can_seed_reserved_pool` asserts that `create_pool` (root) and `add_liquidity` (signed) both fail for a reserved asset before graduation.

*Deviation from the text above, decided during implementation:* if `ExcessRecipient` cannot receive a swept asset (e.g. an account with no provider cannot hold a non-sufficient asset), the seed **fails** with `Error::ExcessRecipientCannotReceive` instead of burning the balance. A silent burn would make a mis-wired recipient indistinguishable from correct operation; a loud failure is fixed by re-wiring and retried permissionlessly (FM-11). The transfer attempt runs in its own storage layer so nothing is half-applied. The seeder additionally requires `asset` to be reserved and `quote` not to be (`Error::NotReservedAsset`), so it cannot be used to create arbitrary permanently-locked pools around `ManageOrigin`. Sweep order is asset-then-quote so the non-sufficient asset's consumer reference is released before the native balance is swept to zero. DEX tests: `reserved_asset_rejected_by_create_pool_for_every_caller`, `seed_reserved_pool_creates_pool_deposits_stored_amounts_and_locks_forever`, `seed_sweeps_pre_seed_donations_so_opening_price_is_stored_ratio`, `seed_burns_pre_seed_donation_when_recipient_cannot_receive_it` (name predates the deviation; it asserts the failure and recovery), `seed_is_transactional_when_sweep_or_deposit_fails`, `seed_rejects_wrong_assets_double_seed_and_mismatched_adoption`, `canonical_pair_orders_native_before_with_id`.

**D3 — DONE in `5ea3008`: locks can only extend.** At `f3350e4` `do_lock_liquidity_for` set `locked_until` unconditionally, so a later call could shorten a lock. It is now monotone: an equal block is a no-op, an earlier block fails with `Error::LockCannotBeShortened` (rather than silently keeping the max, so a caller that expected to shorten finds out). Covered by `lock_can_be_extended_but_never_shortened` and the lock assertions in the seed test. Not exploitable for launchpad positions today (no signer for escrow, no other in-runtime caller), but the invariant "locked until MAX means forever" should not depend on that.

**D4 — OPEN, v2: per-pool fee routing.** At 5ea3008 100% of the swap fee stays in the pool's reserves. Because the launchpad's LP is locked forever, post-graduation fees accrue as pool depth, not as revenue to the treasury or creator. If the protocol is to *capture* ongoing trading fees (as opposed to compounding them), `PoolInfo` needs `protocol_fee_bps` / `creator_fee_bps` fields and `do_swap` must route those slices. The launchpad spec is written so that D4 can be added without touching launch state: `Launch.creator_fee_recipient` is the address the DEX would pay. Out of v1 scope.

**D5 — DONE in `c7d3752`: liquidity amounts and minimums must follow the canonical pair order.** At `5ea3008`, `do_add_liquidity_for` and `remove_liquidity` canonicalised the pair but left `amount_a`/`amount_b` and the minimums bound to the *caller's* argument positions, so `add_liquidity(USDC, VTRS, 1000, 1)` deposited 1000 on the VTRS side. User-facing, found by the launchpad rescue path. Fixed with `canonical_pair_with`, which reorders the values alongside the assets; `do_swap` already handled the flipped case and the seeder maps amounts explicitly. DEX tests: `d5_add_liquidity_non_canonical_order_maps_amounts_to_assets`, `d5_remove_liquidity_non_canonical_order_maps_minimums`, `d5_pool_manager_add_liquidity_for_non_canonical_order`.

**D6 — note, no change:** `MINIMUM_LIQUIDITY = 1000` shares are burned on first deposit. At seed scale `isqrt(10^22 × 2·10^26) ≈ 1.4·10^24` shares, so the burn is 10^-21 of the position; ignore.

### 5.3 Runtime wiring

- `pallet_assets` main instance: `CreateOrigin = EnsureNever` on mainnet, so `fungibles::Create::create` (which bypasses the origin check) is the *only* way this pallet's assets come into existence. On testnet `CreateOrigin = EnsureSigned`; §2.1.3 covers the squatting case (FM-14).
- `Assets::asset_exists` for ids ≥ `2^64` must be false at genesis. Add a `try_state` on the launchpad that scans nothing (we cannot iterate `pallet_assets` cheaply) but asserts `NextLaunchId`'s next id is free — I12.
- `dispatch_info_to_fee` in the runtime charges a constant VNRG fee for an explicit pallet list and a weight-based fee otherwise. Decide at integration whether `RuntimeCall::Launchpad(..)` joins the constant list; this spec assumes weight-based (default `_` arm).
- `OnCurveBuy` bound to `()`.
- `Treasury = pallet_treasury::TreasuryAccountId<Runtime>`.

---

## 6. Test plan

Unit tests live in `pallets/launchpad/src/tests.rs` against a mock that includes the real `pallet_vitreus_dex` (D1–D3 landed; D5 required for the rescue path), `pallet_assets` with signed creation (so FM-14 can squat an id), and utility/proxy/multisig for FM-05. The mock uses `AccountId32`: with `u64` ids every PalletId sub-account truncates to the same `"modlvtrs"` prefix and escrow, pool and DEX accounts collide. Property tests use an in-test xorshift generator over trade sequences (no `proptest` in the workspace). Test names are stable identifiers; a test that cannot be made to fail before the mitigation is added is not a test.

### 6.1 Failure-mode tests

| Test | Setup | Attack | Assertion that must fail the attack |
|---|---|---|---|
| `fm01_only_launchpad_can_seed_reserved_pool` | Launch L in Trading | (a) root calls `VitreusDex::create_pool(WithId(asset_L), Native, 3)`; (b) signed user calls `add_liquidity` for the pair | (a) `Error::ReservedAsset`; (b) `PoolNotFound`. After L graduates: `pool_exists`, and `add_liquidity` by users succeeds (post-graduation LP is allowed). |
| `fm01_seed_into_existing_liquid_pool_is_rejected` | Insert a `Pools`/`TotalLiquidity` record for asset_L directly into DEX storage (the guard makes this unreachable through any call) then add liquidity at 10× `p_end` | Crossing buy | Buy succeeds; `GraduationDeferred{PoolAlreadySeeded}` emitted; `phase == Complete`; escrow still holds `real_quote` and `RESERVED`; `graduate()` fails with the same error; `force_seed_into_existing_pool` before `RescueDelay` fails `RescueNotDue`; after delay with `max_price_deviation_bps = 100` fails `PriceOutOfTolerance`. |
| `fm02_prefunded_pool_account_does_not_move_opening_price` | Launch L; compute the DEX pool sub-account for `(Native, WithId(asset_L))` | Before crossing: transfer 10× `RESERVED` of tokens (bought on curve) and 5 VTRS directly to that account | After graduation the pool reserves equal exactly `(real_quote, RESERVED)`; the donated balances are in Treasury; `spot_price == quote_seeded / tokens_seeded`; first swap after seed does not change price by more than its own impact. |
| `fm03_no_path_moves_escrow_funds_except_curve_and_seed` | Graduated and Trading launches | Enumerate every dispatchable in the runtime (`RuntimeCall` variants) with root and with signed origins targeting escrow / pool accounts | For each call, escrow VTRS and token balances after == before, except `buy`, `sell`, `claim_creator_fees` (creator part only, ≤ `creator_fees_unclaimed`), `graduate`. `pallet_assets::force_transfer` from escrow is the documented exception and is asserted to be root-only. Second assertion: `RuntimeCall::Launchpad` has no variant named `force_withdraw|force_refund|force_cancel|force_set_phase|force_mint` (compile-time enum check). |
| `fm03_flash_style_complete_then_extract` | Launch at 50% progress | Actor buys to completion in one call, then attempts every call from the fm03 list in the same block | Actor's VTRS out ≤ VTRS in − fees; the only way to get VTRS back is `sell` (rejected, phase Complete) or the pool after seed at `p_end` with price impact. |
| `fm04_donation_to_escrow_is_inert` | Launch at 30% | Transfer 100 VTRS and 10M tokens (bought) directly to escrow | `quote_buy(1 VTRS)` identical before/after; `tokens_remaining`, `real_quote` unchanged; a later crossing seeds exactly `(real_quote, RESERVED)`; donated amounts remain in escrow after graduation (never enter the pool). |
| `fm05_all_entry_paths_hit_the_hook` | Bind `BuyHook` to a mock that records `(launch_id, who, now, quote_in)` and rejects `who == BLACKLISTED` | Call `buy` directly; via `utility.batch_all`; via `proxy.proxy`; via `multisig`; via `create_launch(initial_buy>0)` | Every path produces exactly one hook call per buy with identical arguments; the blacklisted account is rejected on every path; a batch of N buys yields N hook calls (no aggregation). |
| `fm06_no_creator_lp_and_position_is_permanent` | Graduated launch | (a) creator calls `remove_liquidity`; (b) escrow "signs" (impossible — assert no key derivation); (c) call `lock_liquidity_for(escrow, …, lock_until = now)` through the trait | (a) `InsufficientShares`; (c) `LockCannotBeShortened` and `locked_until` remains `MAX`; `LiquidityPositions` contains exactly one position for the pair, owned by escrow, `locked_until == Some(MAX)`; advancing to `BlockNumber::MAX − 1` still blocks removal. |
| `fm07_hook_receives_block_numbers_not_time` | Mock hook that rejects buys where `now == launch_created_at` | Buy in the creation block from a non-creator; buy in creation block via `create_launch(initial_buy)`; change `Timestamp` without advancing block | Non-creator rejected; creator initial buy passes (`is_creator == true`); timestamp change alone does not alter the outcome. (Behavioural hook test for the v2 mechanism's substrate; v1 `()` variant asserts pass-through.) |
| `fm08_crossing_buy_partial_fill_and_deferred_seed` | Launch with `tokens_remaining = 1M`; DEX mock forced to fail seeding once | Buy with `q_in` = 3× the VTRS needed to sell out | Buyer receives exactly `1M` tokens; charged exactly `quote_net_used + fee` per §3.3 (≤ `q_in`, remainder untouched); `phase == Complete`; `GraduationDeferred` emitted; `sell` now fails `WrongPhase`; `buy` fails `WrongPhase`; unforce the DEX; anyone calls `graduate` → `Graduated`, pool reserves `(real_quote, RESERVED)`. |
| `fm08_min_tokens_out_respected_on_partial_fill` | Same | Buy with `min_tokens_out = 2M` | Fails `SlippageExceeded`; state unchanged; a second buyer with `min_tokens_out = 0` completes the curve. |
| `fm09_rounding_always_favours_pool` (proptest) | Random `T` in bounds, random fee | Random sequence of 1–200 buys/sells with amounts drawn from `{1, 2, 3, 10^k, u128-near-max clipped to balance}` | After each trade: I4 (`k' ≥ k`, U256), I13, I5. Trades that would give 0 out revert with `Unquotable` and leave state unchanged. A full sell-back of all sold tokens leaves `real_quote ≥ 0` and `≤` the fees retained (never negative, never dips into ED). |
| `fm09_one_unit_edges` | `T = MinGraduationTarget` and `T = MaxGraduationTarget` | `buy(1)`, `sell(1)`, `buy(u128::MAX)` (clipped by balance), `buy(MaxTradeIn + 1)` | `buy(1)` → tokens > 0 at min T; `sell(1)` → `Unquotable`; `buy(MaxTradeIn+1)` → `ArithmeticOverflow`; no panic anywhere (`debug_assertions` on). |
| `fm09_no_u128_product_of_reserves` | Static | Grep the pallet source for `checked_mul` on any of `Q`, `Tk`, `real_quote`, `tokens_remaining`, `V_q` with each other | Zero matches; every reserve product is `U256::from(..) * U256::from(..)`. (Lint test; cheap insurance for the 10^50 issue.) |
| `fm10_params_change_does_not_touch_live_launch` | Launch L created at fee 1%, T = 30 VTRS | Governance sets fee 5%, T = 3000 VTRS, `protocol_share` 100% | L still charges 1%, graduates at ≈30 VTRS, splits 50/50; new launch M uses the new values; `create_launch(expected_params_hash = hash_before)` fails `ParamsMismatch`; with `hash_after` succeeds. |
| `fm10_params_bounds` | — | `set_params` with each field just outside its bound | Each fails `ParamsOutOfBounds`; each just inside succeeds. |
| `fm11_seed_overflow_is_impossible` | `T = MaxGraduationTarget`; real DEX | Cross the curve | Seeding succeeds (regression guard for D1: this failed with `Overflow` before `ce34a05`). |
| `fm11_every_complete_state_has_a_forward_path` | Enumerate seeding failure causes from §4.4 (mock each) | Cross the curve under each | For each: `phase == Complete` after the buy; either `graduate()` succeeds once the cause is removed, or `force_seed_into_existing_pool` succeeds after `RescueDelay` within tolerance; in no case does any balance leave escrow to anywhere but the pool or (excess) Treasury. |
| `fm11_create_preflight_rejects_unseedable` | `MINIMUM_LIQUIDITY` is a DEX crate constant, so the guard is exercised directly | `ensure_seedable(1000, 1000)` (isqrt == 1000), `(1001, 1001)`, `(MinGraduationTarget, RESERVED)` | The first fails `Unseedable`, the others pass; a `create_launch` with in-bounds params cannot trip it. |
| `fm12_wash_trading_is_net_negative` | Every `protocol_share_bps` in `[MinProtocolShareBps, 10_000]`, fee in `[1, MaxCurveFeeBps]` | Creator buys `x`, sells everything back, repeats 20 times, claims creator fees | `creator_fees_claimed < total_fees_paid_by_creator`; VTRS balance of creator strictly decreased. |
| `fm13_dump_model_exposes_concentration` | Graduated launch; one account holds 30% of `SELLABLE` | Sell 100% into the pool | Realised price impact reported by DEX event equals the constant-product prediction (`(R·0.3·SELLABLE)/(RESERVED + 0.3·SELLABLE)` net of fee) — there is no on-chain mitigation; this test pins the number so the frontend's concentration warning can be checked against it. |
| `fm14_asset_id_squatting` | Testnet config (`CreateOrigin = EnsureSigned`) | User creates asset `LaunchAssetBase + NextLaunchId` themselves, then `create_launch` | Fails `AssetIdTaken`; `NextLaunchId` unchanged; no partial state. Second case: user creates `LaunchAssetBase + NextLaunchId + 1` — launch `NextLaunchId` succeeds, the following one fails; governance can only skip by a migration that bumps `NextLaunchId` (assert no extrinsic does it). |
| `fm14_asset_id_range_never_reused` (proptest) | Random sequence of creates | — | I12 holds; `asset_id(id) == Base + id`; `AssetToLaunch` bijective. |
| `fm15_escrow_survives_full_sellback_and_claims` | Launch, buy, sell everything back, claim creator fees | — | Escrow account still exists; `Currency::balance(escrow) ≥ ED + reserved_deposits`; asset account of escrow intact; next buy works. Second case: creation fee set to exactly `MinCreationFee` — all steps still succeed. |
| `fm15_pool_account_gets_native_before_asset` | Not written here: the transfer order is inside the DEX's seeder and the launchpad cannot reverse it | — | Covered by the DEX's `seed_reserved_pool_creates_pool_deposits_stored_amounts_and_locks_forever`, which seeds into a pool account with no native balance (a token-first order would fail). |
| `fm16_name_symbol_not_enforced_on_chain` | Two launches with identical name/symbol | — | Both succeed (documents that dedupe is a frontend concern in v1). |

### 6.2 Invariant and lifecycle tests

- `inv_try_state_holds_after_random_sequences` (proptest): random interleaving of `create_launch`, `buy`, `sell`, `graduate`, `claim_creator_fees`, donations, `set_params`, across 3 launches; call `try_state` after every step.
- `lifecycle_happy_path`: create with initial buy → 10 buyers → 3 sellers → crossing → Graduated; assert I7 tolerance `≤ 10^-6`, `quote_seeded ∈ [T·(1−10^-9), T]`, `tokens_seeded == RESERVED`, treasury received `Σ protocol_fee + creation_fee − deposits`.
- `lifecycle_initial_buy_completes_curve`: `create_launch(initial_buy ≥ R + fee)` graduates in the create extrinsic.
- `weights_crossing_buy_refunds_when_not_crossing`: post-dispatch weight of a non-crossing `buy` `< WeightInfo::buy_crossing()`. **Not written in v1** — weights are constants like the DEX crate's; see §8.
- `dex_d1_u256_paths` (in the DEX crate): `add_liquidity`, `swap`, `remove_liquidity` with `(10^22, 2·10^26)` succeed and match a U256 reference computation.
- `dex_d3_lock_only_extends` (in the DEX crate).

### 6.3 Benchmarks

`create_launch` (with and without initial buy), `buy` (non-crossing), `buy_crossing` (includes seeding), `sell`, `graduate`, `claim_creator_fees`, `set_creator_fee_recipient`, `set_params`, `force_seed_into_existing_pool`. `buy_crossing` is the max-weight path and must include one `seed_reserved_pool_for` with the worst-case sweep (both pool-account balances non-zero).

---

## 7. Events and errors (reference)

Events: `LaunchCreated{id, asset_id, creator, params_hash}`, `Bought{launch_id, who, quote_used, fee, tokens_out}`, `Sold{launch_id, who, tokens_in, fee, quote_out}`, `CurveCompleted{launch_id, raised}`, `GraduationDeferred{launch_id, error}`, `Graduated{launch_id, quote_seeded, tokens_seeded, shares}`, `CreatorFeesClaimed{launch_id, recipient, amount}`, `CreatorFeeRecipientChanged{launch_id, old, new}`, `ParamsUpdated{..}`, `CreationPausedSet{paused}`, `ForceSeeded{launch_id, deviation_bps}`.

Errors: see §2 header; plus `ReservedAsset`, `PoolAlreadySeeded` surfaced from the DEX.

---

## 8. Open items for the implementer

### 8.1 Implemented — `b07441a` (`pallets/launchpad`), `491892a` (runtime wiring), `f0ad296` (benchmark scaffolding)

- **DEX prerequisites.** D1–D3 and D5 landed in `ce34a05`, `5ea3008`, `c7d3752`. D4 is v2.
- **Storage (§1), extrinsics (§2), curve math (§3), graduation state machine (§4).** All nine calls (`create_launch`, `buy`, `sell`, `graduate`, `claim_creator_fees`, `set_creator_fee_recipient`, `set_params`, `set_creation_paused`, `force_seed_into_existing_pool`) with `LaunchManageOrigin` governance; `OnCurveBuy` hook trait with `()` as the v1 no-op (§2.7).
- **Item 2 (metadata deposit), resolved.** `fm15_escrow_survives_full_sellback_and_claims` asserts the escrow's reserved balance equals `MetadataDepositBase + PerByte × (len(name) + len(symbol))` and that `fungibles::Create::create` reserves no `AssetDeposit`, matching §2.1.5's `MinCreationFee` derivation.
- **Item 3 (nested storage layer), resolved.** `do_buy` runs the deferred seed as `with_storage_layer(|| do_seed(..))` (lib.rs); a seed `Err` rolls back only the seed and leaves the crossing buy committed. Covered by `fm08_crossing_buy_partial_fill_and_deferred_seed` and the `arm_seed_failure` / `disarm_seed_failure` harness.
- **Test plan §6.1 and §6.2.** 35 tests (45 with `runtime-benchmarks`): `fm01`–`fm16` failure modes, `lifecycle_*`, and `inv_try_state_holds_after_random_sequences` (600-step random buy/sell/claim walk over three launches, checking every §5 invariant after each step). The §5 invariants live in the test-side `check_invariants` helper, **not** in a pallet `try_state` (see 8.2).
- **Weights and benchmarks (§6.3), `f0ad296` — scaffolding only; real weights not yet generated.** `WeightInfo` traits for both pallets replace the hard-coded constants; `weights.rs` carries the old constants as **placeholders** in the frame-weight-template layout so `benchmark pallet --output` overwrites them cleanly. v2 benchmarks exist for all nine launchpad calls and all 16 DEX calls (solver marketplace included) and pass in both mocks. `buy_crossing` is the max-weight path as §6.3 requires: no pool yet (creation, not adoption), both assets parked on the pool sub-account so `seed_reserved_pool_for` sweeps twice, then seed and permanent lock; `graduate` uses the same worst case on a `Complete` launch; `force_seed_into_existing_pool` seeds its pre-existing pool through the seeder directly. `create_launch(n, s)` takes name and symbol length components; nothing else in either pallet has a length- or count-dependent cost. Weight wiring for state-dependent cost: `buy` is charged `buy_crossing()` and refunded to `buy()` through `PostDispatchInfo` when the curve is not exhausted; `create_launch` adds `buy_crossing()` when an initial buy is present and refunds likewise; `do_buy` reports whether it crossed. `weights_crossing_buy_refunds_when_not_crossing` (§6.2) is written and passing. Both pallets are in the runtime's `define_benchmarks!` (the launchpad entry under `testnet-runtime` only, matching its wiring) and bind `SubstrateWeight<Runtime>`.
  - The runtime's `runtime-benchmarks` build had been broken since the stable2407 upgrade, independently of this pallet; `c2dc9b0` repairs it (`AssetId` is `u128` unconditionally — the old `cfg` switch to `u32` under benchmarks would have panicked on `LaunchAssetBase = 2^64` and made `LaunchpadReservedAssets` match nothing; plus `pallet_nfts` / `pallet_treasury` benchmark helpers, `pallet_xcm` config, the `Benchmark` runtime API, and feature forwarding). `cargo check` passes for `{testnet,mainnet}-runtime` with and without `runtime-benchmarks`, zero warnings.
- **Runtime wiring (§5.3), `491892a`.** `Launchpad: pallet_launchpad = 57` in `construct_runtime!`, `spec_version` 213 → **215** (214 is taken by upstream PR #99, VTRS as EVM native currency). The `Config` impl and the runtime entry are gated on `#[cfg(feature = "testnet-runtime")]`; mainnet keeps `pallet_assets::CreateOrigin = EnsureNever` and no launchpad. Constants: `PalletId = "vtrs/lpd"`, `TotalSupply = 1e9 UNITS`, `Sellable = 8e8 UNITS`, `VirtualTokenFloor = 266_666_667 UNITS`, `LaunchAssetBase = 2^64` (from the DEX-side `LAUNCHPAD_ASSET_ID_START`), graduation target `[3, 3e9] UNITS`, `MinCreationFee = 2·ED + MetadataDepositBase + 2·AssetsStringLimit·MetadataDepositPerByte` against the runtime's actual `100 / 50 / 2`, `RescueDelay = 7 DAYS`, `Treasury = TreasuryAccountId`, `BuyHook = ()`. `RuntimeCall::Launchpad(..)` takes the weight-based `dispatch_info_to_fee` arm, as assumed. The mainnet wasm built from `b5f7260` was checked with `subwasm metadata` (74 pallets, zero occurrences of `launchpad` in the decoded metadata) and a binary string scan (0 hits vs 44 for `VitreusDex`).
  - **Deviation from §5.3:** the `pallet-launchpad` crate dependency in `runtime/vitreus/Cargo.toml` is **unconditional**, not `optional`. `construct_runtime!` resolves the crate path even for a `#[cfg]`-gated entry (the same pattern `pallet-faucet` uses), so the crate compiles into the mainnet *build graph* but nothing from it is referenced, instantiated, or emitted into the mainnet wasm — see the verification above. Its `std` / `try-runtime` / `runtime-benchmarks` features are forwarded unconditionally for the same reason.
  - `scripts/devchain-launchpad.mjs` drives the pallet end to end on a dev chain (optional sudo upgrade, create, buy/sell, cross, verify the graduated pool).

### 8.2 Still open

1. **`try_state` (§5.3, I12).** Add the pallet hook asserting `NextLaunchId`'s next asset id is free in `pallet_assets`, and port `check_invariants` from `tests.rs` so the same checks run under `try-runtime`. Until then the invariants are only exercised in the mock.
2. **Runtime API.** `LaunchpadApi::{quote_buy, quote_sell, launch_state}` over the existing `Pallet::quote_buy` / `quote_sell` / `spot_price` / `curve_terms` helpers (§3.5). Nothing in `runtime/vitreus/runtime-api` yet.
3. **Real weights (§6.3) — scaffolding landed in `f0ad296`, numbers still to be generated.** Run `benchmark pallet` for `pallet_vitreus_dex` and `pallet_launchpad` on adequate hardware (the development box is 2 vCPU; its numbers would be meaningless) with `--template .maintain/frame-weight-template.hbs`, replacing the placeholder `weights.rs` in both crates. The launchpad must be benchmarked under `testnet-runtime` (it is not in the mainnet runtime); its weights transfer because the pallet code and `DbWeight` are identical. `pallet_assets` and every other pallet's weights should be regenerated at the same time: the ones checked in were produced under the old `AssetId = u32` switch and measured a narrower type than production.
   - **Unproven until `benchmark pallet` runs:** the `pallet_nfts` ECDSA `BenchmarkHelper` in `c2dc9b0` (keccak-hash then `ecdsa_sign_prehashed`, key recovered from the keystore by address) is derived from `EthereumSignature::verify` and type-checks, but the signature round-trip is only demonstrated by `benchmark pallet --pallet pallet_nfts --extrinsic mint_pre_signed` succeeding.
4. **EVM precompile.** Out of scope until PR #99 lands (VTRS as EVM native currency); when added it must call `do_buy` / `do_sell`, never re-implement pricing.
