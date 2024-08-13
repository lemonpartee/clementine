use std::str::FromStr;

use bitcoin::{address::NetworkUnchecked, Address, OutPoint, Txid};
use serde::{Deserialize, Serialize};
use sqlx::{
    postgres::{PgArgumentBuffer, PgRow, PgValueRef},
    Decode, Encode, FromRow, Postgres, Row,
};

use crate::{EVMAddress, PsbtOutPoint};

#[derive(Serialize, Deserialize)]
pub struct OutPointDB(pub OutPoint);

#[derive(Serialize)]
pub struct AddressDB(pub Address<NetworkUnchecked>);

#[derive(Serialize, Deserialize)]
pub struct EVMAddressDB(pub EVMAddress);

#[derive(Serialize, Deserialize)]
pub struct TxidDB(pub Txid);

#[derive(Serialize, Deserialize)]
pub struct SignatureDB(pub secp256k1::schnorr::Signature);

impl SignatureDB {
    fn decode_signature_from_row(row: &PgRow, column_name: &str) -> Result<secp256k1::schnorr::Signature, sqlx::Error> {
        let s = row.try_get_raw(column_name).map_err(|_| sqlx::Error::ColumnNotFound(column_name.into()))?;
        let str: &str = Decode::decode(s).map_err(|_| sqlx::Error::ColumnDecode {
            index: column_name.into(),
            source: Box::new(sqlx::Error::Decode("Invalid Signature".into())),
        })?;
        let res = secp256k1::schnorr::Signature::from_str(str).map_err(|_| sqlx::Error::ColumnDecode {
            index: column_name.into(),
            source: Box::new(sqlx::Error::Decode("Invalid Signature".into())),
        })?;
        Ok(res)
    }
}

#[derive(Serialize, Deserialize)]
pub struct PsbtOutPointDB(pub PsbtOutPoint);

// Implement sqlx::Type manually if needed
impl sqlx::Type<sqlx::Postgres> for OutPointDB {
    fn type_info() -> sqlx::postgres::PgTypeInfo {
        sqlx::postgres::PgTypeInfo::with_name("TEXT")
    }
}

impl<'q> Encode<'q, Postgres> for OutPointDB {
    fn encode_by_ref(&self, buf: &mut PgArgumentBuffer) -> sqlx::encode::IsNull {
        // Encode as &str
        let s = self.0.to_string();
        <&str as Encode<Postgres>>::encode_by_ref(&s.as_str(), buf)
    }

    fn encode(
        self,
        buf: &mut <Postgres as sqlx::database::HasArguments<'q>>::ArgumentBuffer,
    ) -> sqlx::encode::IsNull
    where
        Self: Sized,
    {
        self.encode_by_ref(buf)
    }

    fn produces(&self) -> Option<<Postgres as sqlx::Database>::TypeInfo> {
        // `produces` is inherently a hook to allow database drivers to produce value-dependent
        // type information; if the driver doesn't need this, it can leave this as `None`
        None
    }

    fn size_hint(&self) -> usize {
        std::mem::size_of_val(self)
    }
}

impl<'r> Decode<'r, Postgres> for OutPointDB {
    fn decode(value: PgValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let s = <&str as Decode<Postgres>>::decode(value)?;
        Ok(OutPointDB(OutPoint::from_str(s)?)) // Assuming ExternalOutPoint has a from_string method
    }
}

impl sqlx::Type<sqlx::Postgres> for AddressDB {
    fn type_info() -> sqlx::postgres::PgTypeInfo {
        sqlx::postgres::PgTypeInfo::with_name("TEXT")
    }
}

impl<'q> Encode<'q, Postgres> for AddressDB {
    fn encode_by_ref(&self, buf: &mut PgArgumentBuffer) -> sqlx::encode::IsNull {
        let s = self.0.clone().assume_checked().to_string();
        <&str as Encode<Postgres>>::encode_by_ref(&s.as_str(), buf)
    }

