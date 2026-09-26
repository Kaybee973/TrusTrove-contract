#![no_std]

use soroban_sdk::{contract, contractimpl, Address, Env, Vec};

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
}
