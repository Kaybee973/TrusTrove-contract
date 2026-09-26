use soroban_sdk::{testutils::Address as _, Address, Env};

use crate::{DataKey, PoolFactoryContract, PoolFactoryContractClient};

#[test]
fn test_initialize() {
    let env = Env::default();
    env.mock_all_auths();
    let factory_id = env.register_contract(None, PoolFactoryContract);
    let factory_client = PoolFactoryContractClient::new(&env, &factory_id);

    let admin = Address::generate(&env);
    factory_client.initialize(&admin);

    // Verify admin is stored
    env.as_contract(&factory_id, || {
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap();
        assert_eq!(stored_admin, admin);
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_initialize_twice_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let factory_id = env.register_contract(None, PoolFactoryContract);
    let factory_client = PoolFactoryContractClient::new(&env, &factory_id);

    let admin = Address::generate(&env);
    factory_client.initialize(&admin);
    factory_client.initialize(&admin);
}

#[test]
fn test_register_existing_pool() {
    let env = Env::default();
    env.mock_all_auths();
    let factory_id = env.register_contract(None, PoolFactoryContract);
    let factory_client = PoolFactoryContractClient::new(&env, &factory_id);

    let admin = Address::generate(&env);
    factory_client.initialize(&admin);

    let asset = Address::generate(&env);
    let pool_address = Address::generate(&env);

    factory_client.register_existing_pool(&asset, &pool_address);

    // Verify registration
    env.as_contract(&factory_id, || {
        let stored_pool: Address = env
            .storage()
            .instance()
            .get(&DataKey::AssetToPool(asset.clone()))
            .unwrap();
        assert_eq!(stored_pool, pool_address);

        let count: u32 = env.storage().instance().get(&DataKey::AssetCount).unwrap();
        assert_eq!(count, 1);

        let indexed_asset: Address = env.storage().instance().get(&DataKey::AssetIndex(0)).unwrap();
        assert_eq!(indexed_asset, asset);
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_register_existing_pool_duplicate_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let factory_id = env.register_contract(None, PoolFactoryContract);
    let factory_client = PoolFactoryContractClient::new(&env, &factory_id);

    let admin = Address::generate(&env);
    factory_client.initialize(&admin);

    let asset = Address::generate(&env);
    let pool_address = Address::generate(&env);

    factory_client.register_existing_pool(&asset, &pool_address);
    factory_client.register_existing_pool(&asset, &pool_address);
}

#[test]
fn test_list_assets_empty() {
    let env = Env::default();
    let factory_id = env.register_contract(None, PoolFactoryContract);
    let factory_client = PoolFactoryContractClient::new(&env, &factory_id);

    let assets = factory_client.list_assets();
    assert_eq!(assets.len(), 0);
}

#[test]
fn test_gas_benchmark_register_existing_pool() {
    extern crate std;
    let env = Env::default();
    env.mock_all_auths();
    let factory_id = env.register_contract(None, PoolFactoryContract);
    let factory_client = PoolFactoryContractClient::new(&env, &factory_id);

    let admin = Address::generate(&env);
    factory_client.initialize(&admin);

    let asset = Address::generate(&env);
    let pool_address = Address::generate(&env);

    // Measure register_existing_pool resource cost
    env.budget().reset_default();
    let cpu_before = env.budget().cpu_instruction_cost();
    let mem_before = env.budget().memory_bytes_cost();

    factory_client.register_existing_pool(&asset, &pool_address);

    let cpu_after = env.budget().cpu_instruction_cost();
    let mem_after = env.budget().memory_bytes_cost();

    let cpu_delta = cpu_after - cpu_before;
    let mem_delta = mem_after - mem_before;

    // Log for manual inspection
    std::eprintln!("register_existing_pool CPU instructions: {}", cpu_delta);
    std::eprintln!("register_existing_pool Memory bytes: {}", mem_delta);
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
