use std::collections::HashMap;
use connection::Connection;
use std::error::Error;
use tokio;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use num_bigint::{BigUint, ToBigUint};
use serde;
use std::str::FromStr;

// use solana_client::nonblocking::rpc_client::RpcClient;
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


use super::order::OpenOrders;
use anyhow::format_err;
use serde::{Value, Error};

use crate::order::Order;
use crete::queue;
use crate::slab::{Slab, SLAB_LAYOUT};

pub struct AccountInfo {
    balance: u64,
    mint: Pubkey,
    pubkey: Pubkey,
    fee_tier: u64,
}

pub struct FeeDiscountKeyCache {
    accounts: Vec<AccountInfo>,
    ts: u64,
}

#[derive(Debug)]
pub struct MarketOptions {
    pub skip_preflight: Option<bool>,
    pub commitment: Option<String>,
}

pub struct Market {
    decoded: serde::Value,
    base_mint_decimals: u8,
    quote_mint_decimals: u8,
    base_spl_token_decimals: u32,
    quote_spl_token_decimals: u32,
    skip_preflight: bool,
    commitment: CommitmentConfig,
    program_id: Pubkey,
    open_orders_accounts_cache: HashMap<Pubkey, OpenOrders>,
    layout_override: Option<()>, 
    fee_discount_keys_cache: HashMap<String, FeeDiscountKeyCache>,
    
}

impl Market {

    pub fn new(
        decoded: serde::Value,
        base_mint_decimals: u8,
        quote_mint_decimals: u8,
        options: MarketOptions,
        program_id: Pubkey,
        layout_override: Option<LayoutOverride>,
    ) -> Result<Market, &'static str> {
        let MarketOptions {
            skip_preflight,
            commitment,
        } = options;

        if !decoded.account_flags.initialized || !decoded.account_flags.market {
            return Err("Invalid market state");
        }

