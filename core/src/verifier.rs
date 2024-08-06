use crate::actor::Actor;
use crate::config::BridgeConfig;
use crate::database::verifier::VerifierDB;
use crate::errors::BridgeError;
use crate::extended_rpc::ExtendedRpc;
use crate::musig::{self, MusigAggNonce, MusigPartialSignature, MusigPubNonce};
use crate::traits::rpc::VerifierRpcServer;
use crate::transaction_builder::TransactionBuilder;
use crate::{script_builder, utils, EVMAddress, PsbtOutPoint};
use bitcoin::address::NetworkUnchecked;
use bitcoin::hashes::Hash;
use bitcoin::sighash::{self};
use bitcoin::{secp256k1, secp256k1::Secp256k1, OutPoint};
use bitcoin::{taproot, Address, Amount, Network, TxOut};
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

        let transaction = self.db.begin_transaction().await?;
        self.db
            .save_deposit_info(deposit_utxo, recovery_taproot_address, evm_address)
            .await?;
        self.db.save_nonces(deposit_utxo, &nonces).await?;
        transaction.commit().await?;

        let pub_nonces = nonces.iter().map(|(pub_nonce, _)| *pub_nonce).collect();

        Ok(pub_nonces)
    }

    /// - Verify operators signatures about kickoffs
    /// - Check the kickoff_utxos
    /// - Save agg_nonces to a db for future use
    /// - for every kickoff_utxo, calculate kickoff2_tx
    /// - for every kickoff2_tx, partial sign burn_tx (ommitted for now)
    /// - return MusigPartialSignature of sign(kickoff2_txids)
    async fn operator_kickoffs_generated(
        &self,
        deposit_utxo: &OutPoint,
        kickoff_utxos: Vec<PsbtOutPoint>,
        operators_kickoff_sigs: Vec<secp256k1::schnorr::Signature>,
        agg_nonces: Vec<MusigAggNonce>,
    ) -> Result<Vec<MusigPartialSignature>, BridgeError> {
        if operators_kickoff_sigs.len() != kickoff_utxos.len() {
            return Err(BridgeError::InvalidKickoffUtxo);
        }

        for (i, kickoff_utxo) in kickoff_utxos.iter().enumerate() {
            let value = kickoff_utxo.tx.output[kickoff_utxo.vout as usize].value;
            if value.to_sat() < 100_000 {
                return Err(BridgeError::InvalidKickoffUtxo);
            }

            let kickoff_sig_hash = sha256_hash!(
                deposit_utxo.txid,
                deposit_utxo.vout.to_be_bytes(),
                kickoff_utxo.tx.compute_txid(),
                kickoff_utxo.vout.to_be_bytes()
            );

            utils::SECP.verify_schnorr(
                &operators_kickoff_sigs[i],
                &secp256k1::Message::from_digest(kickoff_sig_hash),
                &self.signer.xonly_public_key, // TOOD: Fix this to correct operator
            )?;
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

        self.db.save_agg_nonces(deposit_utxo, &agg_nonces).await?;

        self.db
            .save_kickoff_outpoints_and_amounts(deposit_utxo, &kickoff_outpoints_and_amounts)
            .await?;

        // TODO: Sign burn txs
        Ok(vec![])
    }

    /// verify burn txs are signed by verifiers
    /// sign operator_takes_txs
    async fn burn_txs_signed_rpc(
        &self,
        deposit_utxo: &OutPoint,
        _burn_sigs: Vec<schnorr::Signature>,
    ) -> Result<Vec<MusigPartialSignature>, BridgeError> {
        // TODO: Verify burn txs are signed by verifiers

        let kickoff_outpoints_and_amounts = self
            .db
            .get_kickoff_outpoints_and_amounts(deposit_utxo)
            .await?;

        let kickoff_outpoints_and_amounts =
            kickoff_outpoints_and_amounts.ok_or(BridgeError::KickoffOutpointsNotFound)?;

        let future_nonces = (0..kickoff_outpoints_and_amounts.len())
            .map(|i| self.db.get_nonces(&deposit_utxo, i + 2)); // i + 2 is bcs we used the first two nonce for move_txs

        let nonces = futures::future::try_join_all(future_nonces)
            .await?
            .into_iter()
            .map(|opt| opt.ok_or(BridgeError::NoncesNotFound))
            .collect::<Result<Vec<_>, _>>()?;

        let operator_takes_partial_sigs = kickoff_outpoints_and_amounts
            .iter()
            .enumerate()
            .map(|(index, (kickoff_outpoint, kickoff_amount))| {
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

                let bridge_txout = TxOut {
                    value: Amount::from_sat(BRIDGE_AMOUNT_SATS - self.min_relay_fee - 330),
                    script_pubkey: self.signer.address.script_pubkey(), // TODO: Fix this to N-of-N
                };
                let kickoff_txout = TxOut {
                    value: *kickoff_amount,
                    script_pubkey: self.signer.address.script_pubkey(), // TODO: Fix this address to operator or 200 blocks N-of-N
                };

                let prevouts = vec![bridge_txout, kickoff_txout];

                let musig_script =
                    script_builder::generate_script_n_of_n(&vec![self.signer.xonly_public_key]); // TODO: Fix this to N-of-N musig

                let mut sighash_cache = sighash::SighashCache::new(tx);
                let sig_hash = sighash_cache
                    .taproot_script_spend_signature_hash(
                        0,
                        &bitcoin::sighash::Prevouts::All(&prevouts),
                        bitcoin::TapLeafHash::from_script(
                            &musig_script,
                            taproot::LeafVersion::TapScript,
                        ),
                        sighash::TapSighashType::Default,
                    )
                    .unwrap(); // Is unwrap safe here?

                let (operator_takes_partial_sig, _) = musig::partial_sign(
                    vec![],
                    nonces[index].2,
                    &self.signer.keypair,
                    nonces[index].1,
                    sig_hash.to_byte_array(),
                    None,
                    None,
                );
                operator_takes_partial_sig as MusigPartialSignature
            })
            .collect::<Vec<_>>();

        Ok(operator_takes_partial_sigs)
    }

    /// verify the operator_take_sigs
    /// sign move_commit_tx and move_reveal_tx
    async fn operator_take_txs_signed_rpc(
        &self,
        deposit_utxo: &OutPoint,
        operator_take_sigs: Vec<schnorr::Signature>,
    ) -> Result<(MusigPartialSignature, MusigPartialSignature), BridgeError> {
        let kickoff_outpoints_and_amounts = self
            .db
            .get_kickoff_outpoints_and_amounts(deposit_utxo)
            .await?;

        let kickoff_outpoints_and_amounts =
            kickoff_outpoints_and_amounts.ok_or(BridgeError::KickoffOutpointsNotFound)?;

        kickoff_outpoints_and_amounts.iter().enumerate().map(
            |(index, (kickoff_outpoint, kickoff_amount))| {
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

                let bridge_txout = TxOut {
                    value: Amount::from_sat(BRIDGE_AMOUNT_SATS - self.min_relay_fee - 330),
                    script_pubkey: self.signer.address.script_pubkey(), // TODO: Fix this to N-of-N
                };
                let kickoff_txout = TxOut {
                    value: *kickoff_amount,
                    script_pubkey: self.signer.address.script_pubkey(), // TODO: Fix this address to operator or 200 blocks N-of-N
                };

                let prevouts = vec![bridge_txout, kickoff_txout];

                let musig_script =
                    script_builder::generate_script_n_of_n(&vec![self.signer.xonly_public_key]); // TODO: Fix this to N-of-N musig

                let mut sighash_cache = sighash::SighashCache::new(tx);
                let sig_hash = sighash_cache
                    .taproot_script_spend_signature_hash(
                        0,
                        &bitcoin::sighash::Prevouts::All(&prevouts),
                        bitcoin::TapLeafHash::from_script(
                            &musig_script,
                            taproot::LeafVersion::TapScript,
                        ),
                        sighash::TapSighashType::Default,
                    )
                    .unwrap(); // Is unwrap safe here?

                // verify tjhe operator_take_sigs
                utils::SECP
                    .verify_schnorr(
                        &operator_take_sigs[index],
                        &secp256k1::Message::from_digest(sig_hash.to_byte_array()),
                        &self.signer.xonly_public_key, // TOOD: Fix this to N-of-N pubkey
                    )
                    .unwrap();
            },
        );

        let (recovery_taproot_address, evm_address) = self
            .db
            .get_deposit_info(deposit_utxo)
            .await?
            .ok_or(BridgeError::DepositInfoNotFound)?;

        // TODO: Sign move_commit_tx and move_reveal_tx, move_commit_tx will commit to kickoff txs
        let move_commit_tx = 0;
        let move_reveal_tx = 0;

        Ok((
            [0u8; 32] as MusigPartialSignature,
            [0u8; 32] as MusigPartialSignature,
        ))
    }
}

#[async_trait]
impl<R> VerifierRpcServer for Verifier<R> where R: RpcApiWrapper {}
