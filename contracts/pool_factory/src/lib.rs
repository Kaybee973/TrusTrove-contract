#![no_std]

use soroban_sdk::{contract, contractimpl, panic_with_error, Address, Env, Vec};

mod constants;
mod errors;
mod types;

#[cfg(test)]
mod test;

pub use constants::*;
pub use errors::*;
pub use types::*;

#[contract]
pub struct PoolFactoryContract;

#[contractimpl]
impl PoolFactoryContract {
    /// Initializes the pool factory with an admin address.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    /// * `admin` - The admin address for this contract.
    ///
    /// # Auth
    /// Requires authorization from `admin`.
    ///
    /// # Panics
    /// * `AlreadyInitialized` if the contract has already been initialized.
    ///
    /// # Returns
    /// * `()` - No value is returned.
    ///
    /// # Example
    /// ```ignore
    /// client.initialize(&admin);
    /// ```
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic_with_error!(&env, PoolFactoryError::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::AssetCount, &0u32);
    }

    /// Lists all registered assets.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    ///
    /// # Auth
    /// No authorization is required (read-only view).
    ///
    /// # Panics
    /// Does not panic.
    ///
    /// # Returns
    /// * `Vec<Address>` - A vector of all registered asset addresses.
    ///
    /// # Example
    /// ```ignore
    /// let assets = client.list_assets();
    /// ```
    pub fn list_assets(env: Env) -> Vec<Address> {
        let count: u32 = env
            .storage()
            .instance()
            .get(&DataKey::AssetCount)
            .unwrap_or(0);

        let mut assets = Vec::new(&env);
        for i in 0..count {
            if let Some(asset) = env.storage().instance().get(&DataKey::AssetIndex(i)) {
                assets.push_back(asset);
            }
        }
        assets
    }

    /// Registers an existing pool contract for a given asset.
    ///
    /// This function is used for migration scenarios where a pool contract
    /// already exists and needs to be registered under the factory without
    /// deploying a new instance.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    /// * `asset` - The asset address to register.
    /// * `pool_address` - The existing pool contract address.
    ///
    /// # Auth
    /// Requires authorization from the admin.
    ///
    /// # Panics
    /// * `NotInitialized` if the factory has not been initialized.
    /// * `AssetAlreadyRegistered` if the asset is already registered.
    ///
    /// # Returns
    /// * `()` - No value is returned.
    ///
    /// # Example
    /// ```ignore
    /// client.register_existing_pool(&usdc, &existing_pool);
    /// ```
    pub fn register_existing_pool(env: Env, asset: Address, pool_address: Address) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&env, PoolFactoryError::NotInitialized));
        admin.require_auth();

        // Check if asset is already registered
        if env
            .storage()
            .instance()
            .has(&DataKey::AssetToPool(asset.clone()))
        {
            panic_with_error!(&env, PoolFactoryError::AssetAlreadyRegistered);
        }

        // Get current count and increment
        let count: u32 = env
            .storage()
            .instance()
            .get(&DataKey::AssetCount)
            .unwrap_or(0);
        env.storage()
            .instance()
            .set(&DataKey::AssetCount, &(count + 1));

        // Add to index
        env.storage()
            .instance()
            .set(&DataKey::AssetIndex(count), &asset);

        // Map asset to pool
        env.storage()
            .instance()
            .set(&DataKey::AssetToPool(asset), &pool_address);
    }
}
