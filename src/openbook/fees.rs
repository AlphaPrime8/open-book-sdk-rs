use solana_sdk::{
    address_lookup_table::program,
    account::Account,
    commitment_config::CommitmentConfig,
    pubkey::Pubkey,
    connection::Connection,
    transaction::Transaction,
    program_pack::{Pack, IsInitialized},
    system_instruction,
    sysvar::{rent, Sysvar},
    instruction::{Instruction, SystemProgram},
};
use std::collections::HashMap;
use std::io::Error;

lazy_static::lazy_static! {
    static ref PROGRAM_LAYOUT_VERSIONS: HashMap<String, u8> = {
        let mut map = HashMap::new();
        map.insert("4ckmDgGdxQoPDLUkDT3vHgSAkzA3QRdNq5ywwY4sUSJn".to_string(), 1);
        map.insert("BJ3jrUzddfuSrZHXSCxMUUQsjKEyLmuuyZebkcaFp2fg".to_string(), 1);
        map.insert("EUqojwWA2rd19FZrzeBncJsm38Jm1hEhE3zsmX3bRc2o".to_string(), 2);
        map.insert("srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX".to_string(), 3);
        map
    };
}

pub fn get_layout_version(program_id: &Pubkey) -> u8 {
    PROGRAM_LAYOUT_VERSIONS.get(&program_id.to_string()).cloned().unwrap_or(3)
}

pub fn supports_srm_fee_discounts(program_id: &Pubkey) -> bool {
    get_layout_version(program_id) > 1
}

pub fn get_fee_rates(fee_tier: u8) -> (f64, f64) {
    match fee_tier {
        1 => (0.002, -0.0003), 
        2 => (0.0018, -0.0003), 
        3 => (0.0016, -0.0003), 
        4 => (0.0014, -0.0003), 
        5 => (0.0012, -0.0003), 
        6 => (0.001, -0.0005), 
        _ => (0.0022, -0.0003), 
    }
}

pub fn get_fee_tier(msrm_balance: f64, srm_balance: f64) -> u8 {
    if msrm_balance >= 1.0 {
        6
    } else if srm_balance >= 1_000_000.0 {
        5
    } else if srm_balance >= 100_000.0 {
        4
    } else if srm_balance >= 10_000.0 {
        3
    } else if srm_balance >= 1_000.0 {
        2
    } else if srm_balance >= 100.0 {
        1
    } else {
        0
    }
}

fn get_layout_version(program_id: &Pubkey) -> u8 {
    
    if program_id.to_string() == "token_program_id" {
        2
    } else {
        1
    }
}







