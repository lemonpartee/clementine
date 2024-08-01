use crate::actor::Actor;
use crate::config::BridgeConfig;
use crate::database::verifier::VerifierDB;
use crate::errors::BridgeError;
use crate::extended_rpc::ExtendedRpc;
use crate::musig::{self, MusigAggNonce, MusigPartialSignature, MusigPubNonce};
use crate::traits::rpc::VerifierRpcServer;
use crate::transaction_builder::TransactionBuilder;
use crate::{script_builder, utils, EVMAddress, PsbtOutPoint};
use bitcoin::address::{NetworkChecked, NetworkUnchecked};
use bitcoin::hashes::Hash;
use bitcoin::{secp256k1, secp256k1::Secp256k1, OutPoint};
use bitcoin::{Address, Amount, Network, TxOut, Txid};
use bitcoin_mock_rpc::RpcApiWrapper;
use clementine_circuits::constants::BRIDGE_AMOUNT_SATS;
use clementine_circuits::sha256_hash;
use jsonrpsee::core::async_trait;
use secp256k1::schnorr;
use secp256k1::XOnlyPublicKey;

#[derive(Debug, Clone)]
pub struct Verifier<R>
where
    R: RpcApiWrapper,
{
    rpc: ExtendedRpc<R>,
    signer: Actor,
    transaction_builder: TransactionBuilder,
    db: VerifierDB,
    network: Network,
    confirmation_treshold: u32,
    min_relay_fee: u64,
    user_takes_after: u32,
}

