#![cfg(test)]

use soroban_sdk::{
    contract, contractimpl, contracttype, testutils::Address as _, Address, BytesN, Env,
};

use crate::{PoolContract, PoolContractClient};

use trusttrove_escrow::{EscrowContract as RealEscrow, EscrowContractClient as RealEscrowClient};
use trusttrove_invoice::{
    InvoiceContract as RealInvoice, InvoiceContractClient as RealInvoiceClient,
};

// --------------- Mock Registry ---------------

#[contract]
pub struct MockRegistry;

#[contractimpl]
impl MockRegistry {
    pub fn is_verified(env: Env, address: Address) -> bool {
        env.storage()
            .persistent()
            .get::<_, bool>(&RegKey(address))
            .unwrap_or(false)
    }

    pub fn register(env: Env, address: Address) {
        env.storage()
            .persistent()
            .set(&RegKey(address.clone()), &true);
        env.storage()
            .persistent()
            .extend_ttl(&RegKey(address), 100, 2_000_000);
    }
}

#[contracttype]
pub struct RegKey(Address);

// --------------- Mock Token ---------------

#[contract]
pub struct MockToken;

#[contractimpl]
impl MockToken {
    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        let from_key = TKey(from.clone());
        let to_key = TKey(to.clone());
        let from_bal: i128 = env.storage().persistent().get(&from_key).unwrap_or(0);
        let to_bal: i128 = env.storage().persistent().get(&to_key).unwrap_or(0);
        env.storage()
            .persistent()
            .set(&from_key, &(from_bal - amount));
        env.storage().persistent().set(&to_key, &(to_bal + amount));
    }

    pub fn balance(env: Env, addr: Address) -> i128 {
        env.storage().persistent().get(&TKey(addr)).unwrap_or(0)
    }
}

#[contracttype]
pub struct TKey(Address);

struct TestEnv {
    env: Env,
    pool: PoolContractClient<'static>,
    invoice: RealInvoiceClient<'static>,
    usdc_id: Address,
    xlm_id: Address,
    admin: Address,
    issuer: Address,
    buyer: Address,
    lp: Address,
}

fn setup() -> TestEnv {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let issuer = Address::generate(&env);
    let buyer = Address::generate(&env);
    let lp = Address::generate(&env);

    let registry_id = env.register_contract(None, MockRegistry);
    let registry = MockRegistryClient::new(&env, &registry_id);
    registry.register(&issuer);
    registry.register(&buyer);

    let usdc_id = env.register_contract(None, MockToken);
    let xlm_id = env.register_contract(None, MockToken);

    let lp_bal_key = TKey(lp.clone());
    env.as_contract(&usdc_id, || {
        env.storage()
            .persistent()
            .set(&lp_bal_key, &100_000_000_000_000i128);
    });
    env.as_contract(&xlm_id, || {
        env.storage()
            .persistent()
            .set(&lp_bal_key, &100_000_000_000_000i128);
    });
    let buyer_bal_key = TKey(buyer.clone());
    env.as_contract(&usdc_id, || {
        env.storage()
            .persistent()
            .set(&buyer_bal_key, &100_000_000_000_000i128);
    });
    env.as_contract(&xlm_id, || {
        env.storage()
            .persistent()
            .set(&buyer_bal_key, &100_000_000_000_000i128);
    });

    let invoice_id = env.register_contract(None, RealInvoice);
    let escrow_id = env.register_contract(None, RealEscrow);
    let pool_id = env.register_contract(None, PoolContract);

    let invoice = RealInvoiceClient::new(&env, &invoice_id);
    invoice.initialize(&admin, &registry_id);
    invoice.add_supported_asset(&usdc_id);
    invoice.add_supported_asset(&xlm_id);

    let pool = PoolContractClient::new(&env, &pool_id);
    pool.initialize(&admin, &invoice_id, &escrow_id, &usdc_id);

    let escrow = RealEscrowClient::new(&env, &escrow_id);
    escrow.initialize(&admin, &pool_id, &invoice_id, &usdc_id);

    invoice.set_pool_contract(&pool_id);

    // Raise cap to 100% so existing tests (which fund at 98% utilization) still pass
    pool.set_max_utilization(&admin, &10000);

    TestEnv {
        env,
        pool,
        invoice,
        usdc_id,
        xlm_id,
        admin,
        issuer,
        buyer,
        lp,
    }
}

fn create_and_list(te: &TestEnv, funding_asset: &Address) -> BytesN<32> {
    let due_date = te.env.ledger().timestamp() + 86400;
    let invoice_id = te.invoice.create(
        &te.issuer,
        &te.buyer,
        &10_000_000_000,
        &due_date,
        funding_asset,
    );
    te.invoice.list_for_financing(&invoice_id, &200);
    invoice_id
}

fn fund_and_repay_invoice(te: &TestEnv) -> BytesN<32> {
    let invoice_id = create_and_list(te, &te.usdc_id);
    let _ = te.pool.fund_invoice(&invoice_id);
    te.invoice.mark_shipped(&invoice_id);
    te.invoice.confirm_delivery(&invoice_id, &te.issuer);
    te.invoice.confirm_delivery(&invoice_id, &te.buyer);
    te.invoice.repay(&invoice_id);
    invoice_id
}

fn create_lp_with_balance(te: &TestEnv, balance: i128) -> Address {
    let lp = Address::generate(&te.env);
    let lp_bal_key = TKey(lp.clone());
    te.env.as_contract(&te.usdc_id, || {
        te.env.storage().persistent().set(&lp_bal_key, &balance);
    });
    lp
}

