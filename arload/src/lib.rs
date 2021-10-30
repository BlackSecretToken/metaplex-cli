#![feature(derive_default_enum)]

use crate::error::ArweaveError;
use async_trait::async_trait;
use blake3;
use chrono::Utc;
use futures::{future::try_join_all, stream, Stream, StreamExt};
use infer;
use log::debug;
use num_bigint::BigUint;
use reqwest::{
    self,
    header::{ACCEPT, CONTENT_TYPE},
    StatusCode as ResponseStatusCode,
};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, str::FromStr};
use tokio::fs;
use url::Url;

pub mod crypto;
pub mod error;
pub mod merkle;
pub mod status;
pub mod transaction;
pub mod utils;

use crypto::Methods as CryptoMethods;
use merkle::{generate_data_root, generate_leaves, resolve_proofs};
use status::{Status, StatusCode};
use transaction::{Base64, FromStrs, Tag, Transaction};

pub type Error = ArweaveError;

pub const WINSTONS_PER_AR: u64 = 1000000000000;

pub struct Arweave {
    pub name: String,
    pub units: String,
    pub base_url: Url,
    pub crypto: crypto::Provider,
}

pub fn upload_files_stream<'a, IP>(
    arweave: &'a Arweave,
    paths_iter: IP,
    log_dir: Option<PathBuf>,
    last_tx: Option<Base64>,
    reward: Option<u64>,
    buffer: usize,
) -> impl Stream<Item = Result<Status, Error>> + 'a
where
    IP: Iterator<Item = PathBuf> + Send + Sync + 'a,
{
    stream::iter(paths_iter)
        .map(move |p| {
            arweave.upload_file_from_path(p, log_dir.clone(), None, last_tx.clone(), reward)
        })
        .buffer_unordered(buffer)
}

#[derive(Serialize, Deserialize, Debug)]
struct OraclePrice {
    pub arweave: OraclePricePair,
}

#[derive(Serialize, Deserialize, Debug)]
struct OraclePricePair {
    pub usd: f32,
}

#[async_trait]
pub trait Methods<T> {
    async fn from_keypair_path(keypair_path: PathBuf, base_url: Option<&str>) -> Result<T, Error>;

    async fn get_wallet_balance(&self, wallet_address: Option<String>) -> Result<BigUint, Error>;

    async fn get_price(&self, bytes: &usize) -> Result<(BigUint, BigUint), Error>;

    async fn get_transaction(&self, id: &Base64) -> Result<Transaction, Error>;

    async fn create_transaction_from_file_path(
        &self,
        file_path: PathBuf,
        additional_tags: Option<Vec<Tag>>,
        last_tx: Option<Base64>,
        reward: Option<u64>,
    ) -> Result<Transaction, Error>;

    fn sign_transaction(&self, transaction: Transaction) -> Result<Transaction, Error>;

    async fn post_transaction(
        &self,
        signed_transaction: &Transaction,
        file_path: Option<PathBuf>,
    ) -> Result<Status, Error>;

    async fn get_raw_status(&self, id: &Base64) -> Result<reqwest::Response, Error>;

    async fn write_status(&self, mut status: Status, log_dir: PathBuf) -> Result<(), Error>;

    async fn read_status(&self, file_path: PathBuf, log_dir: PathBuf) -> Result<Status, Error>;

    async fn read_statuses<IP>(
        &self,
        paths_iter: IP,
        log_dir: PathBuf,
    ) -> Result<Vec<Status>, Error>
    where
        IP: Iterator<Item = PathBuf> + Send;

    async fn update_status(&self, file_path: PathBuf, log_dir: PathBuf) -> Result<Status, Error>;

    async fn update_statuses<IP>(
        &self,
        paths_iter: IP,
        log_dir: PathBuf,
    ) -> Result<Vec<Status>, Error>
    where
        IP: Iterator<Item = PathBuf> + Send;

    async fn upload_file_from_path(
        &self,
        file_path: PathBuf,
        log_dir: Option<PathBuf>,
        additional_tags: Option<Vec<Tag>>,
        last_tx: Option<Base64>,
        reward: Option<u64>,
    ) -> Result<Status, Error>;

    async fn upload_files_from_paths<IP, IT>(
        &self,
        paths_iter: IP,
        log_dir: Option<PathBuf>,
        tags_iter: Option<IT>,
        last_tx: Option<Base64>,
        reward: Option<u64>,
    ) -> Result<Vec<Status>, Error>
    where
        IP: Iterator<Item = PathBuf> + Send,
        IT: Iterator<Item = Option<Vec<Tag>>> + Send;

    async fn filter_statuses<IP>(
        &self,
        paths_iter: IP,
        log_dir: PathBuf,
        status: StatusCode,
        min_confirms: Option<u64>,
    ) -> Result<Vec<Status>, Error>
    where
        IP: Iterator<Item = PathBuf> + Send;
}

