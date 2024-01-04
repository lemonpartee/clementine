use std::collections::HashMap;

use crate::actor::{Actor, EVMAddress, EVMSignature};
use crate::merkle::MerkleTree;
use crate::verifier::Verifier;
use bitcoin::{
    absolute,
    hashes::Hash,
    secp256k1,
    secp256k1::{schnorr, PublicKey},
    Address, Txid,
};
use bitcoincore_rpc::Client;
use circuit_helpers::config::NUM_VERIFIERS;
use circuit_helpers::hashes::sha256;
use secp256k1::rand::rngs::OsRng;
use sha2::{Digest, Sha256};

pub const NUM_ROUNDS: usize = 10;
type PreimageType = [u8; 32];
type HashType = [u8; 32];

pub fn check_deposit(
    _rpc: &Client,
    _txid: [u8; 32],
    _hash: [u8; 32],
    _return_address: Address,
    _verifiers_pks: Vec<PublicKey>,
) -> absolute::Time {
    // 1. Check if txid is mined in bitcoin
    // 2. Check if 0th output of the txid has 1 BTC
    // 3. Check if 0th output of the txid's scriptpubkey is N-of-N multisig and preimage of Hash or return_address after 200 blocks
    // 4. If all checks pass, return true
    // 5. Return the UNIX timestamp of the block in which the txid was mined
    return absolute::Time::MAX;
}

pub struct DepositPresigns {
    pub rollup_sign: EVMSignature,
    pub kickoff_sign: schnorr::Signature,
    pub kickoff_txid: Txid,
    pub move_bridge_sign: Vec<schnorr::Signature>,
    pub operator_take_sign: Vec<schnorr::Signature>,
}

pub struct Operator<'a> {
    rpc: &'a Client,
    signer: Actor,
    verifiers: Vec<PublicKey>,
    verifier_evm_addresses: Vec<EVMAddress>,
    deposit_presigns: Vec<[DepositPresigns; NUM_VERIFIERS]>,
    deposit_merkle_tree: MerkleTree,
    withdrawals_merkle_tree: MerkleTree,
    mock_verifier_access: Vec<Verifier<'a>>, // on production this will be removed rather we will call the verifier's API
    waiting_deposists: HashMap<Txid, HashType>
}

pub fn check_presigns(
    txid: [u8; 32],
    timestamp: absolute::Time,
    deposit_presigns: &DepositPresigns,
) {
}

impl<'a> Operator<'a> {
    pub fn new(rng: &mut OsRng, rpc: &'a Client) -> Self {
        let signer = Actor::new(rng);
        let mut verifiers = Vec::new();
        for _ in 0..NUM_VERIFIERS {
            verifiers.push(Verifier::new(rng, rpc, signer.public_key));
        }
        let verifiers_pks = verifiers
            .iter()
            .map(|verifier| verifier.signer.public_key)
            .collect::<Vec<_>>();

        verifiers.iter_mut().for_each(|verifier| {
            verifier.set_verifiers(verifiers_pks.clone());
        });

        let verifier_evm_addresses = verifiers
            .iter()
            .map(|verifier| verifier.signer.evm_address)
            .collect::<Vec<_>>();
        let deposit_presigns = vec![];
        
        Self {
            rpc,
            signer,
            verifiers: verifiers_pks,
            verifier_evm_addresses,
            deposit_presigns,
            deposit_merkle_tree: MerkleTree::initial(),
            withdrawals_merkle_tree: MerkleTree::initial(),
            mock_verifier_access: verifiers,
            waiting_deposists: HashMap::new(),
        }
    }
    // this is a public endpoint that every depositor can call
    pub fn new_deposit(
        &self,
        txid: [u8; 32],
        hash: [u8; 32],
        return_address: Address,
    ) -> Vec<EVMSignature> {
        // self.verifiers + signer.public_key
        let mut all_verifiers = self.verifiers.to_vec();
        all_verifiers.push(self.signer.public_key);
        let timestamp = check_deposit(
            self.rpc,
            txid,
            hash,
            return_address.clone(),
            all_verifiers.to_vec(),
        );

        let presigns_from_all_verifiers = self
            .mock_verifier_access
            .iter()
            .map(|verifier| {
                // Note: In this part we will need to call the verifier's API to get the presigns
                let deposit_presigns = verifier.new_deposit(txid, hash, return_address.clone());
                check_presigns(txid, timestamp, &deposit_presigns);
                deposit_presigns
            })
            .collect::<Vec<_>>();

        let kickoff_txid = Txid::all_zeros();

        let rollup_sign = self.signer.sign_deposit(
            kickoff_txid,
            timestamp.to_consensus_u32().to_be_bytes(),
            hash,
        );
        let mut all_rollup_signs = presigns_from_all_verifiers
            .iter()
            .map(|presigns| presigns.rollup_sign)
            .collect::<Vec<_>>();
        all_rollup_signs.push(rollup_sign);

        all_rollup_signs
    }

    // this is called when a Withdrawal event emitted on rollup
    pub fn new_withdrawal(withdrawal_address: Address) {
        // 1. Add the address to WithdrawalsMerkleTree
        // 2. Pay to the address and save the txid
    }

    // this is called when a Deposit event emitted on rollup
    pub fn preimage_revealed(&mut self, preimage: [u8; 32], txid: Txid) {
        let hash = self.waiting_deposists.get(&txid).unwrap().clone();
        // calculate hash of preimage
        let mut hasher = Sha256::new();
        hasher.update(preimage);
        let calculated_hash: HashType = hasher.finalize().try_into().unwrap();
        if calculated_hash != hash {
            panic!("preimage does not match with the hash");
        }

        // 1. Add the corresponding txid to DepositsMerkleTree
        self.deposit_merkle_tree.add(txid.to_byte_array());
        // this function is interal, where it checks if the preimage is revealed, then if it is revealed
        // it starts the kickoff tx.
    }

    // this function is interal, where it checks if the current bitcoin height reaced to th end of the period,
    pub fn period1_end(&self) {
        self.move_bridge_funds();

        // Check if all deposists are satisifed, all remaning bridge funds are moved to a new multisig
    }

    // this function is interal, where it checks if the current bitcoin height reaced to th end of the period,
    pub fn period2_end(&self) {
        // This is the time we generate proof.
    }

    // this function is interal, where it checks if the current bitcoin height reaced to th end of the period,
    pub fn period3_end(&self) {
        // This is the time send generated proof along with k-deep proof
        // and revealing bit-commitments for the next bitVM instance.
    }

    // this function is interal, where it moves remaining bridge funds to a new multisig using DepositPresigns
    fn move_bridge_funds(&self) {}

    // This function is internal, it gives the appropriate response for a bitvm challenge
    pub fn challenge_received() {}
}
