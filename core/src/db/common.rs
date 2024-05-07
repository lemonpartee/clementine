//! # Common Database Operations
//!
//! Common database operations for both operator and verifier. This module
//! directly talks with PostgreSQL. It is expected that PostgreSQL is properly
//! installed and configured.
//!
//! ## Testing
//!
//! For testing, user can supply out-of-source-tree configuration file with
//! `TEST_CONFIG` environment variable (`core/src/test_common.rs`).
//!
//! Tests that requires a proper PostgreSQL host configuration flagged with
//! `ignore`. They can be run if configuration is OK with `--include-ignored`
//! `cargo test` flag.

use crate::EVMAddress;
use crate::{config::BridgeConfig, errors::BridgeError};
use crate::{merkle::MerkleTree, ConnectorUTXOTree, HashTree, InscriptionTxs, WithdrawalPayment};
use bitcoin::{OutPoint, TxOut, Txid, XOnlyPublicKey};
use clementine_circuits::{
    constants::{CLAIM_MERKLE_TREE_DEPTH, WITHDRAWAL_MERKLE_TREE_DEPTH},
    PreimageType,
};
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgRow, Pool, Postgres};
use std::sync::{Arc, Mutex};

/// Actual information that database will hold. This information is not directly
/// accessible for an outsider; It should be updated and used by a database
/// organizer. Therefore, it is internal use only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DatabaseContent {
    inscribed_connector_tree_preimages: Vec<Vec<PreimageType>>,
    connector_tree_hashes: Vec<HashTree>,
    claim_proof_merkle_trees: Vec<MerkleTree<CLAIM_MERKLE_TREE_DEPTH>>,
    inscription_txs: Vec<InscriptionTxs>,
    deposit_txs: Vec<(Txid, TxOut)>,
    withdrawals_merkle_tree: MerkleTree<WITHDRAWAL_MERKLE_TREE_DEPTH>,
    withdrawals_payment_txids: Vec<Vec<WithdrawalPayment>>,
    connector_tree_utxos: Vec<ConnectorUTXOTree>,
    start_block_height: u64,
    period_relative_block_heights: Vec<u32>,
}
impl DatabaseContent {
    pub fn new() -> Self {
        Self {
            inscribed_connector_tree_preimages: Vec::new(),
            withdrawals_merkle_tree: MerkleTree::new(),
            withdrawals_payment_txids: Vec::new(),
            inscription_txs: Vec::new(),
            deposit_txs: Vec::new(),
            connector_tree_hashes: Vec::new(),
            claim_proof_merkle_trees: Vec::new(),
            connector_tree_utxos: Vec::new(),
            start_block_height: 0,
            period_relative_block_heights: Vec::new(),
        }
    }
}

/// Main database struct that holds all the information of the database.
#[derive(Clone, Debug)]
pub struct Database {
    connection: Pool<Postgres>,
    lock: Arc<Mutex<usize>>,
}

/// First pack of implementation for the `Database`. This pack includes general
/// functions for accessing the database.
impl Database {
    /// Creates a new `Database`. Then tries to connect actual database.
    pub async fn new(config: BridgeConfig) -> Result<Self, BridgeError> {
        let url = "postgresql://".to_owned()
            + config.db_host.as_str()
            + ":"
            + config.db_port.to_string().as_str()
            + "?dbname="
            + config.db_name.as_str()
            + "&user="
            + config.db_user.as_str()
            + "&password="
            + config.db_password.as_str();
        tracing::debug!("Connecting database: {}", url);

        match sqlx::PgPool::connect(url.as_str()).await {
            Ok(c) => Ok(Self {
                connection: c,
                lock: Arc::new(Mutex::new(0)),
            }),
            Err(e) => Err(BridgeError::DatabaseError(e)),
        }
    }

    /// Runs given query through database and returns result received from
    /// database.
    async fn run_query(&self, query: &str) -> Result<Vec<PgRow>, sqlx::Error> {
        tracing::debug!("Running query: {}", query);

        sqlx::query(query).fetch_all(&self.connection).await
    }

