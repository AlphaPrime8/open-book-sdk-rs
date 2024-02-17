use solana_sdk::{
    pubkey::Pubkey,
    commitment_config::CommitmentConfig,
    program_pack::Pack,
    account::Account,
    program_error::ProgramError,
    account_info::AccountInfo,
    system_instruction,
};
use solana_client::rpc_client::RpcClient;
use solana_client::rpc_filter::RpcFilterType;

use bytemuck::{Pod, Zeroable};
use std::mem::size_of;
use std::{convert::TryInto, str::FromStr};
use std::collections::HashMap;
use std::error::Error;
use borsh::{BorshDeserialize};
use bigdecimal::BigDecimal;
use std::collections::VecDeque;
use std::iter::Iterator;
use num_bigint::BigUint;

use crate::market::Market;
use crate::slab::Slab;
use crate base64;
use base64::{encode, decode};

#[derive(Debug)]
pub enum Side {
    Buy,
    Sell,
}



#[derive(Debug)]
pub enum OrderType {
    Limit,
    Ioc,
    PostOnly,
}

#[derive(Debug)]
pub enum SelfTradeBehavior {
    DecrementTake,
    CancelProvide,
    AbortTransaction,
}

#[derive(Debug)]
pub struct OrderParamsBase<T = Account> {
    pub side: Side,
    pub price: f64,
    pub size: f64,
    pub order_type: Option<OrderType>,
    pub client_id: Option<BN>,
    pub self_trade_behavior: Option<SelfTradeBehavior>,
    pub max_ts: Option<i64>,
}

#[derive(Debug)]
pub struct OrderParamsAccounts<T = Account> {
    pub owner: T,
    pub payer: Pubkey,
    pub open_orders_address_key: Option<Pubkey>,
    pub open_orders_account: Option<T>,
    pub fee_discount_pubkey: Option<Pubkey>,
    pub program_id: Option<Pubkey>,
}

#[derive(Debug)]
pub struct OrderParams<T = Account> {
    pub side: Side,
    pub price: f64,
    pub size: f64,
    pub order_type: Option<OrderType>,
    pub client_id: Option<BN>,
    pub self_trade_behavior: Option<SelfTradeBehavior>,
    pub max_ts: Option<i64>,
    pub owner: T,
    pub payer: Pubkey,
    pub open_orders_address_key: Option<Pubkey>,
    pub open_orders_account: Option<T>,
    pub fee_discount_pubkey: Option<Pubkey>,
    pub program_id: Option<Pubkey>,
    pub replace_if_exists: Option<bool>,
}

#[derive(Debug)]
pub struct SendTakeParamsBase<T = Account> {
    pub side: Side,
    pub price: f64,
    pub max_base_size: f64,
    pub max_quote_size: f64,
    pub min_base_size: f64,
    pub min_quote_size: f64,
    pub limit: Option<i64>,
}

#[derive(Debug)]
pub struct SendTakeParamsAccounts<T = Account> {
    pub owner: T,
    pub base_wallet: Pubkey,
    pub quote_wallet: Pubkey,
    pub vault_signer: Option<Pubkey>,
    pub fee_discount_pubkey: Option<Pubkey>,
    pub program_id: Option<Pubkey>,
}

