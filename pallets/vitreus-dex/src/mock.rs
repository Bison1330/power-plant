//! Test environment for the Vitreus DEX pallet.

use super::*;
use crate as pallet_vitreus_dex;

use frame_support::{
    construct_runtime, derive_impl, parameter_types,
    traits::{AsEnsureOriginWithArg, ConstU128, ConstU32, ConstU64},
};
use frame_system::{EnsureRoot, EnsureSigned};
use sp_runtime::{traits::IdentityLookup, BuildStorage};

type Block = frame_system::mocking::MockBlock<Test>;

pub const ALICE: u128 = 1;
pub const BOB: u128 = 2;
pub const CHARLIE: u128 = 3;

pub const USDC_ID: u32 = 1;
pub const VNRG_ID: u32 = 2;

construct_runtime!(
    pub enum Test
    {
        System: frame_system,
        Balances: pallet_balances,
        Assets: pallet_assets,
        VitreusDex: pallet_vitreus_dex,
    }
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
    type AccountId = u128;
    type Lookup = IdentityLookup<Self::AccountId>;
    type Block = Block;
    type AccountData = pallet_balances::AccountData<u128>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
    type Balance = u128;
    type ExistentialDeposit = ConstU128<1>;
    type AccountStore = System;
}

impl pallet_assets::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type Balance = u128;
    type RemoveItemsLimit = ConstU32<1000>;
    type AssetId = u32;
    type AssetIdParameter = u32;
    type Currency = Balances;
    type CreateOrigin = AsEnsureOriginWithArg<EnsureSigned<Self::AccountId>>;
    type ForceOrigin = EnsureRoot<Self::AccountId>;
    type AssetDeposit = ConstU128<0>;
    type AssetAccountDeposit = ConstU128<0>;
    type MetadataDepositBase = ConstU128<0>;
    type MetadataDepositPerByte = ConstU128<0>;
    type ApprovalDeposit = ConstU128<0>;
    type StringLimit = ConstU32<50>;
    type Freezer = ();
    type Extra = ();
    type WeightInfo = ();
    type CallbackHandle = ();
    pallet_assets::runtime_benchmarks_enabled! {
        type BenchmarkHelper = ();
    }
}

pub type NativeOrAssetId = frame_support::traits::fungible::NativeOrWithId<u32>;

type NativeAndAssets = frame_support::traits::fungible::UnionOf<
    Balances,
    Assets,
    frame_support::traits::fungible::NativeFromLeft,
    NativeOrAssetId,
    u128,
>;

parameter_types! {
    pub const NativeAsset: NativeOrAssetId = NativeOrAssetId::Native;
    pub const USDC: u32 = USDC_ID;
    pub const VNRG: u32 = VNRG_ID;
    pub EnergyAsset: NativeOrAssetId = NativeOrAssetId::WithId(VNRG_ID);
}

impl Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type ManageOrigin = EnsureRoot<u128>;
    type Balance = u128;
    type AssetKind = NativeOrAssetId;
    type Assets = NativeAndAssets;
    type NativeAsset = NativeAsset;
    type EnergyAsset = EnergyAsset;
    type DefaultBidWindowBlocks = ConstU64<10>;
    type DefaultSettlementWindowBlocks = ConstU64<5>;
    type DefaultSolverBondAmount = ConstU128<1_000_000_000_000>;
}

pub(crate) fn new_test_ext() -> sp_io::TestExternalities {
    let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();

    pallet_balances::GenesisConfig::<Test> {
        // 10× the default solver bond (1e12) so tests can run register +
        // deregister + re-register flows without hitting ED or balance limits.
        balances: vec![
            (ALICE, 10_000_000_000_000),
            (BOB, 10_000_000_000_000),
            (CHARLIE, 10_000_000_000_000),
        ],
    }
    .assimilate_storage(&mut t)
    .unwrap();

    pallet_assets::GenesisConfig::<Test> {
        assets: vec![(USDC_ID, ALICE, true, 1), (VNRG_ID, ALICE, true, 1)],
        accounts: vec![
            (USDC_ID, ALICE, 1_000_000),
            (USDC_ID, BOB, 1_000_000),
            (USDC_ID, CHARLIE, 1_000_000),
            (VNRG_ID, ALICE, 1_000_000),
            (VNRG_ID, BOB, 1_000_000),
            (VNRG_ID, CHARLIE, 1_000_000),
        ],
        ..Default::default()
    }
    .assimilate_storage(&mut t)
    .unwrap();

    let mut ext = sp_io::TestExternalities::new(t);
    ext.execute_with(|| System::set_block_number(1));
    ext
}