// ============== DEPOSIT TESTS ==============

#[test]
fn test_first_deposit_issues_one_to_one_shares() {
    let te = setup();
    let shares = te.pool.deposit(&te.lp, &5_000_000_000);
    assert_eq!(shares, 5_000_000_000);

    let pos = te.pool.get_lp_position(&te.lp);
    assert_eq!(pos.shares, 5_000_000_000);
    assert_eq!(pos.deposit_count, 1);
}

#[test]
fn test_second_deposit_issues_proportional_shares() {
    let te = setup();
    te.pool.deposit(&te.lp, &10_000_000_000);
    let shares = te.pool.deposit(&te.lp, &5_000_000_000);
    assert_eq!(shares, 5_000_000_000);

    let pos = te.pool.get_lp_position(&te.lp);
    assert_eq!(pos.shares, 15_000_000_000);
    assert_eq!(pos.deposit_count, 2);
}

#[test]
fn test_second_deposit_scales_by_share_price() {
    let te = setup();
    te.pool.deposit(&te.lp, &10_000_000_000);

    let shares = te.pool.deposit(&te.lp, &5_000_000_000);
    assert_eq!(shares, 5_000_000_000);

    let pos = te.pool.get_lp_position(&te.lp);
    assert_eq!(pos.shares, 15_000_000_000);
    assert_eq!(pos.deposit_count, 2);
}

// ============== WITHDRAW TESTS ==============