#[derive(Debug)]
pub struct SendTakeParams<T = Account> {
    pub side: Side,
    pub price: f64,
    pub max_base_size: f64,
    pub max_quote_size: f64,
    pub min_base_size: f64,
    pub min_quote_size: f64,
    pub limit: Option<i64>,
    pub owner: T,
    pub base_wallet: Pubkey,
    pub quote_wallet: Pubkey,
    pub vault_signer: Option<Pubkey>,
    pub fee_discount_pubkey: Option<Pubkey>,
    pub program_id: Option<Pubkey>,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct OpenOrdersLayoutV1 {
    account_flags: u64, 
    market: [u8; 32],   
    owner: [u8; 32],    
    base_token_free: u64,
    base_token_total: u64,
    quote_token_free: u64,
    quote_token_total: u64,
    free_slot_bits: u128,
    is_bid_bits: u128,
    orders: [u128; 128],
    client_ids: [u64; 128],
    padding: [u8; 7],   
}

impl OpenOrdersLayoutV1 {
    pub fn new() -> Self {
        unsafe { std::mem::zeroed() }
    }
    pub fn size() -> usize {
        size_of::<Self>()
    }
}

const _: () = assert!(size_of::<OpenOrdersLayoutV1>() == 
    5 + size_of::<u64>() + 32 + 32 + size_of::<u64>() * 4 
    + size_of::<u128>() * 2 + size_of::<u128>() * 128 + 
    size_of::<u64>() * 128 + 7
);

pub struct OpenOrdersLayoutV2 {
    pub account_flags: u64,
    pub market: [u8; 32],
    pub owner: [u8; 32],
    pub base_token_free: u64,
    pub base_token_total: u64,
    pub quote_token_free: u64,
    pub quote_token_total: u64,
    pub free_slot_bits: u128,
    pub is_bid_bits: u128,
    pub orders: [u128; 128],
    pub client_ids: [u64; 128],
    pub referrer_rebates_accrued: u64,
    _padding: [u8; 7],
}

impl OpenOrdersLayoutV2 {
    pub fn new() -> Self {
        unsafe { std::mem::zeroed() }
    }
    pub fn size() -> usize {
        size_of::<Self>()
    }
}

pub struct OpenOrders {
    program_id: Pubkey,
    address: Pubkey,
    market: Pubkey,
    owner: Pubkey,
    base_token_free: u64,
    base_token_total: u64,
    quote_token_free: u64,
    quote_token_total: u64,
    free_slot_bits: u128,
    is_bid_bits: u128,
    orders: [u128; 128],
    client_ids: [u64; 128],
}

impl OpenOrders {
    pub fn new(address: Pubkey, 
        decoded: DecodedType,   
        program_id: Pubkey) -> Self {
            let mut instance = Self {
                address,
                program_id: program_id,
        };
        instance
    }
    fn get_layout_version(program_id: &Pubkey) -> u8 {
        unimplemented!()
    }

    fn get_layout(program_id: &Pubkey) -> &'static str {
        if get_layout_version(program_id) == 1 {
            OpenOrdersLayoutV2
        } else {
            OpenOrdersLayoutV2
        }
    }

    pub async fn get_derived_00_account_pubkey(
        owner_address: &Pubkey,
        market_address: &Pubkey,
        program_id: &Pubkey,
    ) -> Result<(Pubkey, String), Box<dyn std::error::Error>> {
        let seed = market_address.to_string().get(..32)
            .ok_or("Invalid seed length")?.to_string();
        let (public_key, _) = Pubkey::create_with_seed(
            owner_address,
            &seed,
            program_id,
        )?;
        Ok((public_key, seed))
    }