#[async_trait]
impl Methods<Arweave> for Arweave {
    async fn from_keypair_path(
        keypair_path: PathBuf,
        base_url: Option<&str>,
    ) -> Result<Arweave, Error> {
        Ok(Arweave {
            name: String::from("arweave"),
            units: String::from("winstons"),
            base_url: Url::parse(base_url.unwrap_or("https://arweave.net/"))?,
            crypto: crypto::Provider::from_keypair_path(keypair_path).await?,
        })
    }

    /// Returns the balance of the wallet.
    async fn get_wallet_balance(&self, wallet_address: Option<String>) -> Result<BigUint, Error> {
        let wallet_address = if let Some(wallet_address) = wallet_address {
            wallet_address
        } else {
            self.crypto.wallet_address()?.to_string()
        };
        let url = self
            .base_url
            .join(&format!("wallet/{}/balance", &wallet_address))?;
        let winstons = reqwest::get(url).await?.json::<u64>().await?;
        Ok(BigUint::from(winstons))
    }

    /// Returns price of uploading data to the network in winstons and usd per AR
    /// as a BigUint with two decimals.
    async fn get_price(&self, bytes: &usize) -> Result<(BigUint, BigUint), Error> {
        let url = self.base_url.join("price/")?.join(&bytes.to_string())?;
        let winstons_per_bytes = reqwest::get(url).await?.json::<u64>().await?;
        let winstons_per_bytes = BigUint::from(winstons_per_bytes);
        let oracle_url =
            "https://api.coingecko.com/api/v3/simple/price?ids=arweave&vs_currencies=usd";
        let usd_per_ar = reqwest::get(oracle_url)
            .await?
            .json::<OraclePrice>()
            .await?
            .arweave
            .usd;

        let usd_per_ar: BigUint = BigUint::from((usd_per_ar * 100.0).floor() as u32);

        Ok((winstons_per_bytes, usd_per_ar))
    }
    async fn get_transaction(&self, id: &Base64) -> Result<Transaction, Error> {
        let url = self.base_url.join("tx/")?.join(&id.to_string())?;
        let resp = reqwest::get(url).await?.json::<Transaction>().await?;
        Ok(resp)
    }

    async fn create_transaction_from_file_path(
        &self,
        file_path: PathBuf,
        other_tags: Option<Vec<Tag>>,
        last_tx: Option<Base64>,
        reward: Option<u64>,
    ) -> Result<Transaction, Error> {
        let data = fs::read(file_path).await?;
        let chunks = generate_leaves(data.clone(), &self.crypto)?;
        let root = generate_data_root(chunks.clone(), &self.crypto)?;
        let data_root = Base64(root.id.clone().into_iter().collect());
        let proofs = resolve_proofs(root, None)?;
        let owner = self.crypto.keypair_modulus()?;

        // Get content type from [magic numbers](https://developer.mozilla.org/en-US/docs/Web/HTTP/Basics_of_HTTP/MIME_types)
        // and include additional tags if any.
        let content_type = if let Some(kind) = infer::get(&data) {
            kind.mime_type()
        } else {
            "application/json"
        };
        let mut tags = vec![Tag::from_utf8_strs("Content-Type", content_type)?];

        // Add other tags if provided.
        if let Some(other_tags) = other_tags {
            tags.extend(other_tags);
        }

        // Fetch and set last_tx if not provided (primarily for testing).
        let last_tx = if let Some(last_tx) = last_tx {
            last_tx
        } else {
            let last_tx_str = reqwest::get(self.base_url.join("tx_anchor")?)
                .await?
                .text()
                .await?;
            Base64::from_str(&last_tx_str)?
        };

        // Fetch and set reward if not provided (primarily for testing).
        let reward = reward.unwrap_or({
            let (winstons_per_bytes, _) = self.get_price(&data.len()).await?;
            winstons_per_bytes.to_u64_digits()[0]
        });

        Ok(Transaction {
            format: 2,
            data_size: data.len() as u64,
            data: Base64(data),
            data_root,
            tags,
            reward,
            owner,
            last_tx,
            chunks,
            proofs,
            ..Default::default()
        })
    }

    /// Gets deep hash, signs and sets signature and id.
    fn sign_transaction(&self, mut transaction: Transaction) -> Result<Transaction, Error> {
        let deep_hash = self.crypto.deep_hash(&transaction)?;
        let signature = self.crypto.sign(&deep_hash)?;
        let id = self.crypto.hash_SHA256(&signature)?;
        transaction.signature = Base64(signature);
        transaction.id = Base64(id.to_vec());
        Ok(transaction)
    }

