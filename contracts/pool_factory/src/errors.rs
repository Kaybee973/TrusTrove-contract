use soroban_sdk::contracterror;

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PoolFactoryError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    InvalidAmount = 3,
    AssetAlreadyRegistered = 4,
}
