use anyhow::Result;
use clap::Parser;

use artemis_core::engine::Engine;
use artemis_core::types::{CollectorMap, ExecutorMap};
use collectors::time_collector::TimeCollector;
use alloy::{
    network::{EthereumWallet, Ethereum},
    signers::local::PrivateKeySigner,
    providers::ProviderBuilder, 
};

use executors::protect_executor::ProtectExecutor;
use std::sync::Arc;
use strategies::{
    mm_strategy::{MmStrategy, Deployment},
    types::{Action, Config, Event},
};
use tracing::{info, Level};
use tracing_subscriber::{filter, prelude::*};

pub mod collectors;
pub mod executors;
pub mod strategies;

//static POLL_INTERVAL_SECS: u64 = 1 * 10;
//pub const CHAIN_ID: u64 = 31337;

/// CLI Options.
#[derive(Parser, Debug)]
pub struct Args {
    /// Ethereum node WS endpoint.
    #[arg(long)]
    pub rpc: String,

    /// Private key for sending txs.
    #[arg(long)]
    pub private_key: String,

    /// Percentage of profit to pay in gas.
    #[arg(long)]
    pub bid_percentage: u64,

    #[arg(long)]
    pub deployment: Deployment,

    #[arg(long)]
    pub total_profit: u128,    

    // #[arg(long)]
    // pub liquidator_address: String,

    #[arg(long)]
    pub chain_id: u64,

    #[arg(long)]
    pub last_block_number: u64,

    #[arg(long, default_value_t = 10)]
    pub pool_interval_secs: u64,

    #[arg(long, default_value_t = 60*60*24*2)]
    pub update_all_pools_secs: u64,

    #[arg(long, default_value_t = 60*60*24*7)]
    pub activity_level_clean_secs: u64,

    // #[arg(long, default_value_t = 100)]
    // pub activity_level_init: u64,

    #[arg(long, default_value_t = 60*60*24)]
    pub calc_all_positions_secs: u64,

    // #[arg(long, default_value_t = 130)]
    // pub monitor_margin_level_thresold: u128,
}



#[tokio::main]
async fn main() -> Result<()> {
    // Set up tracing and parse args.
    let filter = filter::Targets::new()
        .with_target("artemis_core", Level::INFO)
        .with_target("mm_liquidator", Level::INFO);

    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(filter)
        .init();

    let args = Args::parse();
    println!("{:?}", args);

    let chain_id: u64 = args.chain_id;

    // Set up alloy provider.
    let signer: PrivateKeySigner = args.private_key.parse().expect("should parse private key");
    let wallet = EthereumWallet::from(signer.clone());
    let liquidator = signer.address();

    let rpc = (&args.rpc).parse()?;
    let provider = ProviderBuilder::new().with_cached_nonce_management().wallet(wallet.clone()).on_http(rpc);
    //let provider = ProviderBuilder::new().wallet(wallet.clone()).on_http(rpc);

    // // Set up engine.
    let mut engine: Engine<Event, Action<Ethereum>> = Engine::default();

    // // Set up time collector.
    let time_collector = Box::new(TimeCollector::new(args.pool_interval_secs));
    let time_collector = CollectorMap::new(time_collector, Event::NewTick);
    engine.add_collector(Box::new(time_collector));

    let config = Config {
        chain_id: chain_id,
    };

    let strategy = MmStrategy::new(
        Arc::new(provider.clone()),
        config,
        args.deployment,
        liquidator,
        args.last_block_number,
        args.total_profit,
        args.pool_interval_secs,
        args.update_all_pools_secs,
        args.activity_level_clean_secs,
        args.calc_all_positions_secs,
    );
    engine.add_strategy(Box::new(strategy));

    let executor = Box::new(
        ProtectExecutor::new(
            Arc::new(provider.clone()), 
            Arc::new(provider.clone())
        )
    );

    let executor = ExecutorMap::new(executor, |action| match action {
        Action::SubmitTx(tx) => Some(tx),
    });

    engine.add_executor(Box::new(executor));
    // Start engine.
    if let Ok(mut set) = engine.run().await {
        while let Some(res) = set.join_next().await {
            info!("res: {:?}", res);
        }
    }
    Ok(())
}
