use bitcoin::taproot::Signature;
use bitcoin::sighash::SighashCache;
use bitcoin::{Address, Amount, TxIn, TxOut};
use bitcoincore_rpc::Auth;
use clementine_circuits::constants::BRIDGE_AMOUNT_SATS;
use clementine_core::actor::Actor;
use clementine_core::config::BridgeConfig;
use clementine_core::extended_rpc::ExtendedRpc;
use clementine_core::transaction_builder::TransactionBuilder;
use clementine_core::{cli, EVMAddress};
fn calculate_min_relay_fee(n: u64) -> u64 {
    98 + 57 * n + ((n - 2) / 2)
}

fn main() {
    let secp = bitcoin::secp256k1::Secp256k1::new();
    let config = cli::get_configuration();
    println!("config: {:?}", config);
    let rpc = ExtendedRpc::new(
        config.bitcoin_rpc_url.clone(),
        config.bitcoin_rpc_user.clone(),
        config.bitcoin_rpc_password.clone(),
    );
    let (xonly_pk, _) = config.secret_key.public_key(&secp).x_only_public_key();
    let actor = Actor::new(config.secret_key, config.network);

    let address = actor.address.clone();

    let operator_commitment = rpc.send_to_address(&address, 100_005_000).unwrap();
    let leaf = rpc.send_to_address(&address, 330).unwrap();

    let txouts = TransactionBuilder::create_tx_outs(vec![(
        Amount::from_sat(100_000_000),
        address.script_pubkey(),
    )]);

    let txins = TransactionBuilder::create_tx_ins(vec![leaf]);
    let mut prevouts = vec![
        TxOut {
            script_pubkey: address.script_pubkey(),
            value: Amount::from_sat(330),
        },
    ];

    let mut tx = TransactionBuilder::create_btc_tx(txins, txouts);
    let mut sighash_cache = SighashCache::new(&tx);
    let sighash = sighash_cache.taproot_key_spend_signature_hash(0,&bitcoin::sighash::Prevouts::All(&prevouts), bitcoin::sighash::TapSighashType::SinglePlusAnyoneCanPay).unwrap();
    let sig = actor.sign_with_tweak(sighash, None).unwrap();
    tx.input[0].witness.push(
        Signature {
            signature: sig,
            sighash_type: bitcoin::sighash::TapSighashType::SinglePlusAnyoneCanPay,
        }
        .to_vec(),
    );
    let add_txin = TransactionBuilder::create_tx_ins(vec![operator_commitment]);
    println!("tx: {:?}", tx);
    // let txid = rpc.send_raw_transaction(&tx).unwrap();
    tx.input.push(add_txin[0].clone());
    prevouts.push(TxOut {
        script_pubkey: address.script_pubkey(),
        value: Amount::from_sat(100_005_000),
    });
    tx.output.push(TxOut {
        script_pubkey: address.script_pubkey(),
        value: Amount::from_sat(4000),
    });
    let mut sighash_cache = SighashCache::new(&tx);
    let sighash = sighash_cache.taproot_key_spend_signature_hash(1,&bitcoin::sighash::Prevouts::All(&prevouts), bitcoin::sighash::TapSighashType::Default).unwrap();
    let sig = actor.sign_with_tweak(sighash, None).unwrap();
    tx.input[1].witness.push(sig.as_ref());
    println!("tx new: {:?}", tx);
    let txid = rpc.send_raw_transaction(&tx).unwrap();

    println!("txid: {:?}", txid);
}