    fn encode(
        self,
        buf: &mut <Postgres as sqlx::database::HasArguments<'q>>::ArgumentBuffer,
    ) -> sqlx::encode::IsNull
    where
        Self: Sized,
    {
        self.encode_by_ref(buf)
    }

    fn produces(&self) -> Option<<Postgres as sqlx::Database>::TypeInfo> {
        None
    }

    fn size_hint(&self) -> usize {
        std::mem::size_of_val(self)
    }
}

impl<'r> Decode<'r, Postgres> for AddressDB {
    fn decode(value: PgValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let s = <&str as Decode<Postgres>>::decode(value)?;
        Ok(AddressDB(Address::from_str(s)?)) // Assuming ExternalOutPoint has a from_string method
    }
}

impl sqlx::Type<sqlx::Postgres> for EVMAddressDB {
    fn type_info() -> sqlx::postgres::PgTypeInfo {
        sqlx::postgres::PgTypeInfo::with_name("TEXT")
    }
}

impl<'q> Encode<'q, Postgres> for EVMAddressDB {
    fn encode_by_ref(&self, buf: &mut PgArgumentBuffer) -> sqlx::encode::IsNull {
        let s = hex::encode(self.0 .0);
        <&str as Encode<Postgres>>::encode_by_ref(&s.as_str(), buf)
    }

    fn encode(
        self,
        buf: &mut <Postgres as sqlx::database::HasArguments<'q>>::ArgumentBuffer,
    ) -> sqlx::encode::IsNull
    where
        Self: Sized,
    {
        self.encode_by_ref(buf)
    }

    fn produces(&self) -> Option<<Postgres as sqlx::Database>::TypeInfo> {
        None
    }

    fn size_hint(&self) -> usize {
        std::mem::size_of_val(self)
    }
}

impl<'r> Decode<'r, Postgres> for EVMAddressDB {
    fn decode(value: PgValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let s = <&str as Decode<Postgres>>::decode(value)?;
        Ok(EVMAddressDB(EVMAddress(
            hex::decode(s).unwrap().try_into().unwrap(),
        )))
    }
}

impl sqlx::Type<sqlx::Postgres> for TxidDB {
    fn type_info() -> sqlx::postgres::PgTypeInfo {
        sqlx::postgres::PgTypeInfo::with_name("TEXT")
    }
}

impl<'q> Encode<'q, Postgres> for TxidDB {
    fn encode_by_ref(&self, buf: &mut PgArgumentBuffer) -> sqlx::encode::IsNull {
        let s = hex::encode(self.0);
        <&str as Encode<Postgres>>::encode_by_ref(&s.as_str(), buf)
    }

    fn encode(
        self,
        buf: &mut <Postgres as sqlx::database::HasArguments<'q>>::ArgumentBuffer,
    ) -> sqlx::encode::IsNull
    where
        Self: Sized,
    {
        self.encode_by_ref(buf)
    }

    fn produces(&self) -> Option<<Postgres as sqlx::Database>::TypeInfo> {
        None
    }

    fn size_hint(&self) -> usize {
        std::mem::size_of_val(self)
    }
}

impl<'r> Decode<'r, Postgres> for TxidDB {
    fn decode(value: PgValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let s = <&str as Decode<Postgres>>::decode(value)?;
        Ok(TxidDB(Txid::from_str(s).unwrap()))
    }
}

impl<'r> FromRow<'r, PgRow> for TxidDB {
    fn from_row(row: &'r PgRow) -> Result<Self, sqlx::Error> {
        let s = row.try_get_raw("move_txid").unwrap();
        let str: &str = Decode::decode(s).map_err(|_| sqlx::Error::ColumnDecode {
            index: "move_txid".into(),
            source: Box::new(sqlx::Error::Decode("Invalid Txid".into())),
        })?;
        let res = Txid::from_str(str).map_err(|_| sqlx::Error::ColumnDecode {
            index: "move_txid".into(),
            source: Box::new(sqlx::Error::Decode("Invalid Txid".into())),
        })?;
        Ok(TxidDB(res))
    }
}

