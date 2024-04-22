use std::str::FromStr;

use bitcoin::{address, XOnlyPublicKey};
use clementine_core::{extended_rpc::ExtendedRpc, user::User};
use clementine_core::{keys, EVMAddress};
use secp256k1::SecretKey;
fn main() {
    let rpc = ExtendedRpc::new();
    // let (secret_key, all_xonly_pks) = keys::get_from_file().unwrap();

    // let user = User::new(rpc.clone(), all_xonly_pks.clone(), secret_key);
    // let evm_address: EVMAddress = [1u8; 20];
    // let address = user.get_deposit_address(evm_address).unwrap();

    let secp = secp256k1::Secp256k1::new();
    // println!("EVM Address: {:?}", hex::encode(evm_address));
    // println!("User: {:?}", user.signer.xonly_public_key.to_string());
    // println!("Deposit address: {:?}", address);
    let faucet_sk =
        SecretKey::from_str("fbec9961573617004dfef3b8035e4c43df2effc9bf95cb4a63f917413cf9258f")
            .unwrap();
    let faucet_keypair = secp256k1::Keypair::from_secret_key(&secp, &faucet_sk);
    let faucet_pk = XOnlyPublicKey::from_keypair(&faucet_keypair);
    let address = address::Address::p2tr(&secp, faucet_pk.0, None, bitcoin::Network::Testnet);
    println!("Faucet Address: {:?}", address);
}
