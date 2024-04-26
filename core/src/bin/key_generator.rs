//! This binary generates random private/public key pairs for testing. They will
//! be put in `ENV_DIR`/`PREFIX`(0..`num_verifiers`).json.
//! File format is described in `core/src/keys.rs`.

use bitcoin::XOnlyPublicKey;
use clementine_core::keys::{self, FileContents};
use crypto_bigint::rand_core::OsRng;
use secp256k1::SecretKey;
use std::{
    env,
    fs::{self, File},
    io::Write,
};

/// Environment variable that defines key file's directory.
const ENV_DIR: &str = "KEY_DIR";

/// Defualt directory to put generated key files if `ENV_DIR` is not specified.
const DIRECTORY: &str = "configs";

/// Key file prefix.
const PREFIX: &str = "keys";

fn main() {
    let directory = env::var(ENV_DIR).unwrap_or_else(|_| DIRECTORY.to_string());
    let num_verifiers: usize = env::var("NUM_VERIFIERS")
        .unwrap_or_else(|_| "1".to_string())
        .parse()
        .unwrap();

    let (all_sks, all_xonly_pks) = generate_keypair(num_verifiers);
    println!("Generated private keys: {:#?}", all_sks.clone());
    println!("Generated public keys: {:#?}", all_xonly_pks.clone());

    // Create directory. If it exist, it will return an `Err`. Handle that with
    // a variable.
    let _ = fs::create_dir(directory.clone());

    for i in 0..all_sks.len() {
        create_file(&directory, i, all_sks.clone(), all_xonly_pks.clone());
    }
}

/// This function's contents are copied from clementine_core's `main.rs`.
/// Currently it is not in a dedicated function. If it is refactored to have a
/// dedicated function, it should also be used here and this should be deleted.
/// It is not ideal to have a possibly different key generator algorithms, in
/// case of a change.
fn generate_keypair(num_verifiers: usize) -> (Vec<SecretKey>, Vec<XOnlyPublicKey>) {
    let secp: secp256k1::Secp256k1<secp256k1::All> = bitcoin::secp256k1::Secp256k1::new();
    let rng = &mut OsRng;

    let (all_sks, all_xonly_pks): (Vec<_>, Vec<_>) =
        keys::create_key_pairs(secp.clone(), rng, num_verifiers);

    (all_sks, all_xonly_pks)
}

/// Creates nth file in key directory.
fn create_file(
    directory: &String,
    index: usize,
    all_sks: Vec<SecretKey>,
    all_xonly_sks: Vec<XOnlyPublicKey>,
) {
    let content = FileContents {
        private_key: all_sks[index],
        public_keys: all_xonly_sks,
        id: index,
    };

    let serialized = serde_json::to_string_pretty(&content).unwrap();
    let file = directory.to_string() + "/" + PREFIX + index.to_string().as_str() + ".json";

    let mut file = File::create(file).unwrap();
    file.write_all(serialized.as_bytes()).unwrap();
}