    async fn find_for_owner(
        connection: &RpcClient,
        owner_address: &str,
        program_id: &str,
    ) -> Result<Vec<OpenOrders>, Box<dyn std::error::Error>> {
        let owner_pubkey = Pubkey::from_str(owner_address)?;
        let program_pubkey = Pubkey::from_str(program_id)?;
    
        let filters = vec![
            solana_client::rpc_filter::RpcFilterType::Memcmp {
                offset: OpenOrders::get_layout(&program_pubkey).offset_of("owner")?,
                bytes: bs58::decode(owner_pubkey.to_string()).into_vec()?,
            },
            solana_client::rpc_filter::RpcFilterType::DataSize(
                OpenOrders::get_layout(&program_pubkey).span as usize,
            ),
        ];
    
        let accounts: <<Result<Vec<(Pubkey, solana_sdk::account::Account)>, 
            solana_client::client_error::ClientError> as IntoFuture>::Output as Try>::Output 
                = connection.get_program_accounts_with_config(
                    &program_pubkey,
                    solana_client::rpc_config::RpcProgramAccountsConfig {
                        filters: Some(filters),
                        account_config: solana_client::rpc_config::RpcAccountInfoConfig {
                            encoding: Some(solana_client::rpc_config::UiAccountEncoding::Base64),
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                    CommitmentConfig::confirmed(),
            ).await?;
    
        let open_orders: Vec<OpenOrders> = accounts.iter().map(|(public_key, account)| {
            OpenOrders::from_account_info(public_key, account, &program_pubkey)
        }).collect();
    
        Ok(open_orders)
    }

    async fn find_for_market_and_owner(
        connection: &RpcClient,
        market_address: &Pubkey,
        owner_address: &Pubkey,
        program_id: &Pubkey,
        force_seed_account: bool,
    ) -> Result<Vec<OpenOrders>, Box<dyn std::error::Error>> {
        let account = get_derived_oo_account_pubkey(
            owner_address,
            market_address,
            program_id,
        ).await?;
        let oo_account_info = connection.get_account(&account).await?;
        if let Some(account_info) = oo_account_info {
            return Ok(vec![
                OpenOrders::from_account_info(&account, &account_info, program_id),
            ]);
        }
        if force_seed_account {
            return Ok(vec![]);
        }
        let filters = vec![
            RpcFilterType::Memcmp(Memcmp {
                offset: get_layout(program_id)?.offset_of("market")?,
                bytes: MemcmpEncodedBytes::Base58(marketAddress.to_string()),
                encoding: None,
            }),
            RpcFilterType::Memcmp(Memcmp {
                offset: get_layout(program_id)?.offset_of("owner")?,
                bytes: MemcmpEncodedBytes::Base58(ownerAddress.to_string()),
                encoding: None,
            }),
            RpcFilterType::DataSize(get_layout(program_id)?.span()?),
        ];
        let accounts = get_filtered_program_accounts(
            connection,
            program_id,
            filters,
        ).await?;
        Ok(accounts.iter().map(|(public_key, account_info)| {
            OpenOrders::from_account_info(public_key, account_info, program_id)
        }).collect())
    }

    async fn load(
        connection: &RpcClient,
        address: &Pubkey,
        program_id: &Pubkey,
    ) -> Result<Self, Box<dyn Error>> {
        let account_info = connection.get_account_info(address).await?;
        if account_info.is_none() {
            return Err("Open orders account not found".into());
        }
        Ok(OpenOrders::from_account_info(address, &account_info.unwrap(), program_id))
    }

    pub fn from_account_info(
        address: &Pubkey,
        account_info: &AccountInfo,
        program_id: &Pubkey,
    ) -> Result<Self, ProgramError> {
        let owner = &account_info.owner;
        if owner != program_id {
            return Err(ProgramError::Custom(0));
        }
        let data = &account_info.data.borrow();
        let decoded = Self::get_layout(program_id).try_from_slice(data)?;
        if !decoded.account_flags.initialized || !decoded.account_flags.open_orders {
            return Err(ProgramError::Custom(1)); 
        }
        Ok(OpenOrders {
            address: address,
            decoded: decoded,
            program_id: program_id,
        })
    }

    async fn make_create_account_transaction(
        connection: &RpcClient,
        market_address: &Pubkey,
        owner_address: &Pubkey,
        new_account_address: &Pubkey,
        program_id: &Pubkey,
        seed: &str,
    ) -> Result<Transaction, Box<dyn std::error::Error>> {
        let lamports = connection.get_minimum_balance_for_rent_exemption(
            get_layout(program_id).span,
        ).await?;
    
        let create_account_instruction = system_instruction::create_account_with_seed(
            owner_address,
            new_account_address,
            owner_address,
            seed,
            lamports,
            get_layout(program_id).span, 
            program_id,
        );
    
        let transaction = Transaction::new_signed_with_payer(
            &[create_account_instruction],
            Some(owner_address),
            &[],
            connection.get_latest_blockhash().await?,
        );
    
        Ok(transaction)
    }

    fn public_key(&self) -> &str {
        &self.address
    }

}

pub struct Orderbook {
    market: Market,
    is_bids: bool,
    slab: Slab,
}

impl Orderbook {
    fn new(market: Market, account_flags: AccountFlags, slab: Slab) -> Result<Self, Box<dyn Error>> {
        if !account_flags.initialized || !(account_flags.bids ^ account_flags.asks) {
            Err("Invalid orderbook".into())
        } else {
            Ok(Orderbook {
                market,
                is_bids: account_flags.bids,
                slab,
            })
        }
    }

    fn layout() -> &'static OrderbookLayout {
        &ORDERBOOK_LAYOUT
    }

    fn decode(market: Market, buffer: &[u8]) -> Result<Self, Box<dyn Error>> {
        let (account_flags, slab) = Orderbook::layout().decode(buffer)?;
        Orderbook::new(market, account_flags, slab)
    }

    fn get_l2(&self, depth: usize) -> Vec<(f64, f64, BigDecimal, BigDecimal)> {
        let descending = self.is_bids;
        let mut levels: VecDeque<(BigDecimal, BigDecimal)> = VecDeque::new();
        for item in self.slab.items(descending) {
            let price = self.market.get_price_from_key(&item.key);
            if let Some(last) = levels.back_mut() {
                if last.0 == price {
                    last.1 = last.1.clone() + &item.quantity;
                    continue;
                }
            }
            if levels.len() == depth {
                break;
            }
            levels.push_back((price, item.quantity));
        }
        levels
            .into_iter()
            .map(|(price_lots, size_lots)| {
                (
                    self.market.price_lots_to_number(&price_lots),
                    self.market.base_size_lots_to_number(&size_lots),
                    price_lots,
                    size_lots,
                )
            })
            .collect()
    }

    fn items(&self, descending: bool) -> impl Iterator<Item = Order> + '_ {
        self.slab.items(descending).into_iter().map(move |item| {
            let price = get_price_from_key(item.key);
            Order {
                order_id: item.key,
                client_id: item.client_order_id,
                open_orders_address: item.owner,
                open_orders_slot: item.owner_slot,
                fee_tier: item.fee_tier,
                price: (self.market.price_lots_to_number)(price),
                price_lots: price,
                size: (self.market.base_size_lots_to_number)(item.quantity),
                size_lots: item.quantity,
                side: if self.is_bids { Side::Buy } else { Side::Sell },
            }
        })
    }

}

