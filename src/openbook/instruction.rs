use crate::error::

use solana_client::{
    client_error::ClientError, 
    rpc_client::RpcClient, 
    rpc_request::RpcRequest,
    rpc_config::RpcProgramAccountsConfig,
};
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

use borsh::{BorshDeserialize, BorshSerialize};


const TOKEN_PROGRAM_ID: Pubkey = Pubkey::new_from_array([
    4, 203, 124, 179, 243, 185, 36, 150, 209, 73, 91, 131, 15, 200, 87, 250, 50, 35, 220, 
    90, 139, 198, 193, 19, 236, 115, 82, 71, 244, 60, 187, 249,
]);

const SETTLE_FUNDS_BASE_WALLET_INDEX: u8 = 5;
const SETTLE_FUNDS_QUOTE_WALLET_INDEX: u8 = 6;

const NEW_ORDER_OPEN_ORDERS_INDEX: u8 = 1;
const NEW_ORDER_OWNER_INDEX: u8 = 4;

const NEW_ORDER_V3_OPEN_ORDERS_INDEX: u8 = 1;
const NEW_ORDER_V3_OWNER_INDEX: u8 = 7;

pub struct InstructionLayout {
    pub instruction: u32,
}

impl InstructionLayout {
    pub fn new(instruction: u32) -> Self {
        Self { instruction }
    }
}

pub enum Instruction {
    InitializeMarket(InitializeMarket),
    NewOrder(NewOrder),
    MatchOrders(MatchOrders),
    ConsumeEvents(ConsumeEvents),
    CancelOrder(CancelOrder),
    SettleFunds(SettleFunds),
    CancelOrderByClientId(CancelOrderByClientId),
    NewOrderV3(NewOrderV3),
    CancelOrderV2(CancelOrderV2),
    CancelOrderByClientIdV2(CancelOrderByClientIdV2),
    SendTake(SendTake),
    CloseOpenOrders(CloseOpenOrders),
    InitOpenOrders(InitOpenOrders),
    Prune(Prune),
    ConsumeEventsPermissioned(ConsumeEventsPermissioned),
    CancelOrdersByClientIds(CancelOrdersByClientIds),
    ReplaceOrderByClientId(ReplaceOrderByClientId),
    ReplaceOrdersByClientIds(ReplaceOrdersByClientIds),
}

impl Pack for Instruction {
    const LEN: usize = std::mem::size_of::<u32>();

    fn pack_into_slice(&self, dst: &mut [u8]) {
        let mut writer = std::io::Cursor::new(dst);
        match self {
            Instruction::InitializeMarket(inner) => {
                writer.write_all(&[0]).unwrap();
                inner.serialize(&mut writer).unwrap();
            }
            Instruction::NewOrder(inner) => {
                writer.write_all(&[1]).unwrap();
                inner.serialize(&mut writer).unwrap();
            }
            Instruction::MatchOrders(inner) => {
                writer.write_all(&[2]).unwrap();
                inner.serialize(&mut writer).unwrap();
            }
            Instruction::ConsumeEvents(inner) => {
                writer.write_all(&[3]).unwrap();
                inner.serialize(&mut writer).unwrap();
            }
            Instruction::CancelOrder(inner) => {
                writer.write_all(&[4]).unwrap();
                inner.serialize(&mut writer).unwrap();
            }
            Instruction::SettleFunds(inner) => {
                writer.write_all(&[5]).unwrap();
                inner.serialize(&mut writer).unwrap();
            }
            Instruction::CancelOrderByClientId(inner) => {
                writer.write_all(&[6]).unwrap();
                inner.serialize(&mut writer).unwrap();
            }
            Instruction::NewOrderV3(inner) => {
                writer.write_all(&[7]).unwrap();
                inner.serialize(&mut writer).unwrap();
            }
            Instruction::CancelOrderV2(inner) => {
                writer.write_all(&[8]).unwrap();
                inner.serialize(&mut writer).unwrap();
            }
            Instruction::CancelOrderByClientIdV2(inner) => {
                writer.write_all(&[9]).unwrap();
                inner.serialize(&mut writer).unwrap();
            }
            Instruction::SendTake(inner) => {
                writer.write_all(&[10]).unwrap();
                inner.serialize(&mut writer).unwrap();
            }
            Instruction::CancelOrdersByClientIds(inner) => {
                writer.write_all(&[11]).unwrap();
                inner.serialize(&mut writer).unwrap();
            }
            Instruction::ReplaceOrderByClientId(inner) => {
                writer.write_all(&[12]).unwrap();
                inner.serialize(&mut writer).unwrap();
            }
            Instruction::ReplaceOrdersByClientIds(inner) => {
                writer.write_all(&[13]).unwrap();
                inner.serialize(&mut writer).unwrap();
            }
            Instruction::ReplaceOrdersByClientIds(inner) => {
                writer.write_all(&[14]).unwrap();
                inner.serialize(&mut writer).unwrap();
            }
        }
    }

