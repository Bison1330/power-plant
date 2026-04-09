//! Mock runtime for the `pallet-hiveiq-settlement` test suite.

use crate as pallet_hiveiq_settlement;
use frame_support::{
    derive_impl, parameter_types,
    traits::{ConstU64, EnsureOrigin},
    PalletId,
};
use sp_core::H256;
use sp_runtime::{traits::IdentityLookup, BuildStorage, Percent};

pub type Block = frame_system::mocking::MockBlock<Test>;
pub type AccountId = u64;
pub type Balance = u64;

frame_support::construct_runtime!(
    pub enum Test {
        System: frame_system,
        Balances: pallet_balances,
        HiveIQSettlement: pallet_hiveiq_settlement,
    }
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
    type Block = Block;
    type AccountId = AccountId;
    type Lookup = IdentityLookup<Self::AccountId>;
    type AccountData = pallet_balances::AccountData<Balance>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
    type AccountStore = System;
    type Balance = Balance;
    type ExistentialDeposit = ConstU64<1>;
}

parameter_types! {
    pub const HiveIQPalletId: PalletId = PalletId(*b"hiveiqsl");
    pub HiveIQSlashBurnPercent: Percent = Percent::from_percent(50);
}

/// Test origin gate that only accepts the [`ORACLE`] account, mirroring how a
/// production runtime would wire `OracleOrigin` to a known signer.
pub struct EnsureOracle;
impl EnsureOrigin<RuntimeOrigin> for EnsureOracle {
    type Success = AccountId;

    fn try_origin(o: RuntimeOrigin) -> Result<Self::Success, RuntimeOrigin> {
        let cloned = o.clone();
        match frame_system::ensure_signed(o) {
            Ok(who) if who == ORACLE => Ok(who),
            _ => Err(cloned),
        }
    }

    #[cfg(feature = "runtime-benchmarks")]
    fn try_successful_origin() -> Result<RuntimeOrigin, ()> {
        Ok(RuntimeOrigin::signed(ORACLE))
    }
}

impl pallet_hiveiq_settlement::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type OracleOrigin = EnsureOracle;
    type ManageOrigin = frame_system::EnsureRoot<AccountId>;
    type SlashBurnPercent = HiveIQSlashBurnPercent;
    type PalletId = HiveIQPalletId;
}

pub const ORACLE: AccountId = 1;
pub const PROVIDER: AccountId = 2;
pub const ALICE: AccountId = 3;
pub const ORACLE_ENDOWMENT: Balance = 1_000_000;

/// Build genesis state with the oracle pre-funded so it can pay into escrow.
pub fn new_test_ext() -> sp_io::TestExternalities {
    let mut t = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .unwrap();
    pallet_balances::GenesisConfig::<Test> {
        balances: vec![
            (ORACLE, ORACLE_ENDOWMENT),
            (PROVIDER, 1_000),
            (ALICE, 1_000),
        ],
    }
    .assimilate_storage(&mut t)
    .unwrap();
    let mut ext = sp_io::TestExternalities::new(t);
    ext.execute_with(|| System::set_block_number(1));
    ext
}

/// Convenience helper for tests that need a deterministic task ID.
pub fn task_id(n: u8) -> H256 {
    let mut bytes = [0u8; 32];
    bytes[31] = n;
    H256::from(bytes)
}
