use soroban_sdk::{contracttype, Address};

#[contracttype]
#[derive(Clone, Debug)]
pub enum DataKey {
    Admin,
    AssetCount,
    AssetIndex(u32),
    AssetToPool(Address),
}
