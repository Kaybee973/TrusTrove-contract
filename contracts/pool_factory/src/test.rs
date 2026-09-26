use soroban_sdk::{testutils::Address as _, Address, Env};

use crate::{DataKey, PoolFactoryContract, PoolFactoryContractClient};

#[test]
fn test_list_assets_empty() {
    let env = Env::default();
    let factory_id = env.register_contract(None, PoolFactoryContract);
    let factory_client = PoolFactoryContractClient::new(&env, &factory_id);

    let assets = factory_client.list_assets();
    assert_eq!(assets.len(), 0);
}

#[test]
fn test_list_assets_single() {
    let env = Env::default();
    let factory_id = env.register_contract(None, PoolFactoryContract);
    let factory_client = PoolFactoryContractClient::new(&env, &factory_id);

    // Simulate asset registration by setting the index directly
    let asset = Address::generate(&env);
    env.as_contract(&factory_id, || {
        env.storage().instance().set(&DataKey::AssetCount, &1u32);
        env.storage()
            .instance()
            .set(&DataKey::AssetIndex(0), &asset);
    });

    let assets = factory_client.list_assets();
    assert_eq!(assets.len(), 1);
    assert_eq!(assets.get(0), Some(asset));
}

#[test]
fn test_list_assets_multiple() {
    let env = Env::default();
    let factory_id = env.register_contract(None, PoolFactoryContract);
    let factory_client = PoolFactoryContractClient::new(&env, &factory_id);

    // Simulate multiple asset registrations
    let asset1 = Address::generate(&env);
    let asset2 = Address::generate(&env);
    let asset3 = Address::generate(&env);

    env.as_contract(&factory_id, || {
        env.storage().instance().set(&DataKey::AssetCount, &3u32);
        env.storage()
            .instance()
            .set(&DataKey::AssetIndex(0), &asset1);
        env.storage()
            .instance()
            .set(&DataKey::AssetIndex(1), &asset2);
        env.storage()
            .instance()
            .set(&DataKey::AssetIndex(2), &asset3);
    });

    let assets = factory_client.list_assets();
    assert_eq!(assets.len(), 3);
    assert_eq!(assets.get(0), Some(asset1));
    assert_eq!(assets.get(1), Some(asset2));
    assert_eq!(assets.get(2), Some(asset3));
}
