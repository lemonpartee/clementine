// use std::borrow::BorrowMut;
// use std::ops::Add;

use bitcoin::hashes::{Hash, HashEngine};
use bitcoin::opcodes::all::OP_CHECKSIG;
use bitcoin::script::Builder;
use bitcoin::taproot::LeafVersion;
// use bitcoin::sighash::SighashCache;
use bitcoin::{Address, Amount, Script, ScriptBuf, TapTweakHash, TxOut, XOnlyPublicKey}; // Script, ScriptBuf, Transaction, TxIn,
use bitcoincore_rpc::{Auth, RawTx};
// use clementine_circuits::constants::BRIDGE_AMOUNT_SATS;
use clementine_core::actor::Actor;
// use clementine_core::config::BridgeConfig;
use clementine_core::extended_rpc::ExtendedRpc;
// use clementine_core::script_builder::ScriptBuilder;
use clementine_core::{cli, create_extended_rpc};
use clementine_core::transaction_builder::{CreateTxOutputs, TransactionBuilder};
use clementine_core::utils::handle_taproot_witness_new;

fn main() {
    let secp = bitcoin::secp256k1::Secp256k1::new();
    let mut 
    config = cli::get_configuration();

    let (xonly_pk, _) = config.secret_key.public_key(&secp).x_only_public_key();
    println!("x only pub key: {:?}", xonly_pk);

    let address = Address::p2tr(&secp, xonly_pk, None, config.network);
    println!("address: {:?}", address.to_string());

    let script = address.script_pubkey();
    println!("script: {:?}", hex::encode(script.as_bytes()));

    let tweaked_pk_script: [u8; 32] = script.as_bytes()[2..].try_into().unwrap();
    println!("tweaked pk: {:?}", hex::encode(tweaked_pk_script));

    // calculate tweaked pk, i.e. Q
    let mut hasher = TapTweakHash::engine();
    hasher.input(&xonly_pk.serialize());
    let (q, _) = xonly_pk
        .add_tweak(
            &secp,
            &secp256k1::Scalar::from_be_bytes(TapTweakHash::from_engine(hasher).to_byte_array())
                .unwrap(),
        )
        .unwrap();
    println!("q:          {:?}", hex::encode(q.serialize()));

    let rpc = create_extended_rpc!(config);

    let to_pay_script = ScriptBuf::from_bytes(hex::decode("14865e91f24c6ec441f01ed04b764385e48108304c5814985dc0d964a56655af63e66eaafab9f01b2c85b453146ec3dba02c692fa682ce79bb4ceeedbdd9aa31cb57145c8d0790b0730d3290a78e07502c06511d8521f95c14acef459604ff1f304439e9a32ea68b93d5f630a20014ea9a78eab619d2083df179c75a86f687861c35be0014151f168ef4795416c669d77b49a8651c18c6021300147ca4b0b440ca70cfcf32a4f0e67e2174fa26b3c10014d84820dfa60baf7ba34ef849c225b86a8aaee0ca0014cbf34f81799254df30ad8853aaeed2fc14a55e355514ee87143027e0c67c4eb59cf96833f8f5d6b7c3255a5fa3766b6b76a976a976a976a976a976a976a976a976a976a976a976a976a976a976a96c7914cfe55f0d7f55fb10b5ab95f7b370b986940a5f88886d6d6d6d6d6d6d6d5fa3766b6b76a976a976a976a976a976a976a976a976a976a976a976a976a976a976a96c7914799e5adde121f6e0d29a32e5adeab5614e3fd418886d6d6d6d6d6d6d6d5fa3766b6b76a976a976a976a976a976a976a976a976a976a976a976a976a976a976a96c7914be4f5c0539153982dcd046ebfac41eb9ffd6f040886d6d6d6d6d6d6d6d5fa3766b6b76a976a976a976a976a976a976a976a976a976a976a976a976a976a976a96c7914746cc0daab0217da8556e3e646ee7ed2050bcd70886d6d6d6d6d6d6d6d5fa3766b6b76a976a976a976a976a976a976a976a976a976a976a976a976a976a976a96c7914773c571d3dc4745d887db4fd3e2b024e8710bc85886d6d6d6d6d6d6d6d5fa3766b6b76a976a976a976a976a976a976a976a976a976a976a976a976a976a976a96c791426ffff405d264d81dcd1d306258a42d8e1929edd886d6d6d6d6d6d6d6d5fa3766b6b76a976a976a976a976a976a976a976a976a976a976a976a976a976a976a96c791453f8bf07ef9e4317a8bcfa696ee5004008c89bfa886d6d6d6d6d6d6d6d5fa3766b6b76a976a976a976a976a976a976a976a976a976a976a976a976a976a976a96c791470c075882caf0aefb0cb2c96c76ee7551712a6e1886d6d6d6d6d6d6d6d5fa3766b6b76a976a976a976a976a976a976a976a976a976a976a976a976a976a976a96c7914cc24625ed2f9ad63bbe49fd8cce10dfa452f1660886d6d6d6d6d6d6d6d5fa3766b6b76a976a976a976a976a976a976a976a976a976a976a976a976a976a976a96c7914577cecbd65b5557e284ed647d5f1492415511bcd886d6d6d6d6d6d6d6d5fa3766b6b76a976a976a976a976a976a976a976a976a976a976a976a976a976a976a96c791486502e23fe6eff305e5b2625a5b762c634eb5258886d6d6d6d6d6d6d6d6c768f6c7d946c7d946c7d946c7d946c7d946c7d946c7d940178936c76937693769376936c9376937693769376936c938876937693769376939376937693769376939376937693769376939376937693769376939376937693769376939376937693769376939376937693769376939302f00394b1").unwrap());

    let (taproot_address, taproot_spend_info) = TransactionBuilder::create_taproot_address(
        vec![to_pay_script.clone()],
        config.network,
    )
    .unwrap();
    println!("taproot address: {:?}", taproot_address.to_string());
    println!("taproot spend info: {:?}", taproot_spend_info);
    let utxo = rpc.send_to_address(&taproot_address, 1000).unwrap();

    let ins = TransactionBuilder::create_tx_ins(vec![utxo.clone()]);

    let tx_outs = vec![TxOut {
        value: Amount::from_sat(330),
        script_pubkey: taproot_address.script_pubkey(),
    }];

    let prevouts = vec![TxOut {
        value: Amount::from_sat(1000),
        script_pubkey: taproot_address.script_pubkey(),
    }];

    let tx = TransactionBuilder::create_btc_tx_with_locktime(ins, tx_outs.clone(), 51000);

    let signer = Actor::new(config.secret_key, config.network);

    let mut tx_details = CreateTxOutputs {
        tx: tx.clone(),
        prevouts,
        scripts: vec![vec![to_pay_script.clone()]],
        taproot_spend_infos: vec![taproot_spend_info.clone()],
    };

    tx_details.tx.input[0].witness.push(to_pay_script.clone());
    let spend_control_block = taproot_spend_info.control_block(&(
        to_pay_script.clone(),
        LeafVersion::TapScript,
    ))
    .unwrap();
    tx_details.tx.input[0].witness.push(&spend_control_block.serialize()); 
    println!("tx: {:?}", tx_details.tx);
    println!("tx_hex: {:?}", tx_details.tx.raw_hex());
    println!("tx_size: {:?}", tx_details.tx.weight());
    let result = rpc.send_raw_transaction(&tx_details.tx).unwrap();

    println!("Result: {:?}", result);
    println!("UTXO: {:?}", utxo);

}