impl<R> Verifier<R>
where
    R: RpcApiWrapper,
{
    pub async fn new(rpc: ExtendedRpc<R>, config: BridgeConfig) -> Result<Self, BridgeError> {
        let signer = Actor::new(config.secret_key, config.network);

        let secp: Secp256k1<secp256k1::All> = Secp256k1::new();

        let pk: secp256k1::PublicKey = config.secret_key.public_key(&secp);
        let xonly_pk = XOnlyPublicKey::from(pk);

        // Generated public key must be in given public key list.
        if !config.verifiers_public_keys.contains(&xonly_pk) {
            return Err(BridgeError::PublicKeyNotFound);
        }

        let db = VerifierDB::new(config.clone()).await;

        let transaction_builder =
            TransactionBuilder::new(config.verifiers_public_keys.clone(), config.network);

        Ok(Verifier {
            rpc,
            signer,
            transaction_builder,
            db,
            network: config.network,
            confirmation_treshold: config.confirmation_treshold,
            min_relay_fee: config.min_relay_fee,
            user_takes_after: config.user_takes_after,
        })
    }

    /// Operator only endpoint for verifier.
    ///
    /// 1. Check if the deposit UTXO is valid, finalized (6 blocks confirmation) and not spent
    /// 2. Generate random pubNonces, secNonces
    /// 3. Save pubNonces and secNonces to a db
    /// 4. Return pubNonces
    async fn new_deposit(
        &self,
        deposit_utxo: &OutPoint,
        recovery_taproot_address: &Address<NetworkUnchecked>,
        evm_address: &EVMAddress,
    ) -> Result<Vec<MusigPubNonce>, BridgeError> {
        self.rpc.check_deposit_utxo(
            &self.transaction_builder,
            &deposit_utxo,
            recovery_taproot_address,
            evm_address,
            BRIDGE_AMOUNT_SATS,
            self.user_takes_after,
            self.confirmation_treshold,
        )?;

        let num_required_sigs = 10; // TODO: Fix this

        let pub_nonces_from_db = self.db.get_pub_nonces(deposit_utxo).await?;
        if let Some(pub_nonces) = pub_nonces_from_db {
            return Ok(pub_nonces);
        }

        // let nonces = musig::nonce_pair(&self.signer.keypair);

        let nonces = (0..num_required_sigs)
            .map(|_| musig::nonce_pair(&self.signer.keypair))
            .collect::<Vec<_>>();

        self.db.save_pub_nonces(deposit_utxo, &nonces).await?;

        let pub_nonces = nonces.iter().map(|(pub_nonce, _)| *pub_nonce).collect();

        Ok(pub_nonces)
    }

    /// - Check the kickoff_utxos
    /// - Save agg_nonces to a db for future use
    /// - for every kickoff_utxo, calculate kickoff2_tx
    /// - for every kickoff2_tx, partial sign burn_tx (ommitted for now)
    /// - return MusigPartialSignature of sign(kickoff2_txids)
    async fn operator_kickoffs_generated(
        &self,
        deposit_utxo: &OutPoint,
        kickoff_utxos: Vec<PsbtOutPoint>,
        agg_nonces: Vec<MusigAggNonce>,
    ) -> Result<MusigPartialSignature, BridgeError> {
        for kickoff_utxo in kickoff_utxos.iter() {
            let value = kickoff_utxo.tx.output[kickoff_utxo.vout as usize].value;
            if value.to_sat() < 100_000 {
                // TODO: Fix constant check
                return Err(BridgeError::InvalidKickoffUtxo);
            }
        }

        let kickoff_outpoints_and_amounts = kickoff_utxos
            .iter()
            .map(|x| {
                (
                    OutPoint {
                        txid: x.tx.compute_txid(),
                        vout: x.vout,
                    },
                    x.tx.output[x.vout as usize].value,
                )
            })
            .collect::<Vec<_>>();

        // TODO: If also operator, check if our kick_off_utxo is in the list and in the correct place

        self.db.save_agg_nonces(deposit_utxo, &agg_nonces).await?;

        self.db
            .save_kickoff_utxos(deposit_utxo, &kickoff_outpoints_and_amounts)
            .await?;

        // TODO: Sign burn txs

        let kickoff_txids_root = utils::calculate_merkle_root(
            kickoff_outpoints
                .iter()
                .map(|x| {
                    sha256_hash!(
                        x.txid.to_raw_hash().as_byte_array().clone(),
                        x.vout.to_be_bytes()
                    )
                })
                .collect::<Vec<_>>(),
        );

        let kickoffs_digest = sha256_hash!(
            deposit_utxo.txid,
            deposit_utxo.vout.to_be_bytes(),
            kickoff_txids_root
        );

        let nonces = self.db.get_nonces(deposit_utxo, 0).await?;

        let (pubNonce, secNonce, aggNonce) = nonces.ok_or(BridgeError::NoncesNotFound)?;

        let (partial_kickoff_digest_sig, _) = musig::partial_sign(
            vec![],
            aggNonce,
            &self.signer.keypair,
            secNonce,
            kickoffs_digest,
            None,
            None,
        );

        Ok(partial_kickoff_digest_sig)
    }

    /// verify burn txs are signed by verifiers
    /// sign operator_takes_txs
    async fn burn_txs_signed_rpc(
        &self,
        deposit_utxo: &OutPoint,
        burn_sigs: Vec<schnorr::Signature>,
    ) -> Result<Vec<MusigPartialSignature>, BridgeError> {
        // TODO: Verify burn txs are signed by verifiers

        let kickoff_outpoints_and_amounts = self
            .db
            .get_kickoff_outpoints_and_amounts(deposit_utxo)
            .await?;

        let kickoff_outpoints_and_amounts =
            kickoff_outpoints_and_amounts.ok_or(BridgeError::KickoffOutpointsNotFound)?;

        let partial_operator_takes_sigs = kickoff_outpoints_and_amounts
            .iter()
            .map(|(kickoff_outpoint, kickoff_amount)| {
                let ins = TransactionBuilder::create_tx_ins(vec![kickoff_outpoint.clone()]);
                let outs = vec![
                    TxOut {
                        value: Amount::from_sat(kickoff_amount.to_sat() - 330),
                        script_pubkey: self.signer.address.script_pubkey(), // TODO: Fix this address to operator or 200 blocks N-of-N
                    },
                    script_builder::anyone_can_spend_txout(),
                ];
                let tx = TransactionBuilder::create_btc_tx(ins, outs);

                let ins = TransactionBuilder::create_tx_ins(vec![
                    deposit_utxo.clone(),
                    OutPoint {
                        txid: tx.compute_txid(),
                        vout: 0,
                    },
                ]);
                let outs = vec![
                    TxOut {
                        value: Amount::from_sat(
                            kickoff_amount.to_sat() - 330 + BRIDGE_AMOUNT_SATS - 330,
                        ),
                        script_pubkey: self.signer.address.script_pubkey(), // TODO: Fix this address to operator
                    },
                    script_builder::anyone_can_spend_txout(),
                ];

                let tx = TransactionBuilder::create_btc_tx(ins, outs);

            })
            .collect::<Vec<_>>();

        Ok(vec![[0u8; 32]; 10])
    }

    async fn new_withdrawal_direct(
        &self,
        withdrawal_idx: usize,
        bridge_fund_txid: Txid,
        withdrawal_address: &Address<NetworkChecked>,
    ) -> Result<schnorr::Signature, BridgeError> {
        // TODO: Check Citrea RPC if the withdrawal is already been made or not.

        if let Ok((db_bridge_fund_txid, sig)) =
            self.db.get_withdrawal_sig_by_idx(withdrawal_idx).await
        {
            if db_bridge_fund_txid == bridge_fund_txid {
                return Ok(sig);
            } else {
                return Err(BridgeError::AlreadySpentWithdrawal);
            }
        };

        tracing::info!(
            "Verifier is signing withdrawal transaction with TXID: {:?}",
            bridge_fund_txid
        );

        let bridge_utxo = OutPoint {
            txid: bridge_fund_txid,
            vout: 0,
        };

        let (bridge_address, _) = self.transaction_builder.generate_bridge_address()?;

        let dust_value = script_builder::anyone_can_spend_txout().value;
        let bridge_txout = TxOut {
            value: Amount::from_sat(BRIDGE_AMOUNT_SATS - self.min_relay_fee) - dust_value,
            script_pubkey: bridge_address.script_pubkey(),
        };

        let mut withdrawal_tx = self.transaction_builder.create_withdraw_tx(
            bridge_utxo,
            bridge_txout,
            withdrawal_address,
        )?;

        let sig = self
            .signer
            .sign_taproot_script_spend_tx_new(&mut withdrawal_tx, 0, 0)?;

        self.db
            .save_withdrawal_sig(withdrawal_idx, bridge_fund_txid, sig)
            .await?;

        Ok(sig)
    }
}

