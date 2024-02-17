use num_enum::TryFromPrimitive;
use std::collections::HashMap;
use num_enum::{FromPrimitive, IntoPrimitive};
use thiserror::Error;
use std::collections::HashMap;
use std::io::Error;

use solana_sdk::{
    transaction::Transaction,
};
use solana_program::{
    program_error::ProgramError,
};


lazy_static::lazy_static! {
    static ref PROGRAM_LAYOUT_VERSIONS: HashMap<String, u8> = {
        let mut map = HashMap::new();
        map.insert("4ckmDgGdxQoPDLUkDT3vHgSAkzA3QRdNq5ywwY4sUSJn".to_string(), 1);
        map.insert("BJ3jrUzddfuSrZHXSCxMUUQsjKEyLmuuyZebkcaFp2fg".to_string(), 1);
        map.insert("EUqojwWA2rd19FZrzeBncJsm38Jm1hEhE3zsmX3bRc2o".to_string(), 2);
        map.insert("srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX".to_string(), 3);
        map
    };
    static ref KNOWN_PROGRAMS: HashMap<&'static str, &'static str> = {
        let mut map = HashMap::new();
        map.insert(TOKEN_PROGRAM_ID, "Token program");
        map.insert(SYSTEM_PROGRAM_ID, "System program");
        map
    };
}

fn get_layout_version(program_id: &Pubkey) -> u8 {
    PROGRAM_LAYOUT_VERSIONS.get(&program_id.to_string()).cloned().unwrap_or(3)
}

#[derive(Debug, PartialEq, Eq, TryFromPrimitive)]
#[repr(u32)]
pub enum DexError {
    InvalidMarketFlags = 0,
    InvalidAskFlags,
    InvalidBidFlags,
    InvalidQueueLength,
    OwnerAccountNotProvided,

    ConsumeEventsQueueFailure,
    WrongCoinVault,
    WrongPcVault,
    WrongCoinMint,
    WrongPcMint,

    CoinVaultProgramId = 10,
    PcVaultProgramId,
    CoinMintProgramId,
    PcMintProgramId,

    WrongCoinMintSize,
    WrongPcMintSize,
    WrongCoinVaultSize,
    WrongPcVaultSize,

    UninitializedVault,
    UninitializedMint,

    CoinMintUninitialized = 20,
    PcMintUninitialized,
    WrongMint,
    WrongVaultOwner,
    VaultHasDelegate,

    AlreadyInitialized,
    WrongAccountDataAlignment,
    WrongAccountDataPaddingLength,
    WrongAccountHeadPadding,
    WrongAccountTailPadding,

    RequestQueueEmpty = 30,
    EventQueueTooSmall,
    SlabTooSmall,
    BadVaultSignerNonce,
    InsufficientFunds,

    SplAccountProgramId,
    SplAccountLen,
    WrongFeeDiscountAccountOwner,
    WrongFeeDiscountMint,

    CoinPayerProgramId,
    PcPayerProgramId = 40,
    ClientIdNotFound,
    TooManyOpenOrders,

    FakeErrorSoWeDontChangeNumbers,
    BorrowError,

    WrongOrdersAccount,
    WrongBidsAccount,
    WrongAsksAccount,
    WrongRequestQueueAccount,
    WrongEventQueueAccount,

    RequestQueueFull = 50,
    EventQueueFull,
    MarketIsDisabled,
    WrongSigner,
    TransferFailed,
    ClientOrderIdIsZero,

    WrongRentSysvarAccount,
    RentNotProvided,
    OrdersNotRentExempt,
    OrderNotFound,
    OrderNotYours,

    WouldSelfTrade,

    Unknown = 1000,
}

const TOKEN_PROGRAM_ID: &str = "Token program";
const SYSTEM_PROGRAM_ID: &str = "System program";

#[derive(Debug)]
struct CustomError {
    custom: u32,
}

#[derive(Debug)]
enum InstructionError {
    Custom(u32),
}

fn parse_instruction_error_response(
    transaction: &Transaction,
    error_response: &InstructionError,
) -> (usize, String, String) {
    let failed_instruction_index: usize = match *error_response {
        InstructionError::Custom(custom_error) => custom_error as usize,
    };

    let failed_instruction = transaction.instructions.get(failed_instruction_index);
    let parsed_error: String;
    let failed_program: String;

    match failed_instruction {
        Some(instruction) => {
            let program_id = instruction.program_id.to_string();
            if PROGRAM_LAYOUT_VERSIONS.contains_key(&program_id) {
                parsed_error = format!(
                    "{}",
                    PROGRAM_LAYOUT_VERSIONS[&program_id]
                );
            } else if KNOWN_PROGRAMS.contains_key(&program_id) {
                let program = KNOWN_PROGRAMS[&program_id];
                parsed_error = format!("{} error {}", program, failed_instruction_index);
            } else {
                parsed_error = format!(
                    "Unknown program {} custom error: {}",
                    program_id,
                    failed_instruction_index
                );
            }
            failed_program = program_id;
        }
        None => {
            parsed_error = "Failed instruction not found".to_string();
            failed_program = String::new();
        }
    }

    (failed_instruction_index, parsed_error, failed_program)
}