        Ok(Market {
            decoded,
            base_mint_decimals,
            quote_mint_decimals,
            skip_preflight: skip_preflight.unwrap_or(false),
            commitment: commitment.unwrap_or_else(|| "recent".to_string()),
            program_id,
            open_orders_accounts_cache: HashMap::new(),
            fee_discount_keys_cache: HashMap::new(),
            layout_override,
        })
    }

    async fn load(
        rpc_client: &RpcClient,
        address: Pubkey,
        options: MarketOptions,
        program_id: Pubkey,
        layout_override: Option<&str>,
    ) -> Result<Market, Box<dyn Error>> {
        let account_info = rpc_client.get_account_info(&address)?;

        let account = account_info
            .ok_or_else(|| "Market not found")?;

        if account.owner != program_id {
            return Err(format!("Address not owned by program: {}", account.owner).into());
        }

        let layout = layout_override.unwrap_or_else(|| Self::get_layout(program_id));

        let decoded = layout.decode(&account.data);
        
        if !decoded.account_flags.initialized || !decoded.account_flags.market || !decoded.own_address != address {
            return Err("Invalid market".into());
        }

        let (base_mint_decimals, quote_mint_decimals) = tokio::try_join!(
            get_mint_decimals(rpc_client, decoded.base_mint),
            get_mint_decimals(rpc_client, decoded.quote_mint)
        )?;

        Ok(Market {
            decoded,
            base_mint_decimals,
            quote_mint_decimals,
            program_id,
            // layout_override,
        }) 
    }

    pub async fn get_mint_decimals(rpc_client: &RpcClient, mint: Pubkey) -> Result<(u8, u8), Box<dyn Error>> {
        
        let account_info = rpc_client.get_account_info(&mint)?;
        let account = account_info.ok_or("Mint not found")?;
        
        let decimals = {
            let data = account.data.to_vec();
            let decoded = mint_layout.decode(&data).map_err(|e| format!("Failed to decode mint data: {:?}", e))?;
            decoded.decimals
        };
    
        Ok(decimals)
    }

    pub fn program_id(&self) -> Pubkey {
        self._program_id
    }

    pub fn address(&self) -> Pubkey {
        self._decoded.own_address
    }

    pub fn public_key(&self) -> Pubkey {
        self.address()
    }

    pub fn base_mint_address(&self) -> Pubkey {
        self._decoded.base_mint
    }

    pub fn quote_mint_address(&self) -> Pubkey {
        self._decoded.quote_mint
    }

    pub fn bids_address(&self) -> Pubkey {
        self._decoded.bids
    }

    pub fn asks_address(&self) -> Pubkey {
        self._decoded.asks
    }

    pub fn decoded(&self) -> &YourDecodedStruct {
        &self._decoded
    }

    fn get_layout(program_id: &Pubkey) -> &'static [u8] {
        if get_layout_version(program_id) == 1 {
            &_MARKET_STAT_LAYOUT_V1
        } else {
            &MARKET_STATE_LAYOUT_V2
        }
    }

    async fn find_accounts_by_mints(
        connection: &Connection,
        base_mint_address: &Pubkey,
        quote_mint_address: &Pubkey,
        program_id: &Pubkey,
    ) -> Result<Vec<AccountInfo>, Box<dyn std::error::Error>> {
        let filters = vec![
            Memcmp {
                offset: Self::get_layout(program_id).offset_of("baseMint") as u64,
                bytes: base_mint_address.to_string().into_bytes(),
            },
            Memcmp {
                offset: Self::get_layout(program_id).offset_of("quoteMint") as u64,
                bytes: quote_mint_address.to_string().into_bytes(),
            },
        ];
        get_filtered_program_accounts(connection, program_id, &filters).await
    }

    async fn load_bids(&self, connection: &RpcClient) -> Result<Orderbook, Box<dyn std::error::Error>> {
        let bids_account = connection.get_account(&self.decoded.bids).await?.value;
        
        let data = bids_account.data.ok_or("Account data not found")?;
        
        Ok(Orderbook::decode(&self, &data))
    }

    async fn load_asks(&self, connection: &RpcClient) -> Result<Orderbook, Box<dyn std::error::Error>> {
        let asks_account = connection.get_account(&self.decoded.asks).await?.value;
        
        let data = asks_account.data.ok_or("Account data not found")?;
        
        Ok(Orderbook::decode(&self, &data))
    }

    async fn load_orders_for_owner(
        &self,
        connection: &RpcClient,
        owner_address: Pubkey,
        cache_duration_ms: u64,
    ) -> Result<Vec<Order>, Box<dyn std::error::Error>> {
        let (bids, asks, open_orders_accounts) = tokio::try_join!(
            self.load_bids(connection),
            self.load_asks(connection),
            self.find_open_orders_accounts_for_owner(connection, owner_address, cache_duration_ms)
        )?;
        
        let filtered_orders = self.filter_for_open_orders(&bids, &asks, &open_orders_accounts);
        
        Ok(filtered_orders)
    }

    pub fn filter_for_open_orders(bids: &Orderbook, asks: &Orderbook, open_orders_accounts: &[OpenOrders]) -> Vec<Order> {
        let mut orders_map: HashMap<Pubkey, ()> = HashMap::new();
    
        for order in bids.iter().chain(asks.iter()) {
            for open_orders in open_orders_accounts.iter() {
                if order.open_orders_address == open_orders.address {
                    orders_map.insert(order.id, ());
                }
            }
        }
    
        let filtered_orders: Vec<Order> = bids
            .iter()
            .cloned()
            .chain(asks.iter().cloned())
            .filter(|order| orders_map.contains_key(&order.id))
            .collect();
    
        filtered_orders
    }

    async fn find_base_token_accounts_for_owner(
        connection: Arc<RpcClient>,
        owner_address: Pubkey,
        include_unwrapped_sol: bool,
    ) -> Result<Vec<(Pubkey, Account)>, Box<dyn std::error::Error>> {
        if base_mint_address == WRAPPED_SOL_MINT && include_unwrapped_sol {
            // let (wrapped, unwrapped) = tokio::try_join!(
            //     find_base_token_accounts_for_owner(connection.clone(), owner_address, false),
            //     connection.get_account(&owner_address),
            // )?;
            
            if let Some(unwrapped_account) = unwrapped {
                let mut accounts = vec![(owner_address, unwrapped_account)];
                accounts.extend(wrapped);
                return Ok(accounts);
            }
            
            return Ok(wrapped);
        }
        
        let token_accounts = get_token_accounts_by_owner_for_mint(connection.clone(), owner_address, base_mint_address).await?;
        Ok(token_accounts)
    }  
    
    async fn get_token_accounts_by_owner_for_mint(
        connection: &solana_client::rpc_client::RpcClient,
        owner_address: Pubkey,
        mint_address: Pubkey,
    ) -> Result<Vec<(Pubkey, Account)>, Box<dyn std::error::Error>> {
        let token_accounts = connection
            .get_token_accounts_by_owner(&owner_address, Some(solana_client::rpc_config::RpcTokenAccountsFilter::Mint(mint_address)))
            .await?;
        
        Ok(token_accounts.value.into_iter().map(|(pubkey, account)| (pubkey, account)).collect())
    }

    async fn find_quote_token_accounts_for_owner(
        connection: &RpcClient,
        owner_address: &Pubkey,
        include_unwrapped_sol: bool,
    ) -> Vec<(Pubkey, Account)> {
        if this.quote_mint_address == WRAPPED_SOL_MINT && include_unwrapped_sol {
            let (wrapped, unwrapped) = futures::join!(
                find_quote_token_accounts_for_owner(connection, owner_address, false),
                connection.get_account(owner_address)
            );
            
            if let Some(unwrapped_account) = unwrapped {
                return vec![(owner_address.clone(), unwrapped_account), wrapped];
            }
            
            return wrapped;
        }
    
        return get_token_accounts_by_owner_for_mint(connection, owner_address, this.quote_mint_address);
    }

    struct OpenOrdersCache {
        accounts: Vec<OpenOrders>,
        ts: u128,
    }

    pub async fn find_open_orders_accounts_for_owner(
        connection: &RpcClient,
        owner_address: &Pubkey,
        cache_duration_ms: u128,
        force_seed_account: bool,
        open_orders_accounts_cache: &mut HashMap<String, OpenOrdersCache>,
    ) -> Vec<OpenOrders> {
        let str_owner = owner_address.to_string();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_millis();
        
        if let Some(cache) = open_orders_accounts_cache.get(&str_owner) {
            if now - cache.ts < cache_duration_ms {
                return cache.accounts.clone();
            }
        }
    
        let open_orders_accounts_for_owner = OpenOrders::find_for_market_and_owner(
            connection,
            &self.address,
            owner_address,
            &self.program_id,
            force_seed_account,
        ).await;
    
        open_orders_accounts_cache.insert(str_owner.clone(), OpenOrdersCache {
            accounts: open_orders_accounts_for_owner.clone(),
            ts: now
        });
    
        open_orders_accounts_for_owner
    }

    #[derive(Default)]
    pub struct OrderParamsAccounts<T = Account> {
        pub owner: T,
        pub payer: Pubkey,
        pub open_orders_address_key: Option<Pubkey>,
        pub open_orders_account: Option<T>,
        pub fee_discount_pubkey: Option<Pubkey>,
        pub program_id: Option<Pubkey>,
    }

    async fn replace_orders(
        connection: &RpcClient,
        accounts: &mut OrderParamsAccounts,
        orders: Vec<OrderParamsBase>,
        cache_duration_ms: u128,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if accounts.open_orders_account.is_none() && accounts.open_orders_address_key.is_none() {
            let owner_address: Pubkey = accounts.owner;
            let open_orders_accounts = find_open_orders_accounts_for_owner(connection, &owner_address, cache_duration_ms).await;
            
            if let Some(first_open_orders) = open_orders_accounts.get(0) {
                accounts.open_orders_address_key = Some(first_open_orders.address);
            }
        }
    
        let mut transaction = Transaction::new();
        let replace_orders_instruction = make_replace_orders_by_client_ids_instruction(accounts, orders);
        transaction.add(replace_orders_instruction);
    
        let owner = vec![accounts.owner];
        let result = send_transaction(connection, &transaction, &owner).await;
        Ok(result)
    }

    async fn place_order(
        connection: &RpcClient,
        order_params: OrderParams,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let OrderParams {
            owner,
            payer,
            side,
            price,
            size,
            order_type,
            client_id,
            open_orders_address_key,
            open_orders_account,
            fee_discount_pubkey,
            max_ts,
            replace_if_exists,
        } = order_params;
    
        let place_order_transaction = make_place_order_transaction::<Account>(connection, PlaceOrderTransactionParams {
            owner,
            payer,
            side,
            price,
            size,
            order_type,
            client_id,
            open_orders_address_key,
            open_orders_account,
            fee_discount_pubkey,
            max_ts,
            replace_if_exists,
        }).await?;
    
        let mut signers = place_order_transaction.signers.clone();
        signers.insert(0, owner);
    
        let transaction = place_order_transaction.transaction;
        let result = send_transaction(connection, &transaction, &signers).await;
        Ok(result)
    }

    async fn send_take(
        connection: &RpcClient,
        send_take_params: SendTakeParams,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let SendTakeParams {
            owner,
            base_wallet,
            quote_wallet,
            side,
            price,
            max_base_size,
            max_quote_size,
            min_base_size,
            min_quote_size,
            limit,
            program_id,
            fee_discount_pubkey,
        } = send_take_params;
    
        let send_take_transaction = make_send_take_transaction::<Account>(connection, SendTakeTransactionParams {
            owner,
            base_wallet,
            quote_wallet,
            side,
            price,
            max_base_size,
            max_quote_size,
            min_base_size,
            min_quote_size,
            limit,
            program_id.unwrap_or_default(),
            fee_discount_pubkey.unwrap_or_default(),
        }).await?;
    
        let mut signers = send_take_transaction.signers.clone();
        signers.insert(0, owner); 
    
        let transaction = send_take_transaction.transaction;
        let result = send_transaction(connection, &transaction, &signers).await;
        Ok(result)
    }

    fn get_spl_token_balance_from_account_info(account_info: &AccountInfo<>, decimals: u32) -> f64 {
        let data_slice = &account_info.data[64..72];
        let balance_big_uint = BigUint::from_bytes_le(data_slice);
        let scale_factor = 10u32.to_biguint().unwrap().pow(decimals);
        
        let balance = balance_big_uint / scale_factor;
        
        balance.to_f64().unwrap()
    }

    fn supports_srm_fee_discounts(program_id: &Pubkey) -> bool {
        supports_srm_fee_discounts(this._programId)
    }
    
    fn supports_referral_fees(program_id: &Pubkey) -> bool {
        get_layout_version(program_id) > 1
    }
    
    fn uses_request_queue(program_id: &Pubkey) -> bool {
        get_layout_version(program_id) <= 2
    }

    async fn find_fee_discount_keys(
        connection: &RpcClient,
        owner_address: &Pubkey,
        cache_duration_ms: u128,
    ) -> Vec<FeeDiscountKey> {
        let mut sorted_accounts: Vec<FeeDiscountKey> = Vec::new();
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis();
        let str_owner = owner_address.to_string();

        if let Some(cache) = fee_discount_keys_cache.get(&str_owner) {
            if now - cache.ts < cache_duration_ms {
                return cache.accounts.clone();
            }
        }

        if supports_srm_fee_discounts(&program_id) {
            let msrm_accounts = get_token_accounts_by_owner_for_mint(connection, owner_address, MSRM_MINT).await
                .into_iter()
                .map(|(pubkey, account)| {
                    let balance = get_spl_token_balance_from_account_info(&account, MSRM_DECIMALS);
                    FeeDiscountKey {
                        pubkey: pubkey,
                        mint: MSRM_MINT,
                        balance: balance,
                        fee_tier: get_fee_tier(balance, 0),
                    }
                })
                .collect::<Vec<FeeDiscountKey>>();

            let srm_accounts = get_token_accounts_by_owner_for_mint(connection, owner_address, SRM_MINT).await
                .into_iter()
                .map(|(pubkey, account)| {
                    let balance = get_spl_token_balance_from_account_info(&account, SRM_DECIMALS);
                    FeeDiscountKey {
                        pubkey: pubkey,
                        mint: SRM_MINT,
                        balance: balance,
                        fee_tier: get_fee_tier(0, balance),
                    }
                })
                .collect::<Vec<FeeDiscountKey>>();

            sorted_accounts = msrm_accounts.iter().chain(srm_accounts.iter()).cloned().collect();
            sorted_accounts.sort_by(|a, b| {
                if a.fee_tier > b.fee_tier {
                    return std::cmp::Ordering::Less;
                } else if a.fee_tier < b.fee_tier {
                    return std::cmp::Ordering::Greater;
                } else {
                    if a.balance > b.balance {
                        return std::cmp::Ordering::Less;
                    } else if a.balance < b.balance {
                        return std::cmp::Ordering::Greater;
                    } else {
                        return std::cmp::Ordering::Equal;
                    }
                }
            });
        }

        fee_discount_keys_cache.insert(str_owner.clone(), FeeDiscountKeysCache {
            accounts: sorted_accounts.clone(),
            ts: now,
        });

        sorted_accounts
    }

    async fn find_best_fee_discount_key(
        connection: &RpcClient,
        owner_address: &Pubkey,
        cache_duration_ms: u128,
    ) -> FeeDiscountKey {
        let accounts = find_fee_discount_keys(connection, owner_address, cache_duration_ms).await;
        
        if let Some(best_account) = accounts.iter().min_by(|a, b| a.fee_tier.cmp(&b.fee_tier)) {
            return FeeDiscountKey {
                pubkey: best_account.pubkey,
                fee_tier: best_account.fee_tier,
            };
        }
    
        FeeDiscountKey {
            pubkey: Pubkey::default(), 
            fee_tier: 0,
        }
    }

    async fn make_place_order_transaction<T: Pack>(
        connection: &RpcClient,
        order_params: OrderParams<T>,
        cache_duration_ms: u64,
        fee_discount_pubkey_cache_duration_ms: u64,
    ) -> Result<(Transaction, Vec<Account>, Pubkey), Box<dyn std::error::Error> {
        let owner_address: Pubkey = order_params.owner.public_key.unwrap_or(order_params.owner);
        let open_orders_accounts = find_open_orders_accounts_for_owner(connection, &owner_address, cache_duration_ms).await?;
        let mut transaction = Transaction::new();
        let mut signers: Vec<Account> = Vec::new();
    
        let use_fee_discount_pubkey: Option<Pubkey>;
        if let Some(fee_discount_pubkey) = order_params.fee_discount_pubkey {
            use_fee_discount_pubkey = Some(fee_discount_pubkey);
        } else if fee_discount_pubkey == None && supports_srm_fee_discounts {
            use_fee_discount_pubkey = Some(
                find_best_fee_discount_key(connection, &owner_address, fee_discount_pubkey_cache_duration_ms).await?.pubkey
            );
        } else {
            use_fee_discount_pubkey = None;
        }
    
        let open_orders_address: Pubkey;
        if open_orders_accounts.is_empty() {
            let account: Account;
            if let Some(open_orders_account) = order_params.open_orders_account {
                account = open_orders_account;
            } else {
                account = OpenOrders::get_derived_oo_account_pubkey(&owner_address, &address, &program_id).await?;
            }
            transaction.add(
                OpenOrders::make_create_account_transaction(connection, &address, &owner_address, &account.public_key, &program_id, &account.seed)
                .await?
            );
            open_orders_address = account.public_key;
            open_orders_accounts_cache.get_mut(&owner_address.to_string()).unwrap().ts = 0;
        } else if let Some(open_orders_account) = order_params.open_orders_account {
            open_orders_address = open_orders_account.public_key;
        } else if let Some(open_orders_address_key) = order_params.open_orders_address_key {
            open_orders_address = open_orders_address_key;
        } else {
            open_orders_address = open_orders_accounts[0].address;
        }
    
        let mut wrapped_sol_account: Option<Account> = None;
        if payer == owner_address {
            if (side == "buy" && quote_mint_address == wrapped_sol_mint) || (side == "sell" && base_mint_address == wrapped_sol_mint) {
                wrapped_sol_account = Some(Account::new());
                let mut lamports;
                if side == "buy" {
                    lamports = ((price * size * 1.01) * LAMPORTS_PER_SOL) as u64;
                    if !open_orders_accounts.is_empty() {
                        lamports -= open_orders_accounts[0].quote_token_free as u64;
                    }
                } else {
                    lamports = (size * LAMPORTS_PER_SOL) as u64;
                    if !open_orders_accounts.is_empty() {
                        lamports -= open_orders_accounts[0].base_token_free as u64;
                    }
                }
                lamports = lamports.max(0) + 10_000_000;
                transaction.add(system_instruction::create_account(
                    &owner_address,
                    &wrapped_sol_account.public_key,
                    lamports,
                    165,
                    &token_program_id,
                ));
                transaction.add(initialize_account(&wrapped_sol_account.public_key, &wrapped_sol_mint, &owner_address));
                signers.push(wrapped_sol_account);
            } else {
                return Err(Box::from("Invalid payer account"));
            }
        }
    
        let place_order_instruction = make_place_order_instruction(connection, PlaceOrderInstructionParams {
            owner,
            payer: wrapped_sol_account.map_or(payer, |acc| acc.public_key),
            side,
            price,
            size,
            order_type,
            client_id,
            open_orders_address_key: open_orders_address,
            fee_discount_pubkey: use_fee_discount_pubkey,
            self_trade_behavior,
            max_ts,
            replace_if_exists,
        });
        transaction.add(place_order_instruction);
    
        if let Some(wrapped_sol_account) = wrapped_sol_account {
            transaction.add(close_account(&wrapped_sol_account.public_key, &owner_address));
        }
    
        Ok((transaction, signers, owner))
    }

    struct OrderParams {
        owner: Pubkey,
        payer: Pubkey,
        side: u8,
        price: u64,
        size: u64,
        order_type: String,
        client_id: Option<u64>,
        open_orders_address_key: Pubkey,
        open_orders_account: Option<Pubkey>,
        fee_discount_pubkey: Option<Pubkey>,
        self_trade_behavior: String,
        program_id: Pubkey,
        max_ts: Option<u64>,
        replace_if_exists: bool,
    }
    
    fn make_place_order_instruction<T: Pubkey + Account>(
        connection: Connection,
        params: OrderParams<T>,
    ) -> TransactionInstruction {
        let OrderParams {
            owner,
            payer,
            side,
            price,
            size,
            order_type,
            client_id,
            open_orders_address_key,
            open_orders_account,
            fee_discount_pubkey,
        } = params;
    
        let owner_address: Pubkey = owner.public_key.unwrap_or(owner);
    
        if base_size_number_to_lots(size) <= 0 {
            panic!("size too small");
        }
        if price_number_to_lots(price) <= 0 {
            panic!("invalid price");
        }
    
        if uses_request_queue {
            DexInstructions::new_order(NewOrderParams {
                market: self.address,
                request_queue: self.decoded.request_queue,
                base_vault: self.decoded.base_vault,
                quote_vault: self.decoded.quote_vault,
                open_orders: open_orders_account.map_or(open_orders_address_key, |account| account.public_key),
                owner: owner_address,
                payer,
                side,
                limit_price: price_number_to_lots(price),
                max_quantity: base_size_number_to_lots(size),
                order_type,
                client_id,
                program_id: self.program_id,
                // @ts-ignore
                fee_discount_pubkey: if supports_srm_fee_discounts {
                    fee_discount_pubkey
                } else {
                    None
                },
            });
        } else {
            self.make_new_order_v3_instruction(params);
        }
    }

    fn make_new_order_v3_instruction<T: Pubkey + Account>(
        params: OrderParams<T>,
    ) -> TransactionInstruction {
        let OrderParams {
            owner,
            payer,
            side,
            price,
            size,
            order_type,
            client_id,
            open_orders_address_key,
            open_orders_account,
            fee_discount_pubkey,
            self_trade_behavior,
            program_id,
            max_ts,
            replace_if_exists,
        } = params;
    
        let owner_address: Pubkey = owner.public_key.unwrap_or(owner);
    
        DexInstructions::new_order_v3(NewOrderV3Params {
            market: self.address,
            bids: self.decoded.bids,
            asks: self.decoded.asks,
            request_queue: self.decoded.request_queue,
            event_queue: self.decoded.event_queue,
            base_vault: self.decoded.base_vault,
            quote_vault: self.decoded.quote_vault,
            open_orders: open_orders_account.map_or(open_orders_address_key, |account| account.public_key),
            owner: owner_address,
            payer,
            side,
            limit_price: price_number_to_lots(price),
            max_base_quantity: base_size_number_to_lots(size),
            max_quote_quantity: self.decoded.quote_lot_size.into()
                * (base_size_number_to_lots(size) * price_number_to_lots(price)),
            order_type,
            client_id,
            program_id: program_id.unwrap_or_else(|| self.program_id),
            self_trade_behavior,
            fee_discount_pubkey: if supports_srm_fee_discounts {
                fee_discount_pubkey
            } else {
                None
            },
            max_ts,
            replace_if_exists,
        });
    }

    async fn make_send_take_transaction<'a, T: Pubkey + Account>(
        connection: Connection,
        params: SendTakeParams<T>,
        fee_discount_pubkey_cache_duration_ms: u64,
    ) -> Result<{ transaction: Transaction, signers: Vec<Account>, payer: Pubkey }, Box<dyn Error>> {
        let owner_address: Pubkey = owner.public_key.unwrap_or(owner);
        let mut transaction = Transaction::new();
        let mut signers: Vec<Account> = vec![];
    
        let vault_signer = Pubkey::create_program_address(
            &[
                self.address.to_bytes(),
                self.decoded.vault_signer_nonce.to_le_bytes(),
            ],
            &self.program_id,
        )?;
    
        let mut use_fee_discount_pubkey: Option<Pubkey> = None;
        if let Some(fee_discount_key) = fee_discount_pubkey {
            use_fee_discount_pubkey = Some(fee_discount_key);
        } else if fee_discount_pubkey.is_none() && self.supports_srm_fee_discounts {
            let best_fee_discount_key = self.find_best_fee_discount_key(
                &connection,
                &owner_address,
                fee_discount_pubkey_cache_duration_ms,
            ).await?;
            use_fee_discount_pubkey = Some(best_fee_discount_key.pubkey);
        }
    
        let send_take_instruction = self.make_send_take_instruction(SendTakeInstructionParams {
            owner,
            base_wallet,
            quote_wallet,
            vault_signer,
            side,
            price,
            max_base_size,
            max_quote_size,
            min_base_size,
            min_quote_size,
            limit,
            program_id,
            fee_discount_pubkey: use_fee_discount_pubkey,
        });
    
        transaction.add_instruction(&send_take_instruction);
    
        Ok({ transaction, signers, payer: owner })
    }

    struct SendTakeParams<T> {
        owner: T,
        base_wallet: Pubkey,
        quote_wallet: Pubkey,
        vault_signer: Pubkey,
        side: u8,
        price: u64,
        max_base_size: u64,
        max_quote_size: u64,
        min_base_size: u64,
        min_quote_size: u64,
        limit: u16,
        program_id: Option<Pubkey>,
        fee_discount_pubkey: Option<Pubkey>,
    }
    
    fn make_send_take_instruction<T: Pubkey + Account>(
        params: SendTakeParams<T>,
    ) -> Instruction {
        let SendTakeParams {
            owner,
            base_wallet,
            quote_wallet,
            vault_signer,
            side,
            price,
            max_base_size,
            max_quote_size,
            min_base_size,
            min_quote_size,
            limit,
            program_id,
            fee_discount_pubkey,
        } = params;
    
        let owner_address: Pubkey = owner.public_key.unwrap_or(owner);
    
        if base_size_number_to_lots(max_base_size) <= 0 {
            panic!("size too small");
        }
        if quote_size_number_to_spl_size(max_quote_size) <= 0 {
            panic!("size too small");
        }
        if price_number_to_lots(price) <= 0 {
            panic!("invalid price");
        }
    
        DexInstructions::send_take(SendTakeInstructionParams {
            market: self.address,
            request_queue: self.decoded.request_queue,
            event_queue: self.decoded.event_queue,
            bids: self.decoded.bids,
            asks: self.decoded.asks,
            base_wallet,
            quote_wallet,
            owner: owner_address,
            base_vault: self.decoded.base_vault,
            quote_vault: self.decoded.quote_vault,
            vault_signer,
            side,
            limit_price: price_number_to_lots(price),
            max_base_quantity: base_size_number_to_lots(max_base_size),
            max_quote_quantity: quote_size_number_to_spl_size(max_quote_size),
            min_base_quantity: base_size_number_to_lots(min_base_size),
            min_quote_quantity: quote_size_number_to_spl_size(min_quote_size),
            limit,
            program_id: program_id.unwrap_or_else(|| self.program_id),
            fee_discount_pubkey: if supports_srm_fee_discounts {
                fee_discount_pubkey
            } else {
                None
            },
        })
    }

    struct OrderParamsAccounts<T> {
        owner: T,
        open_orders_account: Option<Pubkey>,
        open_orders_address_key: Pubkey,
        payer: Pubkey,
        program_id: Option<Pubkey>,
        fee_discount_pubkey: Option<Pubkey>,
    }
    
    #[derive(Debug)]
    struct OrderParamsBase<T> {
        side: u8,
        price: u64,
        size: u64,
        order_type: String,
        client_id: Option<u64>,
        self_trade_behavior: String,
        max_ts: Option<u64>,
    }
    
    fn make_replace_orders_by_client_ids_instruction<T: Pubkey + Account>(
        accounts: OrderParamsAccounts<T>,
        orders: Vec<OrderParamsBase<T>>,
    ) -> Instruction {
        let owner_address: Pubkey = accounts.owner.public_key.unwrap_or(accounts.owner);
    
        DexInstructions::replace_orders_by_client_ids(ReplaceOrdersByClientIdsParams {
            market: self.address,
            bids: self.decoded.bids,
            asks: self.decoded.asks,
            request_queue: self.decoded.request_queue,
            event_queue: self.decoded.event_queue,
            base_vault: self.decoded.base_vault,
            quote_vault: self.decoded.quote_vault,
            open_orders: accounts.open_orders_account.unwrap_or(accounts.open_orders_address_key),
            owner: owner_address,
            payer: accounts.payer,
            program_id: accounts.program_id.unwrap_or_else(|| self.program_id),
            fee_discount_pubkey: if supports_srm_fee_discounts {
                accounts.fee_discount_pubkey
            } else {
                None
            },
            orders: orders.iter().map(|order| {
                ReplaceOrder {
                    side: order.side,
                    limit_price: price_number_to_lots(order.price),
                    max_base_quantity: base_size_number_to_lots(order.size),
                    max_quote_quantity: self.decoded.quote_lot_size.into()
                        * (base_size_number_to_lots(order.size) * price_number_to_lots(order.price)),
                    order_type: order.order_type,
                    client_id: order.client_id,
                    program_id: accounts.program_id.unwrap_or_else(|| self.program_id),
                    self_trade_behavior: order.self_trade_behavior,
                   max_ts: order.max_ts,
                }
            }).collect(),
        });
    }

    async fn _send_transaction(
        connection: &Connection,
        transaction: Transaction,
        signers: Vec<Account>,
        skip_preflight: bool,
        commitment: Option<Commitment>,
    ) -> Result<TransactionSignature, Box<dyn Error>> {
        let signature = connection.send_transaction(&transaction, &signers, skip_preflight).await?;
    
        match connection.confirm_transaction(&signature, commitment).await {
            Ok(Some(value)) => {
                if let Some(err) = value.err {
                    return Err(format!("Error: {:?}", err).into());
                }
                Ok(signature)
            },
            _ => Err("Transaction confirmation failed".into()),
        }
    }

    #[allow(dead_code)]
    async fn cancel_order_by_client_id(
        connection: &Connection,
        owner: Account,
        open_orders: Pubkey,
        client_id: u64,
    ) -> Result<TransactionSignature, Box<dyn Error>> {
        let transaction = make_cancel_order_by_client_id_transaction(connection, owner.public_key, open_orders, client_id).await?;
        send_transaction(connection, transaction, vec![owner]).await
    }

    #[allow(dead_code)]
    async fn cancel_orders_by_client_ids(
        connection: &Connection,
        owner: Account,
        open_orders: Pubkey,
        client_ids: Vec<u64>,
    ) -> Result<TransactionSignature, Box<dyn Error>> {
        let transaction = make_cancel_orders_by_client_ids_transaction(connection, owner.public_key, open_orders, client_ids).await?;
        send_transaction(connection, transaction, vec![owner]).await
    }

    #[allow(dead_code)]
    async fn make_cancel_order_by_client_id_transaction(
        connection: Connection,
        owner: Pubkey,
        open_orders: Pubkey,
        client_id: u64,
    ) -> Result<Transaction, Box<dyn Error>> {
        let mut transaction = Transaction::new();

        if uses_request_queue {
            let instruction = DexInstructions::cancel_order_by_client_id(CancelOrderByClientIdParams {
                market: self.address,
                owner,
                open_orders,
                request_queue: self.decoded.request_queue,
                client_id,
                program_id: self.program_id,
            });
            transaction.add(instruction);
        } else {
            let instruction = DexInstructions::cancel_order_by_client_id_v2(CancelOrderByClientIdV2Params {
                market: self.address,
                open_orders,
                owner,
                bids: self.decoded.bids,
                asks: self.decoded.asks,
                event_queue: self.decoded.event_queue,
                client_id,
                program_id: self.program_id,
            });
            transaction.add(instruction);
        }

        Ok(transaction)
    }

    #[allow(dead_code)]
    async fn make_cancel_orders_by_client_ids_transaction(
        connection: Connection,
        owner: Pubkey,
        open_orders: Pubkey,
        client_ids: Vec<u64>,
    ) -> Result<Transaction, Box<dyn Error>> {
        let mut transaction = Transaction::new();

        let instruction = DexInstructions::cancel_orders_by_client_ids(CancelOrdersByClientIdsParams {
            market: self.address,
            open_orders,
            owner,
            bids: self.decoded.bids,
            asks: self.decoded.asks,
            event_queue: self.decoded.event_queue,
            client_ids,
            program_id: self.program_id,
        });
        transaction.add(instruction);

        Ok(transaction)
    }

    async fn cancel_order(connection: Connection, owner: Account, order: Order) -> Result<(), Box<dyn Error>> {
        let transaction = make_cancel_order_transaction(connection, owner.key, order).await?;
        send_transaction(connection, transaction, vec![owner]).await?;
        Ok(())
    }
    
    async fn make_cancel_order_transaction(connection: Connection, owner: Pubkey, order: Order) -> Result<Transaction, Box<dyn Error>> {
        let mut transaction = Transaction::new();
        let cancel_order_instruction = make_cancel_order_instruction(connection, owner, order);
        transaction.add_instruction(&cancel_order_instruction);
        Ok(transaction)
    }

    fn make_cancel_order_instruction(connection: Connection, owner: Pubkey, order: Order) -> Instruction {
        if uses_request_queue {
            DexInstructions::cancel_order(CancelOrderParams {
                market: self.address,
                owner,
                open_orders: order.open_orders_address,
                request_queue: self.decoded.request_queue,
                side: order.side,
                order_id: order.order_id,
                open_orders_slot: order.open_orders_slot,
                program_id: self.program_id,
            })
        } else {
            DexInstructions::cancel_order_v2(CancelOrderV2Params {
                market: self.address,
                owner,
                open_orders: order.open_orders_address,
                bids: self.decoded.bids,
                asks: self.decoded.asks,
                event_queue: self.decoded.event_queue,
                side: order.side,
                order_id: order.order_id,
                open_orders_slot: order.open_orders_slot,
                program_id: self.program_id,
            })
        }
    }

    fn make_consume_events_instruction(open_orders_accounts: Vec<Pubkey>, limit: u64) -> Instruction {
        DexInstructions::consume_events(ConsumeEventsParams {
            market: self.address,
            event_queue: self.decoded.event_queue,
            coin_fee: self.decoded.event_queue,
            pc_fee: self.decoded.event_queue,
            open_orders_accounts,
            limit,
            program_id: self.program_id,
        })
    }
    
    fn make_consume_events_permissioned_instruction(open_orders_accounts: Vec<Pubkey>, limit: u64) -> Instruction {
        DexInstructions::consume_events_permissioned(ConsumeEventsPermissionedParams {
            market: self.address,
            event_queue: self.decoded.event_queue,
            crank_authority: self.decoded.consume_events_authority,
            open_orders_accounts,
            limit,
            program_id: self.program_id,
        })
    }

    #[allow(dead_code)]
    async fn settle_funds(
        connection: Connection,
        owner: Account,
        open_orders: OpenOrders,
        base_wallet: Pubkey,
        quote_wallet: Pubkey,
        referrer_quote_wallet: Option<Pubkey>,
    ) -> Result<TransactionSignature, Box<dyn Error>> {
        if open_orders.owner != owner.public_key {
            return Err("Invalid open orders account".into());
        }

        if let Some(referrer_quote_wallet) = referrer_quote_wallet {
            if !self.supports_referral_fees {
                return Err("This program ID does not support referrerQuoteWallet".into());
            }
        }

        let SettleFundsResult { transaction, signers } = make_settle_funds_transaction(
            connection,
            open_orders,
            base_wallet,
            quote_wallet,
            referrer_quote_wallet,
        ).await?;

        let mut transaction_signers = vec![owner];
        transaction_signers.extend(signers);

        send_transaction(connection, transaction, transaction_signers).await
    }

    #[allow(dead_code)]
    async fn make_settle_funds_transaction(
        connection: Connection,
        open_orders: OpenOrders,
        base_wallet: Pubkey,
        quote_wallet: Pubkey,
        referrer_quote_wallet: Option<Pubkey>,
    ) -> Result<{ transaction: Transaction, signers: Vec<Account>, payer: Pubkey }, Box<dyn Error>> {
        let vault_signer = Pubkey::create_program_address(
            &[
                self.address.to_bytes(),
                self.decoded.vault_signer_nonce.to_le_bytes(),
            ],
            &self.program_id,
        )?;

        let mut transaction = Transaction::new();
        let mut signers: Vec<Account> = vec![];

        let mut wrapped_sol_account: Option<Account> = None;
        if (self.base_mint_address == WRAPPED_SOL_MINT && base_wallet == open_orders.owner)
            || (self.quote_mint_address == WRAPPED_SOL_MINT && quote_wallet == open_orders.owner)
        {
            wrapped_sol_account = Some(Account::new());
            transaction.add_instruction(
                SystemProgram::create_account(
                    &open_orders.owner,
                    &wrapped_sol_account.public_key,
                    connection.get_minimum_balance_for_rent_exemption(165).await?,
                    165,
                    &TOKEN_PROGRAM_ID,
                ),
            );

            transaction.add_instruction(
                initialize_account(
                    &wrapped_sol_account.public_key,
                    &WRAPPED_SOL_MINT,
                    &open_orders.owner,
                ),
            );

            signers.push(wrapped_sol_account);
        }

        transaction.add_instruction(
            DexInstructions::settle_funds(SettleFundsParams {
                market: self.address,
                open_orders: open_orders.address,
                owner: open_orders.owner,
                base_vault: self.decoded.base_vault,
                quote_vault: self.decoded.quote_vault,
                base_wallet: if base_wallet == open_orders.owner && wrapped_sol_account.is_some() {
                    wrapped_sol_account.public_key
                } else {
                    base_wallet
                },
                quote_wallet: if quote_wallet == open_orders.owner && wrapped_sol_account.is_some() {
                    wrapped_sol_account.public_key
                } else {
                    quote_wallet
                },
                vault_signer,
                program_id: self.program_id,
                referrer_quote_wallet,
            }),
        );

        if let Some(wrapped_sol_account) = wrapped_sol_account {
            transaction.add_instruction(
                close_account(
                    &wrapped_sol_account.public_key,
                    &open_orders.owner,
                    &open_orders.owner,
                ),
            );
        }

        Ok({ transaction, signers, payer: open_orders.owner })
    }


    #[allow(dead_code)]
    async fn match_orders(connection: Connection, fee_payer: Account, limit: u64) -> Result<TransactionSignature, Box<dyn Error>> {
        let tx = make_match_orders_transaction(limit);
        _send_transaction(connection, tx, vec![fee_payer]).await
    }

    fn make_match_orders_transaction(limit: u64) -> Transaction {
        let mut tx = Transaction::new();
        tx.add(DexInstructions::match_orders(MatchOrdersParams {
            market: self.address,
            request_queue: self.decoded.request_queue,
            event_queue: self.decoded.event_queue,
            bids: self.decoded.bids,
            asks: self.decoded.asks,
            base_vault: self.decoded.base_vault,
            quote_vault: self.decoded.quote_vault,
            limit,
            program_id: self.program_id,
        }));
        tx
    }

    async fn load_request_queue(connection: Connection) -> Result<RequestQueueData, Box<dyn Error>> {
        let account_info = connection.get_account_info(self.decoded.request_queue).await?;
        let data = account_info.data.ok_or("Failed to get account data")?;
        decode_request_queue(data)
    }

    async fn load_event_queue(connection: Connection) -> Result<EventQueueData, Box<dyn Error>> {
        let account_info = connection.get_account_info(self.decoded.event_queue).await?;
        let data = account_info.data.ok_or("Failed to get account data")?;
        decode_event_queue(data)
    }

    async fn load_fills(connection: Connection, limit: u64) -> Result<Vec<FillEvent>, Box<dyn Error>> {
        let account_info = connection.get_account_info(self.decoded.event_queue).await?;
        let data = account_info.data.ok_or("Failed to get account data")?;
        let events = decode_event_queue(data, limit);

        Ok(events.iter()
            .filter(|event| event.event_flags.fill && event.native_quantity_paid > 0)
            .map(|event| self.parse_fill_event(event))
            .collect())
    }

    fn parse_fill_event(event: Event) -> FillEvent {
        let (mut size, mut price, mut side, mut price_before_fees): (f64, f64, String, u64);
    
        if event.event_flags.bid {
            side = String::from("buy");
            
            price_before_fees = if event.event_flags.maker {
                event.native_quantity_paid + event.native_fee_or_rebate
            } else {
                event.native_quantity_paid - event.native_fee_or_rebate
            };
    
            price = divide_bn_to_number(price_before_fees * self.base_spl_token_multiplier,
                                        self.quote_spl_token_multiplier * event.native_quantity_released);
    
            size = divide_bn_to_number(event.native_quantity_released, self.base_spl_token_multiplier);
    
        } else {
            side = String::from("sell");
    
            price_before_fees = if event.event_flags.maker {
                event.native_quantity_released - event.native_fee_or_rebate
            } else {
                event.native_quantity_released + event.native_fee_or_rebate
            };
    
            price = divide_bn_to_number(price_before_fees * self.base_spl_token_multiplier,
                                        self.quote_spl_token_multiplier * event.native_quantity_paid);
    
            size = divide_bn_to_number(event.native_quantity_paid, self.base_spl_token_multiplier);
        }
    
        FillEvent {
            event: event,
            side: side,
            price: price,
            fee_cost: self.quote_spl_size_to_number(event.native_fee_or_rebate) * 
                      if event.event_flags.maker { -1 } else { 1 },
            size: size,
        }
    }

    fn base_spl_token_multiplier(&self) -> BN {
        BN::from(10).pow(self.base_spl_token_decimals());
      }
      
      fn quote_spl_token_multiplier(&self) -> BN {
        BN::from(10).pow(self.quote_spl_token_decimals());
      }
      
      fn price_lots_to_number(&self, price: &BN) -> f64 {
        divide_bn_to_number(
          &price.mul(&self.decoded.quote_lot_size).mul(&self.base_spl_token_multiplier()),
          &self.decoded.base_lot_size.mul(&self.quote_spl_token_multiplier()),
        )
      }
      
      fn price_number_to_lots(&self, price: f64) -> BN {
        BN::from(
          ((price * 10_f64.powi(self.quote_spl_token_decimals())).mul(self.decoded.base_lot_size.to_f64()))
            / (10_f64.powi(self.base_spl_token_decimals()) * self.decoded.quote_lot_size.to_f64()).round(),
        )
      }
      
      fn base_spl_size_to_number(&self, size: &BN) -> f64 {
        divide_bn_to_number(&size, &self.base_spl_token_multiplier())
      }
      
      fn quote_spl_size_to_number(&self, size: &BN) -> f64 {
        divide_bn_to_number(&size, &self.quote_spl_token_multiplier())
      }
      
      fn base_size_number_to_spl_size(&self, size: f64) -> BN {
        BN::from((size * 10_f64.powi(self.base_spl_token_decimals())).round())
      }
      
      fn quote_size_number_to_spl_size(&self, size: f64) -> BN {
        BN::from((size * 10_f64.powi(self.quote_spl_token_decimals())).round())
      }
      
      fn base_size_lots_to_number(&self, size: &BN) -> f64 {
        divide_bn_to_number(
          &size.mul(&self.decoded.base_lot_size),
          &self.base_spl_token_multiplier(),
        )
      }
      
      fn base_size_number_to_lots(&self, size: f64) -> BN {
        let native = BN::from((size * 10_f64.powi(self.base_spl_token_decimals())).round());
        native.div(&self.decoded.base_lot_size)
      }
      
      fn quote_size_lots_to_number(&self, size: &BN) -> f64 {
        divide_bn_to_number(
          &size.mul(&self.decoded.quote_lot_size),
          &self.quote_spl_token_multiplier(),
        )
      }
      
      fn quote_size_number_to_lots(&self, size: f64) -> BN {
        let native = BN::from((size * 10_f64.powi(self.quote_spl_token_decimals())).round());
        native.div(&self.decoded.quote_lot_size)
      }
      
      fn min_order_size(&self) -> f64 {
        self.base_size_lots_to_number(&BN::from(1))
      }
      
      fn tick_size(&self) -> f64 {
        self.price_lots_to_number(&BN::from(1))
      }
}