    fn unpack_from_slice(src: &[u8]) -> Result<Self, borsh::Error> {
        let mut reader = std::io::Cursor::new(src);
        let variant: u32 = borsh::BorshDeserialize::deserialize(&mut reader)?;
        match variant {
            0 => {
                let inner: InitializeMarket = borsh::BorshDeserialize::deserialize(&mut reader)?;
                Ok(Instruction::InitializeMarket(inner))
            }
            1 => {
                let inner: NewOrder = borsh::BorshDeserialize::deserialize(&mut reader)?;
                Ok(Instruction::NewOrder(inner))
            }
            2 => {
                let inner: MatchOrders = borsh::BorshDeserialize::deserialize(&mut reader)?;
                Ok(Instruction::MatchOrders(inner))
            }
            3 => {
                let inner: ConsumeEvents = borsh::BorshDeserialize::deserialize(&mut reader)?;
                Ok(Instruction::ConsumeEvents(inner))
            }
            4 => {
                let inner: CancelOrder = borsh::BorshDeserialize::deserialize(&mut reader)?;
                Ok(Instruction::CancelOrder(inner))
            }
            5 => Ok(Instruction::SettleFunds),
            6 => {
                let inner: CancelOrderByClientId = borsh::BorshDeserialize::deserialize(&mut reader)?;
                Ok(Instruction::CancelOrderByClientId(inner))
            }
            7 => {
                let inner: NewOrderV3 = borsh::BorshDeserialize::deserialize(&mut reader)?;
                Ok(Instruction::NewOrderV3(inner))
            }
            8 => {
                let inner: CancelOrderV2 = borsh::BorshDeserialize::deserialize(&mut reader)?;
                Ok(Instruction::CancelOrderV2(inner))
            }
            9 => {
                let inner: CancelOrderByClientIdV2 = borsh::BorshDeserialize::deserialize(&mut reader)?;
                Ok(Instruction::CancelOrderByClientIdV2(inner))
            }
            10 => {
                let inner: SendTake = borsh::BorshDeserialize::deserialize(&mut reader)?;
                Ok(Instruction::SendTake(inner))
            }
            11 => {
                let inner: CloseOpenOrders = borsh::BorshDeserialize::deserialize(&mut reader)?;
                Ok(Instruction::CloseOpenOrders(inner))
            }
            12 => {
                let inner: InitOpenOrders = borsh::BorshDeserialize::deserialize(&mut reader)?;
                Ok(Instruction::InitOpenOrders(inner))
            }
            13 => {
                let inner: Prune = borsh::BorshDeserialize::deserialize(&mut reader)?;
                Ok(Instruction::Prune(inner))
            }
            14 => {
                let inner: ConsumeEventsPermissioned = borsh::BorshDeserialize::deserialize(&mut reader)?;
                Ok(Instruction::ConsumeEventsPermissioned(inner))
            }
            15 => {
                let inner: ReplaceOrderByClientId = borsh::BorshDeserialize::deserialize(&mut reader)?;
                Ok(Instruction::ReplaceOrderByClientId(inner))
            }
            16 => {
                let inner: ReplaceOrdersByClientIds = borsh::BorshDeserialize::deserialize(&mut reader)?;
                Ok(Instruction::ReplaceOrdersByClientIds(inner))
            }
            // Add other variants as needed
            _ => Err(borsh::Error::InvalidData),
        }
    }




            


}