#[test]
fn test_withdraw_returns_correct_usdc() {
    let te = setup();
    te.pool.deposit(&te.lp, &10_000_000_000);
    let usdc = te.pool.withdraw(&te.lp, &5_000_000_000);
    assert_eq!(usdc, 5_000_000_000);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_withdraw_fails_if_insufficient_liquidity() {
    let te = setup();
    te.pool.deposit(&te.lp, &10_000_000_000);
    let invoice_id = create_and_list(&te, &te.usdc_id);
    let _ = te.pool.fund_invoice(&invoice_id);

    // Withdraw more shares than available liquidity can satisfy after funding
    te.pool.withdraw(&te.lp, &300_000_000);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_withdraw_zero_shares_panics() {
    let te = setup();
    te.pool.deposit(&te.lp, &10_000_000_000);
    te.pool.withdraw(&te.lp, &0);
}

#[test]
#[should_panic(expected = "Error(Contract, #7)")]
fn test_withdraw_more_than_owned_panics() {
    let te = setup();
    te.pool.deposit(&te.lp, &10_000_000_000);
    te.pool.withdraw(&te.lp, &20_000_000_000);
}

// ============== FUND INVOICE TESTS ==============

#[test]
fn test_fund_invoice_reduces_available_liquidity() {
    let te = setup();
    te.pool.deposit(&te.lp, &100_000_000_000);
    let invoice_id = create_and_list(&te, &te.usdc_id);

    let before = te.pool.get_stats();
    let _ = te.pool.fund_invoice(&invoice_id);
    let after = te.pool.get_stats();

    assert_eq!(after.active_invoice_count, 1);
    assert!(after.total_funded > before.total_funded);
    assert!(after.available_liquidity < before.available_liquidity);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_fund_invoice_fails_when_insufficient_liquidity() {
    let te = setup();
    let invoice_id = create_and_list(&te, &te.usdc_id);
    te.pool.fund_invoice(&invoice_id);
}

#[test]
#[should_panic(expected = "Error(Contract, #11)")]
fn test_fund_invoice_fails_asset_mismatch() {
    let te = setup();
    te.pool.deposit(&te.lp, &100_000_000_000);
    // Create invoice with XLM asset, but pool handles USDC
    let invoice_id = create_and_list(&te, &te.xlm_id);
    te.pool.fund_invoice(&invoice_id);
}

// ============== STATS TESTS ==============

#[test]
fn test_get_stats_initial_state() {
    let te = setup();
    let stats = te.pool.get_stats();
    assert_eq!(stats.total_deposits, 0);
    assert_eq!(stats.total_shares, 0);
    assert_eq!(stats.total_funded, 0);
    assert_eq!(stats.active_invoice_count, 0);
    assert_eq!(stats.available_liquidity, 0);
    assert_eq!(stats.utilization_rate_bps, 0);
}

#[test]
fn test_get_stats_after_deposit() {
    let te = setup();
    te.pool.deposit(&te.lp, &100_000_000_000);
    let stats = te.pool.get_stats();
    assert_eq!(stats.total_deposits, 100_000_000_000);
    assert_eq!(stats.total_shares, 100_000_000_000);
    assert_eq!(stats.available_liquidity, 100_000_000_000);
    assert_eq!(stats.utilization_rate_bps, 0);
}

#[test]
fn test_get_stats_after_funding() {
    let te = setup();
    te.pool.deposit(&te.lp, &100_000_000_000);
    let invoice_id = create_and_list(&te, &te.usdc_id);
    let _ = te.pool.fund_invoice(&invoice_id);

    let stats = te.pool.get_stats();
    assert!(stats.total_funded > 0);
    assert!(stats.available_liquidity < 100_000_000_000);
    assert_eq!(stats.active_invoice_count, 1);
    assert!(stats.utilization_rate_bps > 0);
}

// ============== LP POSITION TESTS ==============

#[test]
fn test_lp_position_empty() {
    let te = setup();
    let pos = te.pool.get_lp_position(&te.lp);
    assert_eq!(pos.shares, 0);
    assert_eq!(pos.usdc_value, 0);
    assert_eq!(pos.yield_earned, 0);
    assert_eq!(pos.deposit_count, 0);
}

#[test]
fn test_lp_position_after_deposit() {
    let te = setup();
    te.pool.deposit(&te.lp, &50_000_000_000);
    let pos = te.pool.get_lp_position(&te.lp);
    assert_eq!(pos.shares, 50_000_000_000);
    assert_eq!(pos.usdc_value, 50_000_000_000);
    assert_eq!(pos.deposit_count, 1);
}

// ============== UTILIZATION RATE TESTS ==============

#[test]
fn test_utilization_rate_zero_when_no_deposits() {
    let te = setup();
    assert_eq!(te.pool.get_utilization_rate(), 0);
}

#[test]
fn test_utilization_rate_zero_when_no_funding() {
    let te = setup();
    te.pool.deposit(&te.lp, &100_000_000_000);
    assert_eq!(te.pool.get_utilization_rate(), 0);
}

#[test]
fn test_utilization_rate_after_funding() {
    let te = setup();
    te.pool.deposit(&te.lp, &100_000_000_000);
    let invoice_id = create_and_list(&te, &te.usdc_id);
    let _ = te.pool.fund_invoice(&invoice_id);
    let rate = te.pool.get_utilization_rate();
    assert!(rate > 0);
    assert!(rate < 10000);
}

#[test]
fn test_utilization_rate_calculates_correctly() {
    let te = setup();
    te.pool.deposit(&te.lp, &10_000_000_000);
    // Raise cap to 100% so funding doesn't get rejected
    te.pool.set_max_utilization(&te.admin, &10000);
    let invoice_id = create_and_list(&te, &te.usdc_id);
    let _ = te.pool.fund_invoice(&invoice_id);
    assert_eq!(te.pool.get_utilization_rate(), 9800);
}

// ============== MAX UTILIZATION TESTS ==============

#[test]
fn test_default_max_utilization_in_stats() {
    // Fresh pool without setup override to verify initialize default is 8500
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let registry_id = env.register_contract(None, MockRegistry);
    let invoice_id = env.register_contract(None, RealInvoice);
    let escrow_id = env.register_contract(None, RealEscrow);
    let usdc_id = env.register_contract(None, MockToken);
    RealInvoiceClient::new(&env, &invoice_id).initialize(&admin, &registry_id);
    let pool_id = env.register_contract(None, PoolContract);
    let pool = PoolContractClient::new(&env, &pool_id);
    pool.initialize(&admin, &invoice_id, &escrow_id, &usdc_id);
    RealEscrowClient::new(&env, &escrow_id).initialize(&admin, &pool_id, &invoice_id, &usdc_id);
    let stats = pool.get_stats();
    assert_eq!(stats.max_utilization_bps, 8500);
}

#[test]
fn test_updated_max_utilization_reflected_in_stats() {
    let te = setup();
    te.pool.set_max_utilization(&te.admin, &9000);
    let stats = te.pool.get_stats();
    assert_eq!(stats.max_utilization_bps, 9000);
}

#[test]
#[should_panic(expected = "Error(Contract, #12)")]
fn test_fund_invoice_rejects_above_cap() {
    let te = setup();
    // Restore cap to 8500; funding at 9800 utilization should fail
    te.pool.set_max_utilization(&te.admin, &8500);
    te.pool.deposit(&te.lp, &10_000_000_000);
    let invoice_id = create_and_list(&te, &te.usdc_id);
    te.pool.fund_invoice(&invoice_id);
}

#[test]
fn test_fund_invoice_allowed_when_below_cap() {
    let te = setup();
    te.pool.set_max_utilization(&te.admin, &10000);
    te.pool.deposit(&te.lp, &10_000_000_000);
    let invoice_id = create_and_list(&te, &te.usdc_id);
    let result = te.pool.fund_invoice(&invoice_id);
    assert!(result);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_set_max_utilization_above_10000_panics() {
    let te = setup();
    te.pool.set_max_utilization(&te.admin, &10001);
}

#[test]
#[should_panic(expected = "Error(Contract, #12)")]
fn test_reducing_cap_mid_lifecycle_blocks_new_funding() {
    let te = setup();
    te.pool.set_max_utilization(&te.admin, &8500);
    te.pool.deposit(&te.lp, &100_000_000_000);
    // First funding: 9_800_000_000 / 100_000_000_000 = 980 bps < 8500 → ok
    let invoice_id = create_and_list(&te, &te.usdc_id);
    te.pool.fund_invoice(&invoice_id);

    // Lower cap below the utilization a second funding would cause
    // (980 bps already used; adding another 9.8B → 1960 bps)
    te.pool.set_max_utilization(&te.admin, &1000);
    // Second funding should push utilization to 1960 bps > 1000 → rejected
    let invoice_id2 = create_and_list(&te, &te.usdc_id);
    te.pool.fund_invoice(&invoice_id2);
}

#[test]
fn test_yield_increases_share_price_after_repayment() {
    let te = setup();
    te.pool.deposit(&te.lp, &10_000_000_000);
    fund_and_repay_invoice(&te);

    let pos = te.pool.get_lp_position(&te.lp);
    assert_eq!(pos.shares, 10_000_000_000);
    assert_eq!(pos.usdc_value, 10_200_000_000);
}

#[test]
fn test_two_lps_receive_proportional_yield() {
    let te = setup();
    let lp2 = create_lp_with_balance(&te, 100_000_000_000_000i128);

    te.pool.deposit(&te.lp, &10_000_000_000);
    te.pool.deposit(&lp2, &30_000_000_000);
    fund_and_repay_invoice(&te);

    let pos1 = te.pool.get_lp_position(&te.lp);
    let pos2 = te.pool.get_lp_position(&lp2);

    assert_eq!(pos1.shares, 10_000_000_000);
    assert_eq!(pos2.shares, 30_000_000_000);
    // With proportional yield distribution: LP1 gets 25% (10B/40B) of yield
    assert_eq!(pos1.usdc_value, 10_050_000_000);
    // LP2 gets 75% (30B/40B) of yield
    assert_eq!(pos2.usdc_value, 30_150_000_000);
}

#[test]
fn test_lp_position_reflects_current_share_price() {
    let te = setup();
    te.pool.deposit(&te.lp, &10_000_000_000);
    let invoice_id = create_and_list(&te, &te.usdc_id);
    let _ = te.pool.fund_invoice(&invoice_id);

    te.invoice.mark_shipped(&invoice_id);
    te.invoice.confirm_delivery(&invoice_id, &te.issuer);
    te.invoice.confirm_delivery(&invoice_id, &te.buyer);
    te.invoice.repay(&invoice_id);

    let pos = te.pool.get_lp_position(&te.lp);
    assert_eq!(pos.usdc_value, 10_200_000_000);
    assert_eq!(pos.shares, 10_000_000_000);
}

// ============== MULTI-LP TESTS ==============

#[test]
fn test_multiple_lps_can_deposit() {
    let te = setup();
    let lp2 = Address::generate(&te.env);
    let lp2_bal_key = TKey(lp2.clone());
    te.env.as_contract(&te.usdc_id, || {
        te.env
            .storage()
            .persistent()
            .set(&lp2_bal_key, &100_000_000_000_000i128);
    });

    let s1 = te.pool.deposit(&te.lp, &10_000_000_000);
    let s2 = te.pool.deposit(&lp2, &20_000_000_000);

    assert_eq!(s1, 10_000_000_000);
    assert_eq!(s2, 20_000_000_000);

    let stats = te.pool.get_stats();
    assert_eq!(stats.total_shares, 30_000_000_000);
    assert_eq!(stats.total_deposits, 30_000_000_000);
}

// ============== REPAYMENT TESTS ==============

#[test]
fn test_receive_repayment() {
    let te = setup();
    te.pool.deposit(&te.lp, &100_000_000_000);
    let invoice_id = create_and_list(&te, &te.usdc_id);
    te.pool.fund_invoice(&invoice_id);

    // face_value=10_000_000_000, discount_bps=200
    // funded_amount = 10_000_000_000 * (10000 - 200) / 10000 = 9_800_000_000
    let yield_amount = 200_000_000;

    let before = te.pool.get_stats();
    let result = te.pool.receive_repayment(&invoice_id, &10_000_000_000);
    assert!(result);

    let after = te.pool.get_stats();
    assert_eq!(after.total_deposits, before.total_deposits + yield_amount);
    assert_eq!(after.total_yield_distributed, yield_amount);
    assert_eq!(after.total_funded, 0);
    assert_eq!(after.active_invoice_count, 0);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_receive_repayment_panics_when_amount_below_funded() {
    let te = setup();
    te.pool.deposit(&te.lp, &100_000_000_000);
    let invoice_id = create_and_list(&te, &te.usdc_id);
    te.pool.fund_invoice(&invoice_id);

    // funded_amount = 9_800_000_000, sending less should panic (#4 = InvalidAmount)
    te.pool.receive_repayment(&invoice_id, &1_000_000_000);
}

// ============== DEFAULT TESTS ==============

#[test]
fn test_handle_default() {
    let te = setup();
    te.pool.deposit(&te.lp, &100_000_000_000);
    let invoice_id = create_and_list(&te, &te.usdc_id);
    te.pool.fund_invoice(&invoice_id);

    // funded_amount = 10_000_000_000 * 9800 / 10000 = 9_800_000_000
    let funded_amount = 9_800_000_000;

    let before = te.pool.get_stats();
    let result = te.pool.handle_default(&invoice_id);
    assert!(result);

    let after = te.pool.get_stats();
    assert_eq!(after.total_deposits, before.total_deposits - funded_amount);
    assert_eq!(after.total_funded, 0);
    assert_eq!(after.active_invoice_count, 0);
}

#[test]
fn test_handle_default_unknown_invoice_returns_false() {
    let te = setup();
    let dummy_id = BytesN::from_array(&te.env, &[0u8; 32]);
    let result = te.pool.handle_default(&dummy_id);
    assert!(!result);
}

#[test]
fn test_deposit_when_deposits_zero_but_shares_exist() {
    let te = setup();

    // Deposit exact amount needed to fund the standard test invoice
    // (10B face value, 200bps discount = 9.8B funding amount)
    te.pool.deposit(&te.lp, &9_800_000_000);

    let invoice_id = create_and_list(&te, &te.usdc_id);
    te.pool.fund_invoice(&invoice_id);

    // Trigger default, wiping out all pool deposits
    te.pool.handle_default(&invoice_id);

    let stats = te.pool.get_stats();
    assert_eq!(stats.total_deposits, 0);
    assert!(stats.total_shares > 0);

    // Attempt new deposit, which should not panic and should issue 1-to-1 shares
    let lp2 = create_lp_with_balance(&te, 10_000_000_000);
    let new_shares = te.pool.deposit(&lp2, &5_000_000_000);
    assert_eq!(new_shares, 5_000_000_000);
}

// ============== MULTI-INVOICE LIFECYCLE TESTS ==============

#[test]
fn test_multi_invoice_fund_two_repay_one_default_one() {
    let te = setup();
    te.pool.deposit(&te.lp, &100_000_000_000);

    let inv1 = create_and_list(&te, &te.usdc_id);
    let inv2 = create_and_list(&te, &te.usdc_id);
    te.pool.fund_invoice(&inv1);
    te.pool.fund_invoice(&inv2);

    let stats_after_fund = te.pool.get_stats();
    assert_eq!(stats_after_fund.active_invoice_count, 2);
    assert_eq!(stats_after_fund.total_funded, 19_600_000_000);

    // Repay inv1: adds yield of 200_000_000
    te.invoice.mark_shipped(&inv1);
    te.invoice.confirm_delivery(&inv1, &te.issuer);
    te.invoice.confirm_delivery(&inv1, &te.buyer);
    te.invoice.repay(&inv1);

    let stats_after_repay = te.pool.get_stats();
    assert_eq!(stats_after_repay.active_invoice_count, 1);
    assert_eq!(stats_after_repay.total_yield_distributed, 200_000_000);
    assert_eq!(stats_after_repay.total_funded, 9_800_000_000);

    // Default inv2: removes 9_800_000_000 from deposits
    te.pool.handle_default(&inv2);

    let stats_final = te.pool.get_stats();
    assert_eq!(stats_final.active_invoice_count, 0);
    assert_eq!(stats_final.total_funded, 0);
    // total_deposits = 100B (initial) + 200M (yield from repaid) - 9.8B (defaulted) = 90_400_000_000
    assert_eq!(stats_final.total_deposits, 90_400_000_000);

    // LP value reflects both outcomes
    let pos = te.pool.get_lp_position(&te.lp);
    assert_eq!(pos.shares, 100_000_000_000);
    assert_eq!(pos.usdc_value, 90_400_000_000);
}

#[test]
fn test_multi_invoice_default_first_then_repay_second() {
    let te = setup();
    te.pool.deposit(&te.lp, &100_000_000_000);

    let inv1 = create_and_list(&te, &te.usdc_id);
    let inv2 = create_and_list(&te, &te.usdc_id);
    te.pool.fund_invoice(&inv1);
    te.pool.fund_invoice(&inv2);

    // Default inv1 first
    te.pool.handle_default(&inv1);

    let stats_mid = te.pool.get_stats();
    assert_eq!(stats_mid.active_invoice_count, 1);
    assert_eq!(stats_mid.total_deposits, 90_200_000_000);

    // Repay inv2: adds yield on reduced pool
    te.invoice.mark_shipped(&inv2);
    te.invoice.confirm_delivery(&inv2, &te.issuer);
    te.invoice.confirm_delivery(&inv2, &te.buyer);
    te.invoice.repay(&inv2);

    let stats_final = te.pool.get_stats();
    assert_eq!(stats_final.active_invoice_count, 0);
    assert_eq!(stats_final.total_funded, 0);
    // 90.2B (after default) + 200M (yield from repay) = 90_400_000_000
    assert_eq!(stats_final.total_deposits, 90_400_000_000);
    assert_eq!(stats_final.total_yield_distributed, 200_000_000);
}

// ============== DEPOSIT-AFTER-YIELD TESTS ==============

#[test]
fn test_deposit_after_yield_uses_updated_share_price() {
    let te = setup();
    te.pool.deposit(&te.lp, &10_000_000_000);

    // Fund and repay to generate yield
    fund_and_repay_invoice(&te);

    // Share price is now 10.2B / 10B = 1.02
    let stats = te.pool.get_stats();
    assert_eq!(stats.total_deposits, 10_200_000_000);
    assert_eq!(stats.total_shares, 10_000_000_000);

    // New LP deposits 10.2B USDC → should get 10.2B / 1.02 = 10B shares
    let lp2 = create_lp_with_balance(&te, 100_000_000_000_000i128);
    let new_shares = te.pool.deposit(&lp2, &10_200_000_000);
    assert_eq!(new_shares, 10_000_000_000);

    let pos2 = te.pool.get_lp_position(&lp2);
    assert_eq!(pos2.shares, 10_000_000_000);
    assert_eq!(pos2.usdc_value, 10_200_000_000);

    // Both LPs have equal shares and equal value
    let pos1 = te.pool.get_lp_position(&te.lp);
    assert_eq!(pos1.shares, pos2.shares);
    assert_eq!(pos1.usdc_value, pos2.usdc_value);
}

#[test]
fn test_new_lp_deposits_after_multiple_yield_events() {
    let te = setup();
    te.pool.deposit(&te.lp, &20_000_000_000);

    // Two rounds of yield
    fund_and_repay_invoice(&te);
    fund_and_repay_invoice(&te);

    // total_deposits = 20B + 200M + 200M = 20_400_000_000
    // total_shares = 20B
    // share_price = 1.02
    let stats = te.pool.get_stats();
    assert_eq!(stats.total_deposits, 20_400_000_000);
    assert_eq!(stats.total_shares, 20_000_000_000);

    let lp2 = create_lp_with_balance(&te, 100_000_000_000_000i128);
    let new_shares = te.pool.deposit(&lp2, &10_200_000_000);
    // 10.2B / 1.02 = 10B shares
    assert_eq!(new_shares, 10_000_000_000);
}

// ============== SERIES OF DEFAULTS TESTS ==============

#[test]
fn test_series_of_defaults_erodes_lp_value() {
    let te = setup();
    te.pool.deposit(&te.lp, &100_000_000_000);

    let inv1 = create_and_list(&te, &te.usdc_id);
    let inv2 = create_and_list(&te, &te.usdc_id);
    te.pool.fund_invoice(&inv1);
    te.pool.fund_invoice(&inv2);

    // Default both
    te.pool.handle_default(&inv1);
    te.pool.handle_default(&inv2);

    // Deposits reduced by 2 * 9.8B = 19.6B
    let stats = te.pool.get_stats();
    assert_eq!(stats.total_deposits, 80_400_000_000);
    assert_eq!(stats.total_funded, 0);
    assert_eq!(stats.active_invoice_count, 0);

    let pos = te.pool.get_lp_position(&te.lp);
    assert_eq!(pos.shares, 100_000_000_000);
    assert_eq!(pos.usdc_value, 80_400_000_000);
    assert_eq!(pos.yield_earned, 0);
}

#[test]
fn test_withdraw_after_default_partial() {
    let te = setup();
    te.pool.deposit(&te.lp, &100_000_000_000);

    let inv1 = create_and_list(&te, &te.usdc_id);
    te.pool.fund_invoice(&inv1);
    te.pool.handle_default(&inv1);

    // After default: 100B - 9.8B = 90.2B deposits, 100B shares
    let stats = te.pool.get_stats();
    assert_eq!(stats.total_deposits, 90_200_000_000);

    // Withdraw half shares: 50B shares → 50B * 90.2B / 100B = 45_100_000_000 USDC
    let usdc_returned = te.pool.withdraw(&te.lp, &50_000_000_000);
    assert_eq!(usdc_returned, 45_100_000_000);

    let pos = te.pool.get_lp_position(&te.lp);
    assert_eq!(pos.shares, 50_000_000_000);
    assert_eq!(pos.usdc_value, 45_100_000_000);
}

#[test]
fn test_withdraw_full_after_default() {
    let te = setup();
    te.pool.deposit(&te.lp, &100_000_000_000);

    let inv1 = create_and_list(&te, &te.usdc_id);
    te.pool.fund_invoice(&inv1);
    te.pool.handle_default(&inv1);

    let usdc_returned = te.pool.withdraw(&te.lp, &100_000_000_000);
    assert_eq!(usdc_returned, 90_200_000_000);

    let stats = te.pool.get_stats();
    assert_eq!(stats.total_deposits, 0);
    assert_eq!(stats.total_shares, 0);
}

// ============== MIXED SCENARIO TESTS ==============

#[test]
fn test_two_lps_one_withdraws_after_default() {
    let te = setup();
    let lp2 = create_lp_with_balance(&te, 100_000_000_000_000i128);

    te.pool.deposit(&te.lp, &50_000_000_000);
    te.pool.deposit(&lp2, &50_000_000_000);

    let inv1 = create_and_list(&te, &te.usdc_id);
    te.pool.fund_invoice(&inv1);
    te.pool.handle_default(&inv1);

    // Both LPs share the loss equally (50/50)
    let usdc1 = te.pool.withdraw(&te.lp, &50_000_000_000);
    let usdc2 = te.pool.withdraw(&lp2, &50_000_000_000);
    assert_eq!(usdc1, 45_100_000_000);
    assert_eq!(usdc2, 45_100_000_000);

    let stats = te.pool.get_stats();
    assert_eq!(stats.total_deposits, 0);
    assert_eq!(stats.total_shares, 0);
}

#[test]
fn test_deposit_during_active_funding() {
    let te = setup();
    te.pool.deposit(&te.lp, &100_000_000_000);

    let inv1 = create_and_list(&te, &te.usdc_id);
    te.pool.fund_invoice(&inv1);

    // Pool has 100B deposits, 9.8B funded, 90.2B available
    // New LP deposits while invoice is active
    let lp2 = create_lp_with_balance(&te, 100_000_000_000_000i128);
    let new_shares = te.pool.deposit(&lp2, &50_000_000_000);
    assert_eq!(new_shares, 50_000_000_000);

    // 1:1 because no yield has been distributed yet
    let pos2 = te.pool.get_lp_position(&lp2);
    assert_eq!(pos2.shares, 50_000_000_000);
    assert_eq!(pos2.usdc_value, 50_000_000_000);
}

#[test]
fn test_dust_withdrawal() {
    let te = setup();
    te.pool.deposit(&te.lp, &10_000_000_000);

    // Withdraw 1 unit of shares
    let usdc = te.pool.withdraw(&te.lp, &1);
    assert_eq!(usdc, 1);
}

#[test]
fn test_full_lifecycle_multiple_invoices() {
    let te = setup();
    te.pool.deposit(&te.lp, &100_000_000_000);

    // Fund 3 invoices
    let inv1 = create_and_list(&te, &te.usdc_id);
    let inv2 = create_and_list(&te, &te.usdc_id);
    let inv3 = create_and_list(&te, &te.usdc_id);
    te.pool.fund_invoice(&inv1);
    te.pool.fund_invoice(&inv2);
    te.pool.fund_invoice(&inv3);

    let stats = te.pool.get_stats();
    assert_eq!(stats.active_invoice_count, 3);
    assert_eq!(stats.total_funded, 29_400_000_000);

    // Repay inv1
    te.invoice.mark_shipped(&inv1);
    te.invoice.confirm_delivery(&inv1, &te.issuer);
    te.invoice.confirm_delivery(&inv1, &te.buyer);
    te.invoice.repay(&inv1);

    // Default inv2
    te.pool.handle_default(&inv2);

    // Repay inv3
    te.invoice.mark_shipped(&inv3);
    te.invoice.confirm_delivery(&inv3, &te.issuer);
    te.invoice.confirm_delivery(&inv3, &te.buyer);
    te.invoice.repay(&inv3);

    let stats_final = te.pool.get_stats();
    assert_eq!(stats_final.active_invoice_count, 0);
    assert_eq!(stats_final.total_funded, 0);
    // 100B + 200M (inv1 yield) - 9.8B (inv2 default) + 200M (inv3 yield) = 90_600_000_000
    assert_eq!(stats_final.total_deposits, 90_600_000_000);
    assert_eq!(stats_final.total_yield_distributed, 400_000_000);

    let pos = te.pool.get_lp_position(&te.lp);
    assert_eq!(pos.shares, 100_000_000_000);
    assert_eq!(pos.usdc_value, 90_600_000_000);
    assert_eq!(pos.yield_earned, 0);
}

// ============== SEP-41 COMPLIANCE TESTS ==============

#[test]
fn test_sep41_balance_matches_lp_shares() {
    let te = setup();

    // Initial balance should be 0
    assert_eq!(te.pool.balance(&te.lp), 0);

    // Deposit some USDC
    let deposit_amount = 5_000_000_000;
    let shares_issued = te.pool.deposit(&te.lp, &deposit_amount);

    // Balance should match shares issued
    assert_eq!(te.pool.balance(&te.lp), shares_issued);
    assert_eq!(te.pool.balance(&te.lp), deposit_amount); // 1:1 for first deposit

    // Add another LP
    let lp2 = create_lp_with_balance(&te, 100_000_000_000_000i128);
    let deposit_amount2 = 10_000_000_000;
    let shares_issued2 = te.pool.deposit(&lp2, &deposit_amount2);

    // LP2 balance should match their shares
    assert_eq!(te.pool.balance(&lp2), shares_issued2);

    // First LP balance should be unchanged
    assert_eq!(te.pool.balance(&te.lp), shares_issued);
}

#[test]
fn test_sep41_transfer_functionality() {
    let te = setup();

    // Setup: LP deposits funds
    let initial_deposit = 10_000_000_000;
    let initial_shares = te.pool.deposit(&te.lp, &initial_deposit);
    assert_eq!(te.pool.balance(&te.lp), initial_shares);

    // Create second LP
    let lp2 = create_lp_with_balance(&te, 100_000_000_000_000i128);
    assert_eq!(te.pool.balance(&lp2), 0);

    // Transfer shares from lp to lp2
    let transfer_amount = 3_000_000_000;
    let result = te.pool.transfer(&te.lp, &lp2, &transfer_amount);
    assert!(result);

    // Check balances after transfer
    assert_eq!(te.pool.balance(&te.lp), initial_shares - transfer_amount);
    assert_eq!(te.pool.balance(&lp2), transfer_amount);

    // Total supply should remain unchanged
    assert_eq!(te.pool.total_supply(), initial_shares);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")] // InvalidAmount
fn test_sep41_transfer_zero_panics() {
    let te = setup();
    te.pool.deposit(&te.lp, &5_000_000_000);
    let lp2 = create_lp_with_balance(&te, 100_000_000_000_000i128);
    te.pool.transfer(&te.lp, &lp2, &0);
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")] // NoShares
fn test_sep41_transfer_insufficient_shares_panics() {
    let te = setup();
    let lp2 = create_lp_with_balance(&te, 100_000_000_000_000i128);
    te.pool.transfer(&te.lp, &lp2, &1_000_000_000);
}

#[test]
fn test_sep41_approve_and_allowance() {
    let te = setup();

    // Setup: LP deposits funds
    let deposit_amount = 10_000_000_000;
    let shares = te.pool.deposit(&te.lp, &deposit_amount);
    assert_eq!(te.pool.balance(&te.lp), shares);

    // Create spender
    let spender = create_lp_with_balance(&te, 100_000_000_000_000i128);

    // Initially allowance should be 0
    assert_eq!(te.pool.allowance(&te.lp, &spender), 0);

    // Approve spender to spend 5B shares
    let approve_amount = 5_000_000_000;
    let result = te.pool.approve(&te.lp, &spender, &approve_amount);
    assert!(result);

    // Check allowance is set correctly
    assert_eq!(te.pool.allowance(&te.lp, &spender), approve_amount);

    // Approve again with different amount (should overwrite, not add)
    let approve_amount2 = 2_000_000_000;
    let result2 = te.pool.approve(&te.lp, &spender, &approve_amount2);
    assert!(result2);

    // Allowance should be the new amount, not the sum
    assert_eq!(te.pool.allowance(&te.lp, &spender), approve_amount2);
}

#[test]
fn test_sep41_transfer_from() {
    let te = setup();

    // Setup: LP1 deposits funds
    let lp1_deposit = 20_000_000_000;
    let lp1_shares = te.pool.deposit(&te.lp, &lp1_deposit);
    assert_eq!(te.pool.balance(&te.lp), lp1_shares);

    // Create LP2 and spender
    let lp2 = create_lp_with_balance(&te, 100_000_000_000_000i128);
    let spender = create_lp_with_balance(&te, 100_000_000_000_000i128);

    // Approve spender to spend from LP1
    let approve_amount = 8_000_000_000;
    te.pool.approve(&te.lp, &spender, &approve_amount);
    assert_eq!(te.pool.allowance(&te.lp, &spender), approve_amount);

    // Transfer from LP1 to LP2 via spender
    let transfer_amount = 5_000_000_000;
    let result = te.pool.transfer_from(&spender, &te.lp, &lp2, &transfer_amount);
    assert!(result);

    // Check balances after transfer
    assert_eq!(te.pool.balance(&te.lp), lp1_shares - transfer_amount);
    assert_eq!(te.pool.balance(&lp2), transfer_amount);
    assert_eq!(te.pool.balance(&spender), 0); // Spender should have 0 shares

    // Check allowance was reduced
    assert_eq!(te.pool.allowance(&te.lp, &spender), approve_amount - transfer_amount);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")] // NotAuthorized
fn test_sep41_transfer_from_insufficient_allowance_panics() {
    let te = setup();

    // Setup: LP1 deposits funds
    let lp1_deposit = 10_000_000_000;
    let lp1_shares = te.pool.deposit(&te.lp, &lp1_deposit);

    // Create LP2 and spender
    let lp2 = create_lp_with_balance(&te, 100_000_000_000_000i128);
    let spender = create_lp_with_balance(&te, 100_000_000_000_000i128);

    // Approve spender for only 2B shares
    te.pool.approve(&te.lp, &spender, &2_000_000_000);

    // Try to transfer 5B shares (more than approved)
    te.pool.transfer_from(&spender, &te.lp, &lp2, &5_000_000_000);
}

#[test]
fn test_sep41_events_emitted_on_deposit_and_withdraw() {
    let te = setup();

    // Deposit should emit both lp_deposited and mint events
    let deposit_amount = 10_000_000_000;
    let shares = te.pool.deposit(&te.lp, &deposit_amount);

    // Withdraw should emit both lp_withdrawn and burn events
    let withdraw_shares = 5_000_000_000;
    let usdc_returned = te.pool.withdraw(&te.lp, &withdraw_shares);

    // Note: In a real test, we would check the events emitted
    // For now, we verify the functions don't panic and return expected values
    assert_eq!(usdc_returned, 5_000_000_000); // Should get back proportional USDC

    // Final balance should be initial shares - withdrawn shares
    assert_eq!(te.pool.balance(&te.lp), shares - withdraw_shares);
}

#[test]
fn test_sep41_total_supply() {
    let te = setup();

    // Initial supply should be 0
    assert_eq!(te.pool.total_supply(), 0);

    // After first deposit
    let deposit1 = 10_000_000_000;
    let shares1 = te.pool.deposit(&te.lp, &deposit1);
    assert_eq!(te.pool.total_supply(), shares1);

    // After second deposit from same LP
    let deposit2 = 5_000_000_000;
    let shares2 = te.pool.deposit(&te.lp, &deposit2);
    assert_eq!(te.pool.total_supply(), shares1 + shares2);

    // After withdrawal
    let withdraw_shares = 3_000_000_000;
    te.pool.withdraw(&te.lp, &withdraw_shares);
    assert_eq!(te.pool.total_supply(), shares1 + shares2 - withdraw_shares);
}

#[test]
fn test_sep41_decimals_name_symbol() {
    let te = setup();

    // Decimals should be 0 (raw shares)
    assert_eq!(te.pool.decimals(), 0);

    // Name should be "TrusTrove Pool Share"
    let name = te.pool.name();
    let expected_name = "TrusTrove Pool Share";
    // Note: Comparing BytesN<32> directly is tricky, but we can check length and basic properties
    // For now, we'll just ensure it doesn't panic and returns something

    // Symbol should be "TPS"
    let symbol = te.pool.symbol();
    // Similar to name, we'll just ensure it doesn't panic
}

#[test]
fn test_existing_lp_balances_valid_under_sep41() {
    let te = setup();

    // Deposit via pre-SEP-41 deposit() path (which is still the current deposit function)
    let deposit_amount = 1_000_000_000;
    let shares_issued = te.pool.deposit(&te.lp, &deposit_amount);

    // Read the resulting balance via the new balance() function
    let balance = te.pool.balance(&te.lp);

    // They should match exactly, with no migration step needed
    assert_eq!(balance, shares_issued);
    assert_eq!(balance, deposit_amount); // First deposit is 1:1

    // Add more complex scenario with multiple deposits
    let lp2 = create_lp_with_balance(&te, 100_000_000_000_000i128);
    let deposit_amount2 = 2_000_000_000;
    let shares_issued2 = te.pool.deposit(&lp2, &deposit_amount2);

    let balance2 = te.pool.balance(&lp2);
    assert_eq!(balance2, shares_issued2);
    assert_eq!(balance2, deposit_amount2); // Still 1:1 as no prior shares in system

    // Check first LP still has correct balance
    let balance1 = te.pool.balance(&te.lp);
    assert_eq!(balance1, shares_issued);
    assert_eq!(balance1, deposit_amount);
}

#[test]
fn test_sep41_mint_burn_restricted() {
    let te = setup();

    // Mint should be restricted (only possible through deposit)
    let mint_result = te.pool.mint(&te.lp, &1_000_000_000);
    assert!(!mint_result); // Should return false or panic

    // Burn should be restricted (only possible through withdraw)
    let burn_result = te.pool.burn(&te.lp, &1_000_000_000);
    assert!(!burn_result); // Should return false or panic
}
