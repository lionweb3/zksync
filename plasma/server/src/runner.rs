use std::sync::mpsc::{channel};

use super::prover::{BabyProver, start_prover};
use super::state_keeper::{PlasmaStateKeeper, start_state_keeper};
use super::rest_api::start_api_server;
use super::committer;
use super::eth_watch::{EthWatch, start_eth_watch};
use super::storage::{ConnectionPool};
use super::models::StateKeeperRequest;

pub fn run() {

    // create channel to accept deserialized requests for new transacitons

    let (tx_for_state, rx_for_state) = channel();
    let (tx_for_proof_requests, rx_for_proof_requests) = channel();
    let (tx_for_ops, rx_for_ops) = channel();

    let connection_pool = ConnectionPool::new();
    let state_keeper = PlasmaStateKeeper::new(connection_pool.clone());
    let prover = BabyProver::create(connection_pool.clone()).unwrap();
    let eth_watch = EthWatch::new(0, 0, connection_pool.clone());

    // spawn threads for different processes
    // see https://docs.google.com/drawings/d/16UeYq7cuZnpkyMWGrgDAbmlaGviN2baY1w1y745Me70/edit?usp=sharing

    println!("starting actors");

    // Simple timer, pings every 15 seconds
    let tx_for_state_ticker = tx_for_state.clone();
    std::thread::Builder::new().name("timer".to_string()).spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_secs(15));
            tx_for_state_ticker.send(StateKeeperRequest::TimerTick);
        }
    });

    start_api_server(tx_for_state.clone(), connection_pool.clone());
    start_eth_watch(eth_watch, tx_for_state.clone());
    
    start_state_keeper(state_keeper, rx_for_state, tx_for_ops.clone());
    start_prover(prover, rx_for_proof_requests, tx_for_ops);

    let tx_for_eth = committer::start_eth_sender(connection_pool.clone());
    committer::run_committer(rx_for_ops, tx_for_eth, tx_for_proof_requests, connection_pool.clone());
}