    /// Calls actual database read function and writes it's contents to memory.
    #[cfg(poc)]
    fn read(&self) -> DatabaseContent {
        todo!()
    }

    /// Calls actual database write function and writes input data to database.
    #[cfg(poc)]
    fn write(&self, _content: DatabaseContent) {
        todo!()
    }
}

/// Second implementation pack of `Database`. This pack includes data
/// manupulation functions. They use first pack of functions to access database.
///
/// `Set` functions use a mutex to avoid data races while updating database. But
/// it is not guaranteed that calling `get` and `set` functions one by one won't
/// result on a data race. Users must do their own synchronization to avoid data
/// races.
impl Database {
    /// Adds a deposit transaction to database. This transaction includes the
    /// following:
    ///
    /// * Start UTXO
    /// * Return address
    /// * EVM address
    pub async fn add_deposit_transaction(
        &self,
        start_utxo: OutPoint,
        return_address: XOnlyPublicKey,
        evm_address: EVMAddress,
    ) -> Result<(), BridgeError> {
        // TODO: These probably won't panic. But we should handle these
        // properly regardless in the future.
        let sutxo = serde_json::to_string_pretty(&start_utxo).unwrap();
        let sutxo = sutxo.trim_matches('"');
        let ra = serde_json::to_string(&return_address).unwrap();
        let ra = ra.trim_matches('"');
        let ea = serde_json::to_string(&evm_address).unwrap();
        let ea = ea.trim_matches('"');

        let query = format!(
            "INSERT INTO deposit_transactions VALUES ('{}', '{}', '{}')",
            sutxo, ra, ea
        );

        match self.run_query(query.as_str()).await {
            Ok(_) => Ok(()),
            Err(e) => Err(BridgeError::DatabaseError(e)),
        }
    }

    #[cfg(poc)]
    pub async fn get_connector_tree_hash(
        &self,
        period: usize,
        level: usize,
        idx: usize,
    ) -> HashType {
        let content = self.read();

        // If database is empty, returns an empty array.
        match content.connector_tree_hashes.get(period) {
            Some(v) => match v.get(level) {
                Some(v) => match v.get(idx) {
                    Some(v) => *v,
                    _ => [0u8; 32],
                },
                _ => [0u8; 32],
            },
            _ => [0u8; 32],
        }
    }
    #[cfg(poc)]
    pub async fn set_connector_tree_hashes(&self, connector_tree_hashes: Vec<Vec<Vec<HashType>>>) {
        let _guard = self.lock.lock().unwrap();
        let mut content = self.read();
        content.connector_tree_hashes = connector_tree_hashes;
        self.write(content);
    }

    #[cfg(poc)]
    pub async fn get_claim_proof_merkle_tree(
        &self,
        period: usize,
    ) -> MerkleTree<CLAIM_MERKLE_TREE_DEPTH> {
        let content = self.read();

        match content.claim_proof_merkle_trees.get(period) {
            Some(p) => p.clone(),
            _ => MerkleTree::new(),
        }
    }
    #[cfg(poc)]
    pub async fn set_claim_proof_merkle_trees(
        &self,
        claim_proof_merkle_trees: Vec<MerkleTree<CLAIM_MERKLE_TREE_DEPTH>>,
    ) {
        let _guard = self.lock.lock().unwrap();
        let mut content = self.read();
        content.claim_proof_merkle_trees = claim_proof_merkle_trees;
        self.write(content);
    }

    #[cfg(poc)]
    pub async fn get_inscription_txs(&self) -> Vec<InscriptionTxs> {
        let content = self.read();
        content.inscription_txs.clone()
    }
    #[cfg(poc)]
    pub async fn get_inscription_txs_len(&self) -> usize {
        let content = self.read();
        content.inscription_txs.len()
    }
    #[cfg(poc)]
    pub async fn add_to_inscription_txs(&self, inscription_txs: InscriptionTxs) {
        let _guard = self.lock.lock().unwrap();
        let mut content = self.read();
        content.inscription_txs.push(inscription_txs);
        self.write(content);
    }

