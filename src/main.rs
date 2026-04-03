mod filter_store;
mod utxo;

use crate::filter_store::FilterStore;
use crate::utxo::{BlockInfo, TxoSet};

use bip157_store::chain::ChainState;
use bip157_store::messages::Event;
use bip157_store::{chain::checkpoints::HeaderCheckpoint, BuilderWithStore, Client};
use bip157_store::{Address, BlockHash, Network};
use bitcoin::params::Params;
use bitcoin::{OutPoint, ScriptBuf};
use std::collections::HashSet;
use std::str::FromStr;

const NETWORK: Network = Network::Bitcoin; // or Signet
const TEST_ADDRS: [&str; 2] = [
    "bc1qmuu3qywg3uedj77cq46kwzq52ykkm89gycg8t2", // randomly picked, 939347, 5 txs
    // "bc1qp4tt6v5cg62fzd9hgxal8tqerh0czwnxl320v7", // randomly picked, 790+ txs
    "bc1qj7rdcjzawk0ssltu2vymqm5j9n0lxznt8fh8t3", // randomly picked, 922970
];

fn script_addr_info(script: &ScriptBuf) -> String {
    match Address::from_script(script, Params::BITCOIN) {
        Ok(addr) => addr.to_string(),
        Err(_) => script.to_string(),
    }
}

#[tokio::main]
async fn main() {
    let mut utxo_set = TxoSet::new();

    // Add third-party logging
    let subscriber = tracing_subscriber::FmtSubscriber::new();
    tracing::subscriber::set_global_default(subscriber).unwrap();

    // Use a predefined checkpoint
    // let checkpoint =
    //     HeaderCheckpoint::new(RECOVERY_HEIGHT, BlockHash::from_str(RECOVERY_HASH).unwrap());
    // let anchor_mainnet_640k = HeaderCheckpoint::new(
    //     640_000,
    //     BlockHash::from_str("0000000000000000000b3021a283b981dd08f4ccf318b684b214f995d102af43")
    //         .unwrap(),
    // );
    // let anchor_mainnet_710k = HeaderCheckpoint::new(
    //     710_000,
    //     BlockHash::from_str("00000000000000000007822e1ddba0bed6a55f0072aa1584c70a2f81c275f587")
    //         .unwrap(),
    // );
    let anchor_mainnet_920k = HeaderCheckpoint::new(
        920_000,
        BlockHash::from_str("000000000000000000005e9de5d9e008d923ced659896d9ad012e347882bdc87")
            .unwrap(),
    );
    // let anchor_mainnet_940k = HeaderCheckpoint::new(
    //     940_000,
    //     BlockHash::from_str("000000000000000000002afe1e2f7e176047529419532b2a6773c45623a02c12")
    //         .unwrap(),
    // );

    // Bitcoin scripts to scan the blockchain for
    let mut scripts = HashSet::new();
    for addr in &TEST_ADDRS {
        let script = Address::from_str(addr)
            .unwrap()
            .require_network(NETWORK)
            .unwrap()
            .script_pubkey();
        println!("  Address {} ({})", addr, script_addr_info(&script));
        scripts.insert(script);
    }

    // Create a new node builder
    // TODO: Switch to creating filter instance outside (also affects kyoto-store)
    // let builder = Builder::new(NETWORK);
    let builder = BuilderWithStore::<FilterStore>::new(NETWORK);
    // Add node preferences and build the node/client
    let (node, client) = builder
        // Only scan blocks strictly after a checkpoint
        .chain_state(ChainState::Checkpoint(anchor_mainnet_920k))
        // .after_checkpoint(anchor_mainnet_920k)
        // The number of connections we would like to maintain
        .required_peers(1)
        // Create the node and client
        // .build_with_store::<FilterStore>();
        .build();

    tokio::task::spawn(async move { node.run().await });

    let Client {
        requester,
        mut info_rx,
        mut warn_rx,
        mut event_rx,
    } = client;

    // Continually listen for events until the node is synced to its peers.
    loop {
        tokio::select! {
            info = info_rx.recv() => {
                if let Some(info) = info {
                    tracing::info!("i {info}");
                }
            }
            warn = warn_rx.recv() => {
                if let Some(warn) = warn {
                    tracing::warn!("w {warn}");
                }
            }
            event = event_rx.recv() => {
                if let Some(event) = event {
                    match event {
                        Event::IndexedFilter(filter) => {
                            // let height = filter.height();
                            // tracing::info!("Got filter: {height}");

                            // println!("Got filter {} {}, size {}", height, filter.block_hash(), filter.clone().into_contents().len());
                            if filter.contains_any(scripts.iter()) {
                                let hash = filter.block_hash();
                                for script in &scripts {
                                    if filter.contains_any(vec![script.clone()].iter()) {
                                        tracing::info!("FOUND script in block {}, {} !", filter.height(), script_addr_info(script));
                                    }
                                }

                                let indexed_block = requester.get_block(hash).await.unwrap();
                                tracing::info!("Got block {} {}, with {} txs", filter.height(), indexed_block.height, indexed_block.block.txdata.len());
                                let block_info = BlockInfo::new(filter.height(), hash);
                                // let coinbase = indexed_block.block.txdata.first().unwrap().compute_txid();
                                // tracing::info!("Coinbase transaction ID: {}", coinbase);
                                for tx in &indexed_block.block.txdata {
                                    // Check inputs: script is not known, only txid. Nonetheless, we try to set the Txo as spent.
                                    // The following cases are possible:
                                    // - input is from an unrelated script -> we don't have it, it will be ignored.
                                    // - input is from a watched script, but TXO is not known -> it will be ignored
                                    // - input is from a watched script, and TXO is known -> it will be set as spent
                                    for inpidx in 0..tx.input.len() {
                                        let inp = &tx.input[inpidx];
                                        if let Some((utxo, changed)) = utxo_set.set_spent(inp.previous_output, tx.compute_txid(), inpidx as u32, block_info.clone()) {
                                            if changed {
                                                tracing::info!("TXO set Spent: {}", utxo);
                                            }
                                        }
                                    }
                                    // Check outputs, new UTXO
                                    for outidx in 0..tx.output.len() {
                                        let out = &tx.output[outidx];
                                        for script in &scripts {
                                            if out.script_pubkey == *script {
                                                let txo = utxo_set.add(script.clone(), block_info.clone(), OutPoint::new(tx.compute_txid(), outidx as u32), out.value);
                                                tracing::info!("Output match, TXO added: {}", txo);
                                            }
                                        }
                                    }
                                }
                                // break;
                            }
                        }
                        // Event::Block(block) => {
                        //     println!("Block: {}", block.height);
                        // }
                        Event::FiltersSynced(_update) => {
                            println!("FiltersSynced!");
                            utxo_set.print(false);
                        }
                        _ => (),
                    }
                }
            }
        }
    }
    // let _ = requester.shutdown();
    // tracing::info!("Shutting down");
}
