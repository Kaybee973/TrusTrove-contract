use soroban_sdk::{Address, BytesN, Env, String, Symbol};
use soroban_sdk::testutils::Address as _;

pub fn lp_deposited(env: &Env, lp: &Address, usdc_amount: u128, shares_issued: u128) {
    env.events().publish(
        (Symbol::new(env, "lp_deposited"), lp.clone()),
        (usdc_amount, shares_issued),
    );
}

pub fn lp_withdrawn(env: &Env, lp: &Address, usdc_amount: u128, shares_burned: u128) {
    env.events().publish(
        (Symbol::new(env, "lp_withdrawn"), lp.clone()),
        (usdc_amount, shares_burned),
    );
}

pub fn invoice_funded(env: &Env, invoice_id: &BytesN<32>, funded_amount: u128) {
    env.events().publish(
        (Symbol::new(env, "invoice_funded"), invoice_id.clone()),
        funded_amount,
    );
}

pub fn repayment_received(env: &Env, invoice_id: &BytesN<32>, amount: u128, yield_amount: u128) {
    env.events().publish(
        (Symbol::new(env, "repayment_received"), invoice_id.clone()),
        (amount, yield_amount),
    );
}

pub fn invoice_defaulted(env: &Env, invoice_id: &BytesN<32>, loss_amount: u128) {
    env.events().publish(
        (Symbol::new(env, "invoice_defaulted"), invoice_id.clone()),
        loss_amount,
    );
}

// SEP-41 Events
pub fn transfer(env: &Env, from: &Address, to: &Address, amount: u128) {
    env.events().publish(
        (Symbol::new(env, "transfer"), from.clone(), to.clone()),
        amount,
    );
}

pub fn approval(env: &Env, owner: &Address, spender: &Address, amount: u128) {
    env.events().publish(
        (Symbol::new(env, "approval"), owner.clone(), spender.clone()),
        amount,
    );
}

pub fn mint(env: &Env, to: &Address, amount: u128) {
    // For mint events, use zero address as minter since minting is restricted
    let zero_address = Address::generate(&env);
    env.events().publish(
        (Symbol::new(env, "mint"), zero_address, to.clone()),
        amount,
    );
}

pub fn burn(env: &Env, from: &Address, amount: u128) {
    // For burn events, use zero address as burner since burning is restricted
    let zero_address = Address::generate(&env);
    env.events().publish(
        (Symbol::new(env, "burn"), from.clone(), zero_address),
        amount,
    );
}
