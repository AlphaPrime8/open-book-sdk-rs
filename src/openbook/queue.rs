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

use bitflags::bitflags;
// use std::io::Error;

#[derive(Debug)]
struct RequestQueueHeader {
    account_flags: AccountFlags,
    head: u32,
    count: u32,
    next_seq_num: u32,
    _padding: [u8; 12], 
}

impl RequestQueueHeader {
    fn decode(input: &[u8]) -> Self {
        let (account_flags, head, count, next_seq_num, _, padding) = struct_def![
            blob(5),
            AccountFlags,
            u32,
            zeros(4),
            u32,
            zeros(4)
        ]
        .unpack(input)
        .unwrap();

        RequestQueueHeader {
            account_flags,
            head,
            count,
            next_seq_num,
            _padding: padding,
        }
    }
}

#[derive(Debug)]
pub struct RequestFlags {
    pub new_order: bool,
    pub cancel_order: bool,
    pub bid: bool,
    pub post_only: bool,
    pub ioc: bool,
}

impl Pack for RequestFlags {
    const LEN: usize = 1;

    fn unpack_from_slice(src: &[u8]) -> Result<Self, std::io::Error> {
        if src.len() < Self::LEN {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Input too short"));
        }

        let request_flags_byte = src[0];
        let new_order = (request_flags_byte >> 4) & 1 == 1;
        let cancel_order = (request_flags_byte >> 3) & 1 == 1;
        let bid = (request_flags_byte >> 2) & 1 == 1;
        let post_only = (request_flags_byte >> 1) & 1 == 1;
        let ioc = request_flags_byte & 1 == 1;

        Ok(Self {
            new_order,
            cancel_order,
            bid,
            post_only,
            ioc,
        })
    }
}

pub struct Request {
    pub flags: RequestFlags,
    pub open_orders_slot: u8,
    pub fee_tier: u8,
    pub blob: [u8; 5],
    pub max_base_size_or_cancel_id: u64,
    pub native_quote_quantity_locked: u64,
    pub order_id: u128,
    pub open_orders: Pubkey,
    pub client_order_id: u64,
}

pub struct EventQueueHeader {
    blob: [u8; 5],
    account_flags: AccountFlags,
    head: u32,
    count: u32,
    seq_num: u32,
}

bitflags! {
    pub struct EventFlags: u8 {
        const FILL = 0b0000_0001;
        const OUT = 0b0000_0010;
        const BID = 0b0000_0100;
        const MAKER = 0b0000_1000;
    }
}

pub struct Event {
    flags: EventFlags,
    open_orders_slot: u8,
    fee_tier: u8,
    blob: [u8; 5],
    native_quantity_released: u64,
    native_quantity_paid: u64,
    native_fee_or_rebate: u64,
    order_id: u128,
    open_orders: Pubkey,
    client_order_id: u64,
}

pub struct Event {
    event_flags: EventFlags,
    seq_num: Option<u32>,
    order_id: u128,
    open_orders: Pubkey,
    open_orders_slot: u8,
    fee_tier: u8,
    native_quantity_released: u128,
    native_quantity_paid: u128,
    native_fee_or_rebate: u128,
}