fn get_price_from_key(key: u64) -> u64 {
    key.ushrn(64)
}

impl IntoIterator for Orderbook {
    type Item = Order;
    type IntoIter = Box<dyn Iterator<Item = Self::Item>>;

    fn into_iter(self) -> Self::IntoIter {
        Box::new(self.items(false))
    }
}

pub struct Order {
    pub order_id: BN,
    pub open_orders_address: Pubkey,
    pub open_orders_slot: u64,
    pub price: f64,
    pub price_lots: BN,
    pub size: f64,
    pub fee_tier: u64,
    pub size_lots: BN,
    pub side: Side,
    pub client_id: Option<BN>,
}

fn get_price_from_key(key: u128) -> u64 {
    key >> 64
}

fn divide_bn_to_number(numerator: &BN, denominator: &BN) -> f64 {
    if numerator.bit_length() <= 53 && denominator.bit_length() <= 53 {
        return f64::from(numerator.to_u64().unwrap()) / f64::from(denominator.to_u64().unwrap());
    }

    let gcd_nd = numerator.gcd(denominator);

    let mut temp_numerator = numerator.div(&gcd_nd);
    let mut temp_denominator = denominator.div(&gcd_nd);

    if temp_numerator.bit_length() <= 53 && temp_denominator.bit_length() <= 53 {
        return f64::from(temp_numerator.to_u64().unwrap()) / f64::from(temp_denominator.to_u64().unwrap());
    }

    let numerator_shift = temp_numerator.leading_zeros();
    if numerator_shift > 0 {
        temp_numerator >>= numerator_shift;
    }
    let denominator_shift = temp_denominator.leading_zeros();
    if denominator_shift > 0 {
        temp_denominator >>= denominator_shift;
    }

    let exponent_bias = 2_f64.powi(numerator_shift as i32 - denominator_shift as i32);

    if temp_numerator.bit_length() <= 53 && temp_denominator.bit_length() <= 53 {
        return exponent_bias * (f64::from(temp_numerator.to_u64().unwrap()) / f64::from(temp_denominator.to_u64().unwrap()));
    }

    let string_math = f64::from_str(&temp_numerator.to_string())? / f64::from_str(&temp_denominator.to_string())?;
    return exponent_bias * string_math;
}

async fn get_mint_decimals(connection: &Connection, mint: &Pubkey) -> Result<u8, ProgramError> {
    if mint == &WRAPPED_SOL_MINT {
        return Ok(9);
    }

    let account_info = connection.get_account(mint).await?;
    let data = account_info.data.as_ref().ok_or(ProgramError::InvalidAccountData)?;

    let (_, decimals) = MINT_LAYOUT.decode(data)?;

    Ok(decimals)
}

async fn get_filtered_program_accounts(
    connection: &Connection, program_id: &Pubkey, filters: Vec<Filter>) -> Result<Vec<(Pubkey, AccountInfo)>, ProgramError> 
    {
        let resp = connection.get_program_accounts(&program_id, Some(&filters)).await?;
    
        let result = resp.into_iter().map(|v| {
            let pubkey = Pubkey::from_str(&v.pubkey).unwrap();
            let data = base64::decode(v.account.data[0]).unwrap();
            let owner = Pubkey::from_str(&v.account.owner).unwrap();
            let lamports = v.account.lamports;

            (pubkey, AccountInfo { data, executable: v.account.executable, owner, lamports })
        }).collect();

        Ok(result)
}

fn throw_if_null<T>(value: Option<T>, message: &'static str) -> Result<T, ProgramError> {
    value.ok_or_else(|| ProgramError::Custom(message.into()))
}

