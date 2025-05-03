// Copyright(C) Facebook, Inc. and its affiliates.

use anyhow::{Context, Result};
use clap::{crate_name, crate_version, App, AppSettings, ArgMatches, SubCommand};
use model::file_io::Export;
use model::file_io::Import;
use config::{Committee, KeyPair, Parameters};
#[cfg(not(feature = "dolphin"))]
use consensus::Tusk;
use drb_coordinator::coordinator::Coordinator;
use env_logger::Env;
use model::types_and_const::{WorkerId, BEACON_PER_EPOCH, CHANNEL_CAPACITY, MAX_EPOCH};
#[cfg(feature = "pq")]
use model::types_and_const::MAX_INDEX;
use primary::{Certificate, Primary};
// use std::sync::Arc;
use store::Store;
use tokio::sync::mpsc::{channel, Receiver, Sender};
#[cfg(feature = "pq")]
use tokio::time::sleep;
#[cfg(feature = "pq")]
use std::time::Duration;
// use tokio::sync::RwLock;
use bavss::Breeze;
use secondary_bft::init_bft::InitBFT;
#[cfg(feature = "dolphin")]
use consensus::Dolphin;
use drb_coordinator::beacon_hub::BeaconHub;
use model::breeze_universal::{BreezeCertificate, CommonReferenceString};
use worker::Worker;


#[tokio::main]
async fn main() -> Result<()> {
    let matches = App::new(crate_name!())
        .version(crate_version!())
        .about("A research implementation of Narwhal and Tusk.")
        .args_from_usage("-v... 'Sets the level of verbosity'")
        .subcommand(
            SubCommand::with_name("generate_keys")
                .about("Print a fresh key pair to file")
                .args_from_usage("--filename=<FILE> 'The file where to print the new key pair'"),
        )
        .subcommand(
            SubCommand::with_name("run")
                .about("Run a node")
                .args_from_usage("--keys=<FILE> 'The file containing the node keys'")
                .args_from_usage("--committee=<FILE> 'The file containing committee information'")
                .args_from_usage("--parameters=[FILE] 'The file containing the node parameters'")
                .args_from_usage("--store=<PATH> 'The path where to create the data store'")
                .subcommand(
                    SubCommand::with_name("primary")
                        .about("Run a single primary")
                        .args_from_usage("--crs=<FILE> 'The common reference string of breeze'")
                        .args_from_usage("--bs=<FILE> 'The avss_batch_size configuration'")
                        .args_from_usage("--le=<FILE> 'The leader_per_epoch configuration'"),
                )
                .subcommand(
                    SubCommand::with_name("worker")
                        .about("Run a single worker")
                        .args_from_usage("--id=<INT> 'The worker id'"),
                )
                .setting(AppSettings::SubcommandRequiredElseHelp),
        )
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .get_matches();

    let log_level = match matches.occurrences_of("v") {
        0 => "error",
        1 => "warn",
        2 => "info",
        3 => "debug",
        _ => "trace",
    };
    let mut logger = env_logger::Builder::from_env(Env::default().default_filter_or(log_level));
    #[cfg(feature = "benchmark")]
    logger.format_timestamp_millis();
    logger.init();

    match matches.subcommand() {
        ("generate_keys", Some(sub_matches)) => KeyPair::new()
            .export(sub_matches.value_of("filename").unwrap())
            .context("Failed to generate key pair")?,
        ("run", Some(sub_matches)) => run(sub_matches).await?,
        _ => unreachable!(),
    }
    Ok(())
}