    // #[cfg(poc)]
    pub async fn get_deposit_tx(&self, _idx: usize) -> (Txid, TxOut) {
        // let content = self.read();
        // content.deposit_txs[idx].clone()
        todo!()
    }

    #[cfg(poc)]
    pub async fn get_deposit_txs(&self) -> Vec<(Txid, TxOut)> {
        let content = self.read();
        content.deposit_txs.clone()
    }

    #[cfg(poc)]
    pub async fn add_to_deposit_txs(&self, deposit_tx: (Txid, TxOut)) {
        let _guard = self.lock.lock().unwrap();
        let mut content = self.read();
        content.deposit_txs.push(deposit_tx);
        self.write(content);
    }

    #[cfg(poc)]
    pub async fn get_withdrawals_merkle_tree_index(&self) -> u32 {
        let content = self.read();
        content.withdrawals_merkle_tree.index
    }
    #[cfg(poc)]
    pub async fn add_to_withdrawals_merkle_tree(&self, hash: HashType) {
        let _guard = self.lock.lock().unwrap();
        let mut content = self.read();
        content.withdrawals_merkle_tree.add(hash);
        self.write(content);
    }

    #[cfg(poc)]
    pub async fn get_withdrawals_payment_for_period(
        &self,
        period: usize,
    ) -> Vec<WithdrawalPayment> {
        let content = self.read();
        content.withdrawals_payment_txids[period].clone()
    }
    #[cfg(poc)]
    pub async fn add_to_withdrawals_payment_txids(
        &self,
        period: usize,
        withdrawal_payment: WithdrawalPayment,
    ) {
        let _guard = self.lock.lock().unwrap();
        let mut content = self.read();
        while period >= content.withdrawals_payment_txids.len() {
            content.withdrawals_payment_txids.push(Vec::new());
        }
        content.withdrawals_payment_txids[period].push(withdrawal_payment);
        self.write(content);
    }

    #[cfg(poc)]
    pub async fn get_connector_tree_utxo(&self, idx: usize) -> ConnectorUTXOTree {
        let content = self.read();
        content.connector_tree_utxos[idx].clone()
    }
    #[cfg(poc)]
    pub async fn set_connector_tree_utxos(&self, connector_tree_utxos: Vec<ConnectorUTXOTree>) {
        let _guard = self.lock.lock().unwrap();
        let mut content = self.read();
        content.connector_tree_utxos = connector_tree_utxos;
        self.write(content);
    }

    #[cfg(poc)]
    pub async fn get_start_block_height(&self) -> u64 {
        let content = self.read();
        content.start_block_height
    }
    #[cfg(poc)]
    pub async fn set_start_block_height(&self, start_block_height: u64) {
        let _guard = self.lock.lock().unwrap();
        let mut content = self.read();
        content.start_block_height = start_block_height;
        self.write(content);
    }

    #[cfg(poc)]
    pub async fn get_period_relative_block_heights(&self) -> Vec<u32> {
        let content = self.read();
        content.period_relative_block_heights.clone()
    }
    #[cfg(poc)]
    pub async fn set_period_relative_block_heights(&self, period_relative_block_heights: Vec<u32>) {
        let _guard = self.lock.lock().unwrap();
        let mut content = self.read();
        content.period_relative_block_heights = period_relative_block_heights;
        self.write(content);
    }

    #[cfg(poc)]
    pub async fn get_inscribed_preimages(&self, period: usize) -> Vec<PreimageType> {
        let content = self.read();

        match content.inscribed_connector_tree_preimages.get(period) {
            Some(p) => p.clone(),
            _ => vec![[0u8; 32]],
        }
    }
    #[cfg(poc)]
    pub async fn add_inscribed_preimages(&self, period: usize, preimages: Vec<PreimageType>) {
        let _guard = self.lock.lock().unwrap();
        let mut content = self.read();
        while period >= content.inscribed_connector_tree_preimages.len() {
            content.inscribed_connector_tree_preimages.push(Vec::new());
        }
        content.inscribed_connector_tree_preimages[period] = preimages;
        self.write(content);
    }
}