    async fn post_transaction(
        &self,
        signed_transaction: &Transaction,
        file_path: Option<PathBuf>,
    ) -> Result<Status, Error> {
        if signed_transaction.id.0.is_empty() {
            return Err(error::ArweaveError::UnsignedTransaction.into());
        }

        let url = self.base_url.join("tx/")?;
        let client = reqwest::Client::new();
        let resp = client
            .post(url)
            .json(&signed_transaction)
            .header(&ACCEPT, "application/json")
            .header(&CONTENT_TYPE, "application/json")
            .send()
            .await?;
        debug!("post_transaction {:?}", &resp);
        assert_eq!(resp.status().as_u16(), 200);

        let status = Status {
            id: signed_transaction.id.clone(),
            reward: signed_transaction.reward,
            file_path,
            ..Default::default()
        };

        Ok(status)
    }

    async fn get_raw_status(&self, id: &Base64) -> Result<reqwest::Response, Error> {
        let url = self.base_url.join(&format!("tx/{}/status", id))?;
        let resp = reqwest::get(url).await?;
        Ok(resp)
    }

    /// Writes Status Json to `log_dir` with file name based on BLAKE3 hash of `status.file_path`.
    ///
    /// This is done to facilitate checking the status of uploaded file and also means that only
    /// one status object can exist for a given `file_path`. If for some reason you wanted to record
    /// statuses for multiple uploads of the same file you can provide a different log_dir (or copy the
    /// file to a different directory and upload from there).
    async fn write_status(&self, status: Status, log_dir: PathBuf) -> Result<(), Error> {
        if let Some(file_path) = &status.file_path {
            if status.id.0.is_empty() {
                return Err(error::ArweaveError::UnsignedTransaction.into());
            }
            let file_path_hash = blake3::hash(file_path.to_str().unwrap().as_bytes());
            fs::write(
                log_dir
                    .join(file_path_hash.to_string())
                    .with_extension("json"),
                serde_json::to_string(&status)?,
            )
            .await?;
            Ok(())
        } else {
            Err(error::ArweaveError::MissingFilePath)
        }
    }

    async fn read_status(&self, file_path: PathBuf, log_dir: PathBuf) -> Result<Status, Error> {
        let file_path_hash = blake3::hash(file_path.to_str().unwrap().as_bytes());

        let status_path = log_dir
            .join(file_path_hash.to_string())
            .with_extension("json");

        if status_path.exists() {
            let data = fs::read_to_string(status_path).await?;
            let status: Status = serde_json::from_str(&data)?;
            Ok(status)
        } else {
            Err(ArweaveError::StatusNotFound)
        }
    }

    async fn read_statuses<IP>(
        &self,
        paths_iter: IP,
        log_dir: PathBuf,
    ) -> Result<Vec<Status>, Error>
    where
        IP: Iterator<Item = PathBuf> + Send,
    {
        try_join_all(paths_iter.map(|p| self.read_status(p, log_dir.clone()))).await
    }

    async fn update_status(&self, file_path: PathBuf, log_dir: PathBuf) -> Result<Status, Error> {
        let mut status = self.read_status(file_path, log_dir.clone()).await?;
        let resp = self.get_raw_status(&status.id).await?;
        status.last_modified = Utc::now();
        match resp.status() {
            ResponseStatusCode::OK => {
                let resp_string = resp.text().await?;
                if &resp_string == &String::from("Pending") {
                    status.status = StatusCode::Pending;
                } else {
                    status.raw_status = Some(serde_json::from_str(&resp_string)?);
                    status.status = StatusCode::Confirmed;
                }
            }
            ResponseStatusCode::NOT_FOUND => {
                status.status = StatusCode::NotFound;
            }
            _ => unreachable!(),
        }
        self.write_status(status.clone(), log_dir).await?;
        Ok(status)
    }

    async fn update_statuses<IP>(
        &self,
        paths_iter: IP,
        log_dir: PathBuf,
    ) -> Result<Vec<Status>, Error>
    where
        IP: Iterator<Item = PathBuf> + Send,
    {
        try_join_all(paths_iter.map(|p| self.update_status(p, log_dir.clone()))).await
    }

    async fn upload_file_from_path(
        &self,
        file_path: PathBuf,
        log_dir: Option<PathBuf>,
        additional_tags: Option<Vec<Tag>>,
        last_tx: Option<Base64>,
        reward: Option<u64>,
    ) -> Result<Status, Error> {
        let transaction = self
            .create_transaction_from_file_path(file_path.clone(), additional_tags, last_tx, reward)
            .await?;
        let signed_transaction = self.sign_transaction(transaction)?;
        let status = self
            .post_transaction(&signed_transaction, Some(file_path))
            .await?;

        if let Some(log_dir) = log_dir {
            self.write_status(status.clone(), log_dir).await?;
        }
        Ok(status)
    }