// Runs either a worker or a primary.
async fn run(matches: &ArgMatches<'_>) -> Result<()> {
    let key_file = matches.value_of("keys").unwrap();
    let committee_file = matches.value_of("committee").unwrap();
    let parameters_file = matches.value_of("parameters");
    let store_path = matches.value_of("store").unwrap();

    // Read the committee and node's keypair from file.
    let keypair = KeyPair::import(key_file).context("Failed to load the node's keypair")?;
    let committee =
        Committee::import(committee_file).context("Failed to load the committee information")?;

    // Load default parameters if none are specified.
    let parameters = match parameters_file {
        Some(filename) => {
            Parameters::import(filename).context("Failed to load the node's parameters")?
        }
        None => Parameters::default(),
    };

    // Make the data store.
    let store = Store::new(store_path).context("Failed to create a store")?;

    // Channels the sequence of certificates.
    let (tx_output, rx_output) = channel(CHANNEL_CAPACITY);

    let (cer_to_coord_sender, cer_to_coord_receiver) =
        channel(CHANNEL_CAPACITY);
    // Check whether to run a primary, a worker, or an entire authority.
    match matches.subcommand() {
        // Spawn the primary and consensus core.
        ("primary", Some(sub_matches)) => {
            let avss_batch_size = sub_matches
                .value_of("bs")
                .unwrap()
                .parse::<u64>()
                .expect("avss_batch_size must be a valid number");
            let leader_per_epoch = sub_matches
                .value_of("le")
                .unwrap()
                .parse::<u64>()
                .expect("leader_per_epoch must be a valid number");
            assert!(avss_batch_size >= leader_per_epoch, "avss_batch_size must be greater than leader_per_epoch");
            BEACON_PER_EPOCH.set(avss_batch_size - leader_per_epoch).unwrap();
            MAX_EPOCH.set(leader_per_epoch).unwrap();
            let (breeze_share_cmd_sender, breeze_share_cmd_receiver) =
                channel(CHANNEL_CAPACITY);
            let (breeze_certificate_sender, breeze_certificate_receiver) =
                channel(CHANNEL_CAPACITY);
            let (cer_to_consensus_sender, cer_to_consensus_receiver) =
                channel(CHANNEL_CAPACITY);
            let (cer_to_init_consensus_sender, cer_to_init_consensus_receiver) =
                channel(CHANNEL_CAPACITY);
            let (init_cc_to_coord_sender, init_cc_to_coord_receiver) =
                channel(CHANNEL_CAPACITY);
            let (global_coin_recon_req_sender, global_coin_recon_req_receiver) =
                channel(CHANNEL_CAPACITY);
            let (beacon_recon_req_sender, beacon_recon_req_receiver) =
                channel(CHANNEL_CAPACITY);
            let (breeze_reconstruct_cmd_sender, breeze_reconstruct_cmd_receiver) =
                channel(CHANNEL_CAPACITY);
            let (breeze_result_sender, breeze_result_receiver) =
                channel(CHANNEL_CAPACITY);
            let (global_coin_res_sender, global_coin_res_receiver) =
                channel(CHANNEL_CAPACITY);
            let (beacon_res_sender, beacon_res_receiver) =
                channel(CHANNEL_CAPACITY);

            let crs_file = sub_matches.value_of("crs").unwrap();
            let crs =
                CommonReferenceString::import(crs_file).context("Failed to load the crs for breeze")?;
            #[cfg(feature = "pq")]
            MAX_INDEX.set(crs.g * (BEACON_PER_EPOCH.get().unwrap() + MAX_EPOCH.get().unwrap()) as usize).unwrap();
            // let crs = Arc::new(RwLock::new(crs));
            let mut address = committee.breeze_address(&keypair.name)?;
            address.set_ip("0.0.0.0".parse()?);
            let id = committee.get_id(&keypair.name).unwrap();
            let mut bft_address = committee.init_bft_address(&keypair.name)?;
            bft_address.set_ip("0.0.0.0".parse()?);
            #[cfg(feature = "pq")]
            let secret_size = (crs.n * crs.kappa) as f64;
            #[cfg(feature = "pq")]
            let slag = secret_size / 400f64 + secret_size / 4000f64 * committee.size() as f64;
            Breeze::spawn(
                keypair.clone(),
                address,
                id,
                // Arc::clone(&committee),
                committee.clone(),
                breeze_share_cmd_receiver,
                breeze_certificate_sender,
                breeze_reconstruct_cmd_receiver,
                breeze_result_sender,
                crs,
            );

            InitBFT::spawn(
                keypair.clone(),
                bft_address,
                committee.clone(),
                cer_to_init_consensus_receiver,
                init_cc_to_coord_sender
            ).await;
            // let committee = Arc::new(RwLock::new(committee));

            Coordinator::spawn(
                // Arc::clone(&committee),
                committee.clone(),
                breeze_share_cmd_sender,
                breeze_certificate_receiver,
                cer_to_consensus_sender,
                cer_to_init_consensus_sender,
                cer_to_coord_receiver,
                init_cc_to_coord_receiver,
                global_coin_recon_req_receiver,
                beacon_recon_req_receiver,
                breeze_reconstruct_cmd_sender,
                breeze_result_receiver,
                global_coin_res_sender,
                beacon_res_sender,
            ).await;
            #[cfg(feature = "pq")]
            sleep(Duration::from_secs(slag as u64)).await;

            let (tx_new_certificates, rx_new_certificates) = channel(CHANNEL_CAPACITY);
            let (tx_commit, rx_commit) = channel(CHANNEL_CAPACITY);
            let (tx_metadata, rx_metadata) = channel(CHANNEL_CAPACITY);
            #[cfg(not(feature = "dolphin"))]
            {
                Tusk::spawn(
                    committee.clone(),
                    parameters.gc_depth,
                    /* rx_primary */ rx_new_certificates,
                    tx_commit,
                    tx_output,

                    global_coin_recon_req_sender,
                    global_coin_res_receiver
                );
                let _not_used = tx_metadata;
            }
            #[cfg(feature = "dolphin")]
            Dolphin::spawn(
                committee.clone(),
                parameters.timeout,
                parameters.gc_depth,
                /* rx_primary */ rx_new_certificates,
                tx_commit,
                tx_metadata,
                tx_output,
            
                global_coin_recon_req_sender,
                global_coin_res_receiver
            );
            
            Primary::spawn(
                keypair,
                committee,
                parameters.clone(),
                store,
                /* tx_output */ tx_new_certificates,
                rx_commit,
                rx_metadata,
            
                cer_to_consensus_receiver,
            );
            BeaconHub::spawn(
                beacon_recon_req_sender,
                beacon_res_receiver
            );
        }

        // Spawn a single worker.
        ("worker", Some(sub_matches)) => {
            let id = sub_matches
                .value_of("id")
                .unwrap()
                .parse::<WorkerId>()
                .context("The worker id must be a positive integer")?;
            Worker::spawn(keypair.name, id, committee, parameters, store);
        }
        _ => unreachable!(),
    }

    // Analyze the consensus' output.
    analyze(rx_output,cer_to_coord_sender).await;

    // If this expression is reached, the program ends and all other tasks terminate.
    unreachable!();
}

/// Receives an ordered list of certificates and apply any application-specific logic.
async fn analyze(mut rx_output: Receiver<Certificate>, cer_to_coord_sender: Sender<BreezeCertificate>) {
    while let Some(certificate) = rx_output.recv().await {
        if let Some(cer) = certificate.header.breeze_cer{
            cer_to_coord_sender.send(cer).await.unwrap();
        }
        // NOTE: Here goes the application logic.
    }
}
