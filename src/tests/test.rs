use openbook::{
    market,
    order,
};
use std::collections::HashMap;
use connection::Connection;
use std::error::Error;
use tokio;

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

    let rpc_url = "https://testnet.solana.com".to_string();
    let rpc_client = RpcClient::new(rpc_url);

    let market_address_str = "..."; 
    let program_address_str = "..."; 
    let market_address = Pubkey::from_str(market_address_str)
                                .expect("Failed to parse market address");
    let program_address = Pubkey::from_str(program_address_str)
                                .expect("Failed to parse program address");

    let market = Market::load(&rpc_client, &market_address, &program_address)
                        .await.expect("Failed to load market");
    
    let bids = market.load_bids(&rpc_client)
                    .await.expect("Failed to load bids");
    let asks = market.load_asks(&rpc_client)
                    .await.expect("Failed to load asks");

    for order in asks {
        println!(
            "{} {} {} {}", 
            order.order_id, 
            order.price, 
            order.size, 
            order.side
        );
    }