    /// Uploads files from an iterator of paths.
    ///
    /// Optionally logs Status objects to `log_dir`, if provided and optionally adds tags to each
    ///  transaction from an iterator of tags that must be the same size as the paths iterator.
    async fn upload_files_from_paths<IP, IT>(
        &self,
        paths_iter: IP,
        log_dir: Option<PathBuf>,
        tags_iter: Option<IT>,
        last_tx: Option<Base64>,
        reward: Option<u64>,
    ) -> Result<Vec<Status>, Error>
    where
        IP: Iterator<Item = PathBuf> + Send,
        IT: Iterator<Item = Option<Vec<Tag>>> + Send,
    {
        let statuses = if let Some(tags_iter) = tags_iter {
            try_join_all(paths_iter.zip(tags_iter).map(|(p, t)| {
                self.upload_file_from_path(p, log_dir.clone(), t, last_tx.clone(), reward)
            }))
        } else {
            try_join_all(paths_iter.map(|p| {
                self.upload_file_from_path(p, log_dir.clone(), None, last_tx.clone(), reward)
            }))
        }
        .await?;
        Ok(statuses)
    }

    /// Filters saved Status objects by status and number of confirmations.
    ///
    /// If there is no raw status object and min_confirms is passed, it
    /// assumes there are zero confirms. This is designed to be used to
    /// confirm all files have a confirmed status and to collect the
    /// paths of the files that need to be re-uploaded.
    async fn filter_statuses<IP>(
        &self,
        paths_iter: IP,
        log_dir: PathBuf,
        status: StatusCode,
        min_confirms: Option<u64>,
    ) -> Result<Vec<Status>, Error>
    where
        IP: Iterator<Item = PathBuf> + Send,
    {
        let all_statuses = self.read_statuses(paths_iter, log_dir).await?;
        let filtered = all_statuses
            .into_iter()
            .filter(|s| {
                if let Some(min_confirms) = min_confirms {
                    let confirms = if let Some(raw_status) = &s.raw_status {
                        raw_status.number_of_confirmations
                    } else {
                        0
                    };
                    (s.status == status) & (confirms >= min_confirms)
                } else {
                    s.status == status
                }
            })
            .collect();
        Ok(filtered)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        error::ArweaveError,
        transaction::{Base64, FromStrs, Tag},
        utils::{TempDir, TempFrom},
        Arweave, Methods as ArewaveMethods, Status,
    };
    use matches::assert_matches;
    use std::{path::PathBuf, str::FromStr};
    pub type Error = ArweaveError;

    #[tokio::test]
    async fn test_cannot_post_unsigned_transaction() -> Result<(), Error> {
        let arweave = Arweave::from_keypair_path(
            PathBuf::from(
                "tests/fixtures/arweave-key-7eV1qae4qVNqsNChg3Scdi-DpOLJPCogct4ixoq1WNg.json",
            ),
            None,
        )
        .await?;

        let file_path = PathBuf::from("tests/fixtures/0.png");
        let last_tx = Base64::from_str("LCwsLCwsLA")?;
        let other_tags = vec![Tag::from_utf8_strs("key2", "value2")?];
        let transaction = arweave
            .create_transaction_from_file_path(file_path, Some(other_tags), Some(last_tx), Some(0))
            .await?;

        let error = arweave
            .post_transaction(&transaction, None)
            .await
            .unwrap_err();
        assert_matches!(error, ArweaveError::UnsignedTransaction);

        Ok(())
    }

    #[tokio::test]
    async fn test_create_write_read_status() -> Result<(), Error> {
        let arweave = Arweave::from_keypair_path(
            PathBuf::from(
                "tests/fixtures/arweave-key-7eV1qae4qVNqsNChg3Scdi-DpOLJPCogct4ixoq1WNg.json",
            ),
            None,
        )
        .await?;

        let file_path = PathBuf::from("tests/fixtures/0.png");
        let last_tx = Base64::from_str("LCwsLCwsLA")?;
        let other_tags = vec![Tag::from_utf8_strs("key2", "value2")?];
        let transaction = arweave
            .create_transaction_from_file_path(
                file_path.clone(),
                Some(other_tags),
                Some(last_tx),
                Some(0),
            )
            .await?;

        let signed_transaction = arweave.sign_transaction(transaction)?;

        let status = Status {
            id: signed_transaction.id.clone(),
            reward: signed_transaction.reward,
            file_path: Some(file_path.clone()),
            ..Default::default()
        };

        let temp_log_dir = TempDir::from_str("../target/tmp/").await?;
        let log_dir = temp_log_dir.0.clone();

        arweave
            .write_status(status.clone(), log_dir.clone())
            .await?;

        let read_status = arweave.read_status(file_path, log_dir).await?;

        assert_eq!(status, read_status);

        Ok(())
    }
}