pub struct Memcmp {
    offset: u64,
    bytes: Vec<u8>,
}

lazy_static::lazy_static! {
    static ref PROGRAM_LAYOUT_VERSIONS: HashMap<&'static str, u32> = {
        let mut map = HashMap::new();
        map.insert("4ckmDgGdxQoPDLUkDT3vHgSAkzA3QRdNq5ywwY4sUSJn", 1);
        map.insert("BJ3jrUzddfuSrZHXSCxMUUQsjKEyLmuuyZebkcaFp2fg", 1);
        map.insert("EUqojwWA2rd19FZrzeBncJsm38Jm1hEhE3zsmX3bRc2o", 2);
        map.insert("srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX", 3);
        map
    };
}

pub fn get_layout_version(program_id: PublicKey) -> u32 {
    *PROGRAM_LAYOUT_VERSIONS.get(program_id.0).unwrap_or(&3)
}

pub async fn get_filtered_program_accounts(
    rpc_client: &RpcClient,
    program_id: Pubkey,
    filters: Vec<(String, String)>,
) -> Result<Vec<(Pubkey, Account)>, Box<dyn Error>> {
    let resp = rpc_client.get_program_accounts(
        &program_id.to_string(),
        Some(spl_token::rpc::TokenAccountsFilter::default()),
    )?;

    if let Some(error) = resp.error {
        return Err(error.into());
    }

    let accounts = resp
        .result
        .iter()
        .map(|(pubkey, account)|
            Ok((
                Pubkey::from_str(pubkey).unwrap(),
                Account {
                    lamports: account.lamports,
                    owner: Pubkey::from_str(&account.owner).unwrap(),
                    data: base64::decode(&account.data[0]).unwrap(),
                    executable: account.executable,
                    rent_epoch: account.rent_epoch,
                },
            ))
        )
        .collect::<Result<Vec<(Pubkey, Account)>, Box<dyn Error>>>()?;

    Ok(accounts)
}



pub static _MARKET_STAT_LAYOUT_V1: [u8; 0] = [];
pub static MARKET_STATE_LAYOUT_V2: [u8; 0] = [];

pub trait LayoutExt {
    fn offset_of(field_name: &str) -> usize;
}

impl LayoutExt for [u8] {
    fn offset_of(_field_name: &str) -> usize {

    }
}