/// These tests not just aims to show correctness of the implementation: They
/// are here to show doing asynchronous operations over db is possible and data
/// won't get corrupted while doing so. Although db functions guarantee there
/// won't be a data race once a function is called, they won't guarantee data
/// will stay same between two db function calls. Therefore we need to da a
/// manual synchronization between tests too.
///
/// Currently, some tests for some functions are absent because of the complex
/// parameters: They are hard to mock.
#[cfg(test)]
mod tests {
    use super::Database;
    use crate::{config::BridgeConfig, test_common, EVMAddress};
    use bitcoin::{OutPoint, XOnlyPublicKey};
    use sqlx::Row;

    #[tokio::test]
    async fn invalid_connection() {
        let mut config = BridgeConfig::new();
        config.db_host = "nonexistinghost".to_string();
        config.db_name = "nonexistingpassword".to_string();
        config.db_user = "nonexistinguser".to_string();
        config.db_password = "nonexistingpassword".to_string();
        config.db_port = 123;

        match Database::new(config).await {
            Ok(_) => {
                assert!(false);
            }
            Err(e) => {
                println!("{}", e);
                assert!(true);
            }
        };
    }

    #[tokio::test]
    #[ignore]
    async fn valid_connection() {
        let config =
            test_common::get_test_config_from_environment("test_config.toml".to_string()).unwrap();

        match Database::new(config).await {
            Ok(_) => {
                assert!(true);
            }
            Err(e) => {
                eprintln!("{}", e);
                assert!(false);
            }
        };
    }

    #[tokio::test]
    #[ignore]
    async fn write_read_string_query() {
        let config =
            test_common::get_test_config_from_environment("test_config.toml".to_string()).unwrap();
        let database = Database::new(config).await.unwrap();

        database
            .run_query("INSERT INTO test_table VALUES ('test_data');")
            .await
            .unwrap();

        let ret = database
            .run_query("SELECT * FROM test_table")
            .await
            .unwrap();

        let mut is_found: bool = false;
        for i in ret {
            if i.get::<String, _>(0) == "test_data" {
                is_found = true;
                break;
            }
        }

        assert!(is_found);
    }

    #[tokio::test]
    #[ignore]
    async fn write_read_int() {
        let config =
            test_common::get_test_config_from_environment("test_config.toml".to_string()).unwrap();
        let database = Database::new(config).await.unwrap();

        database
            .run_query("INSERT INTO test_table VALUES ('temp',69)")
            .await
            .unwrap();

        let ret = database
            .run_query("SELECT * FROM test_table")
            .await
            .unwrap();
        let mut is_found: bool = false;

        for i in ret {
            if let Ok(0x45) = i.try_get::<i32, _>(1) {
                is_found = true;
                break;
            }
        }

        assert!(is_found);
    }

    #[tokio::test]
    async fn add_deposit_transaction() {
        let config =
            test_common::get_test_config_from_environment("test_config.toml".to_string()).unwrap();
        let database = Database::new(config).await.unwrap();

        database
            .add_deposit_transaction(
                OutPoint::null(),
                XOnlyPublicKey::from_slice(&[
                    0x78u8, 0x19u8, 0x90u8, 0xd7u8, 0xe2u8, 0x11u8, 0x8cu8, 0xc3u8, 0x61u8, 0xa9u8,
                    0x3au8, 0x6fu8, 0xccu8, 0x54u8, 0xceu8, 0x61u8, 0x1du8, 0x6du8, 0xf3u8, 0x81u8,
                    0x68u8, 0xd6u8, 0xb1u8, 0xedu8, 0xfbu8, 0x55u8, 0x65u8, 0x35u8, 0xf2u8, 0x20u8,
                    0x0cu8, 0x4b,
                ])
                .unwrap(),
                EVMAddress([0u8; 20]),
            )
            .await
            .unwrap();
    }