impl sqlx::Type<sqlx::Postgres> for SignatureDB {
    fn type_info() -> sqlx::postgres::PgTypeInfo {
        sqlx::postgres::PgTypeInfo::with_name("TEXT")
    }
}

impl<'q> Encode<'q, Postgres> for SignatureDB {
    fn encode_by_ref(&self, buf: &mut PgArgumentBuffer) -> sqlx::encode::IsNull {
        let s = hex::encode(self.0.as_ref());
        <&str as Encode<Postgres>>::encode_by_ref(&s.as_str(), buf)
    }

    fn encode(
        self,
        buf: &mut <Postgres as sqlx::database::HasArguments<'q>>::ArgumentBuffer,
    ) -> sqlx::encode::IsNull
    where
        Self: Sized,
    {
        self.encode_by_ref(buf)
    }

    fn produces(&self) -> Option<<Postgres as sqlx::Database>::TypeInfo> {
        None
    }

    fn size_hint(&self) -> usize {
        std::mem::size_of_val(self)
    }
}

impl<'r> Decode<'r, Postgres> for SignatureDB {
    fn decode(value: PgValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let s = <&str as Decode<Postgres>>::decode(value)?;
        Ok(SignatureDB(
            secp256k1::schnorr::Signature::from_str(s).unwrap(),
        ))
    }
}

impl<'r> FromRow<'r, PgRow> for SignatureDB {
    fn from_row(row: &'r PgRow) -> Result<Self, sqlx::Error> {
        // This assumes a default column name, adjust as necessary
        let signature = SignatureDB::decode_signature_from_row(row, "sig")?;
        Ok(SignatureDB(signature))
    }
}


impl sqlx::Type<sqlx::Postgres> for PsbtOutPointDB {
    fn type_info() -> sqlx::postgres::PgTypeInfo {
        sqlx::postgres::PgTypeInfo::with_name("TEXT")
    }
}

impl<'q> Encode<'q, Postgres> for PsbtOutPointDB {
    fn encode_by_ref(&self, buf: &mut PgArgumentBuffer) -> sqlx::encode::IsNull {
        let s = serde_json::to_string(&self.0).unwrap();
        <&str as Encode<Postgres>>::encode_by_ref(&s.as_str(), buf)
    }

    fn encode(
        self,
        buf: &mut <Postgres as sqlx::database::HasArguments<'q>>::ArgumentBuffer,
    ) -> sqlx::encode::IsNull
    where
        Self: Sized,
    {
        self.encode_by_ref(buf)
    }

    fn produces(&self) -> Option<<Postgres as sqlx::Database>::TypeInfo> {
        None
    }

    fn size_hint(&self) -> usize {
        std::mem::size_of_val(self)
    }
}

impl<'r> Decode<'r, Postgres> for PsbtOutPointDB {
    fn decode(value: PgValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let s = <&str as Decode<Postgres>>::decode(value)?;
        Ok(PsbtOutPointDB(serde_json::from_str(s).unwrap()))
    }
}

impl<'r> FromRow<'r, PgRow> for PsbtOutPointDB {
    fn from_row(row: &'r PgRow) -> Result<Self, sqlx::Error> {
        let s = row.try_get_raw("move_txid").unwrap();
        let str: &str = Decode::decode(s).map_err(|_| sqlx::Error::ColumnDecode {
            index: "move_txid".into(),
            source: Box::new(sqlx::Error::Decode("Invalid Txid".into())),
        })?;
        let res = serde_json::from_str(str).map_err(|_| sqlx::Error::ColumnDecode {
            index: "move_txid".into(),
            source: Box::new(sqlx::Error::Decode("Invalid Txid".into())),
        })?;
        Ok(PsbtOutPointDB(res))
    }
}