#[async_trait]
impl<R> VerifierRpcServer for Verifier<R>
where
    R: RpcApiWrapper,
{
    async fn new_deposit_rpc(
        &self,
        start_utxo: OutPoint,
        recovery_taproot_address: Address<NetworkUnchecked>,
        deposit_index: u32,
        evm_address: EVMAddress,
        operator_address: Address<NetworkUnchecked>,
    ) -> Result<DepositPresigns, BridgeError> {
        let operator_address = operator_address.require_network(self.network)?;

        self.new_deposit(
            start_utxo,
            &recovery_taproot_address,
            deposit_index,
            &evm_address,
            &operator_address,
        )
        .await
    }

    async fn new_withdrawal_direct_rpc(
        &self,
        withdrawal_idx: usize,
        bridge_fund_txid: Txid,
        withdrawal_address: Address<NetworkUnchecked>,
    ) -> Result<schnorr::Signature, BridgeError> {
        let withdrawal_address = withdrawal_address.require_network(self.network)?;

        self.new_withdrawal_direct(withdrawal_idx, bridge_fund_txid, &withdrawal_address)
            .await
    }
}

#[cfg(feature = "poc")]
impl Verifier {
    /// TODO: Add verification for the connector tree hashes
    fn connector_roots_created(
        &self,
        connector_tree_hashes: &Vec<HashTree>,
        first_source_utxo: &OutPoint,
        start_blockheight: u64,
        period_relative_block_heights: Vec<u32>,
    ) -> Result<(), BridgeError> {
        let (_claim_proof_merkle_roots, _, _utxo_trees, _claim_proof_merkle_trees) =
            self.transaction_builder.create_all_connector_trees(
                &connector_tree_hashes,
                &first_source_utxo,
                start_blockheight,
                &period_relative_block_heights,
            )?;

        self.db.set_connector_tree_utxos(utxo_trees);
        self.db
            .set_connector_tree_hashes(connector_tree_hashes.clone());
        self.db
            .set_claim_proof_merkle_trees(claim_proof_merkle_trees);
        self.db.set_start_block_height(start_blockheight);
        self.db
            .set_period_relative_block_heights(period_relative_block_heights);

        Ok(())
    }

    /// Challenges the operator for current period for now
    /// Will return the blockhash, total work, and period
    fn challenge_operator(&self, period: u8) -> Result<VerifierChallenge, BridgeError> {
        tracing::info!("Verifier starts challenges");
        let last_blockheight = self.rpc.get_block_count()?;
        let last_blockhash = self.rpc.get_block_hash(
            self.db.get_start_block_height()
                + self.db.get_period_relative_block_heights()[period as usize] as u64
                - 1,
        )?;
        tracing::debug!("Verifier last_blockhash: {:?}", last_blockhash);
        let total_work = self.rpc.calculate_total_work_between_blocks(
            self.db.get_start_block_height(),
            last_blockheight,
        )?;
        Ok((last_blockhash, total_work, period))
    }
}