    #[cfg(poc)]
    #[tokio::test]
    async fn connector_tree_hash() {
        let config =
            test_common::get_test_config_from_environment("test_config.toml".to_string()).unwrap();
        let database = Database::new(config).await.unwrap();

        let lock = unsafe { LOCK.clone().unwrap() };
        let _guard = lock.lock().unwrap();

        let mock_data = [0x45u8; 32];
        let mock_array: Vec<Vec<Vec<HashType>>> = vec![vec![vec![mock_data]]];

        assert_ne!(database.get_connector_tree_hash(0, 0, 0).await, mock_data);

        database.set_connector_tree_hashes(mock_array).await;
        assert_eq!(database.get_connector_tree_hash(0, 0, 0).await, mock_data);
    }

    #[cfg(poc)]
    #[tokio::test]
    async fn claim_proof_merkle_tree() {
        let config =
            test_common::get_test_config_from_environment("test_config.toml".to_string()).unwrap();
        let database = Database::new(config).await.unwrap();
        let lock = unsafe { LOCK.clone().unwrap() };
        let _guard = lock.lock().unwrap();

        let mut mock_data: Vec<MerkleTree<CLAIM_MERKLE_TREE_DEPTH>> = vec![MerkleTree::new()];
        mock_data[0].add([0x45u8; 32]);

        assert_ne!(
            database.get_claim_proof_merkle_tree(0).await,
            mock_data[0].clone()
        );

        database
            .set_claim_proof_merkle_trees(mock_data.clone())
            .await;
        assert_eq!(database.get_claim_proof_merkle_tree(0).await, mock_data[0]);
    }

    #[cfg(poc)]
    #[tokio::test]
    async fn withdrawals_merkle_tree() {
        let database = unsafe {
            initialize();
            DATABASE.clone().unwrap()
        };
        let lock = unsafe { LOCK.clone().unwrap() };
        let _guard = lock.lock().unwrap();

        let mock_data: HashType = [0x45u8; 32];

        assert_eq!(database.get_withdrawals_merkle_tree_index().await, 0);

        database
            .add_to_withdrawals_merkle_tree(mock_data.clone())
            .await;
        assert_eq!(database.get_withdrawals_merkle_tree_index().await, 1);
    }

    #[cfg(poc)]
    #[tokio::test]
    async fn start_block_height() {
        let database = unsafe {
            initialize();
            DATABASE.clone().unwrap()
        };
        let lock = unsafe { LOCK.clone().unwrap() };
        let _guard = lock.lock().unwrap();

        let mock_data: u64 = 0x45;

        assert_eq!(database.get_start_block_height().await, 0);

        database.set_start_block_height(mock_data).await;
        assert_eq!(database.get_start_block_height().await, mock_data);
    }

    #[cfg(poc)]
    #[tokio::test]
    async fn period_relative_block_heights() {
        let database = unsafe {
            initialize();
            DATABASE.clone().unwrap()
        };
        let lock = unsafe { LOCK.clone().unwrap() };
        let _guard = lock.lock().unwrap();

        let mock_data: u64 = 0x45;

        assert_eq!(database.get_start_block_height().await, 0);

        database.set_start_block_height(mock_data).await;
        assert_eq!(database.get_start_block_height().await, mock_data);
    }

    #[cfg(poc)]
    #[tokio::test]
    async fn inscribed_preimages() {
        let lock = unsafe { LOCK.clone().unwrap() };
        let _guard = lock.lock().unwrap();

        let mock_data: Vec<PreimageType> = vec![[0x45u8; 32]];

        assert_ne!(database.get_inscribed_preimages(0).await, mock_data);

        database.add_inscribed_preimages(0, mock_data.clone()).await;
        assert_eq!(database.get_inscribed_preimages(0).await, mock_data);

        // Clean things up.
        match fs::remove_file(DB_FILE_PATH) {
            Ok(_) => assert!(true),
            Err(_) => assert!(false),
        }
    }
}
