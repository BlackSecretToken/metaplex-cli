use async_trait::async_trait;
use jsonwebkey::JsonWebKey;
use log::debug;
use ring::{
    digest::{Context, SHA256, SHA384},
    rand,
    signature::{self, KeyPair, RsaKeyPair},
};
use tokio::{fs::File, io::AsyncReadExt};

type Error = Box<dyn std::error::Error>;
use crate::transaction::{Base64, Tag, ToSlices, Transaction};

pub struct Provider {
    pub keypair: RsaKeyPair,
}

#[async_trait]
pub trait Methods {
    async fn from_keypair_path(keypair_path: &str) -> Result<Provider, Error>;
    fn keypair_modulus(&self) -> Result<Base64, Error>;
    fn wallet_address(&self) -> Result<Base64, Error>;
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, Error>;
    fn verify(&self, signature: &[u8], message: &[u8]) -> Result<(), Error>;
    #[allow(non_snake_case)]
    fn hash_SHA256(&self, message: &[u8]) -> Result<[u8; 32], Error>;
    #[allow(non_snake_case)]
    fn hash_SHA384(&self, message: &[u8]) -> Result<[u8; 48], Error>;
    #[allow(non_snake_case)]
    #[allow(non_snake_case)]
    fn hash_all_SHA256(&self, messages: Vec<&[u8]>) -> Result<[u8; 32], Error>;
    #[allow(non_snake_case)]
    fn hash_all_SHA384(&self, messages: Vec<&[u8]>) -> Result<[u8; 48], Error>;
    fn concat_u8_48(&self, left: [u8; 48], right: [u8; 48]) -> Result<[u8; 96], Error>;
    fn deep_hash_list(
        &self,
        data_len: usize,
        data: Vec<&[u8]>,
        hash: Option<[u8; 48]>,
    ) -> Result<[u8; 48], Error>;
    fn deep_hash_tags(&self, tags: &Vec<Tag>) -> Result<[u8; 48], Error>;
    fn deep_hash(&self, transaction: &Transaction) -> Result<[u8; 48], Error>;
}

#[async_trait]
impl Methods for Provider {
    async fn from_keypair_path(keypair_path: &str) -> Result<Provider, Error> {
        debug!("{:?}", keypair_path);
        let mut file = File::open(keypair_path).await?;
        let mut jwk_str = String::new();
        file.read_to_string(&mut jwk_str).await?;
        let jwk_parsed: JsonWebKey = jwk_str.parse().unwrap();
        Ok(Self {
            keypair: signature::RsaKeyPair::from_pkcs8(&jwk_parsed.key.as_ref().to_der())?,
        })
    }
    fn keypair_modulus(&self) -> Result<Base64, Error> {
        let modulus = self
            .keypair
            .public_key()
            .modulus()
            .big_endian_without_leading_zero();
        Ok(Base64(modulus.to_vec()))
    }
    /// Calculates the wallet address of the provided keypair according to [addressing](https://docs.arweave.org/developers/server/http-api#addressing)
    /// in documentation.
    ///```
    /// # use arweave_rs::crypto::Methods as CryptoMethods;
    /// # use arweave_rs::{Arweave, Methods as ArweaveMethods};
    /// # use ring::{signature, rand};
    /// # use std::fmt::Display;
    /// #
    /// #
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let arweave = Arweave::from_keypair_path("tests/fixtures/arweave-key-7eV1qae4qVNqsNChg3Scdi-DpOLJPCogct4ixoq1WNg.json", None).await?;
    /// let calc = arweave.crypto.wallet_address()?;
    /// let actual = String::from("7eV1qae4qVNqsNChg3Scdi-DpOLJPCogct4ixoq1WNg");
    /// assert_eq!(&calc.to_string(), &actual);
    /// # Ok(())
    /// # }
    /// ```
    fn wallet_address(&self) -> Result<Base64, Error> {
        let mut context = Context::new(&SHA256);
        context.update(&self.keypair_modulus()?.0[..]);
        let wallet_address = Base64(context.finish().as_ref().to_vec());
        Ok(wallet_address)
    }

    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, Error> {
        let rng = rand::SystemRandom::new();
        let mut signature = vec![0; self.keypair.public_modulus_len()];
        self.keypair
            .sign(&signature::RSA_PSS_SHA256, &rng, message, &mut signature)?;
        Ok(signature)
    }

    /// Verifies that a message was signed by the public key of the Provider.key keypair.
    ///```
    /// # use ring::{signature, rand};
    /// # use arweave_rs::crypto::{Provider, Methods};
    /// #
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let crypto = Provider::from_keypair_path("tests/fixtures/arweave-key-7eV1qae4qVNqsNChg3Scdi-DpOLJPCogct4ixoq1WNg.json").await?;
    /// let message = String::from("hello, world");
    /// let rng = rand::SystemRandom::new();
    /// let signature = crypto.sign(&message.as_bytes())?;
    ///
    /// assert_eq!((), crypto.verify(&signature.as_ref(), &message.as_bytes())?);
    /// # Ok(())
    /// # }
    /// ```
    fn verify(&self, signature: &[u8], message: &[u8]) -> Result<(), Error> {
        let public_key = signature::UnparsedPublicKey::new(
            &signature::RSA_PSS_2048_8192_SHA256,
            self.keypair.public_key().as_ref(),
        );
        public_key.verify(message, signature)?;
        Ok(())
    }

    fn hash_SHA256(&self, message: &[u8]) -> Result<[u8; 32], Error> {
        let mut context = Context::new(&SHA256);
        context.update(message);
        let mut result: [u8; 32] = [0; 32];
        result.copy_from_slice(context.finish().as_ref());
        Ok(result)
    }

    fn hash_SHA384(&self, message: &[u8]) -> Result<[u8; 48], Error> {
        let mut context = Context::new(&SHA384);
        context.update(message);
        let mut result: [u8; 48] = [0; 48];
        result.copy_from_slice(context.finish().as_ref());
        Ok(result)
    }

    fn hash_all_SHA256(&self, messages: Vec<&[u8]>) -> Result<[u8; 32], Error> {
        let hash: Vec<u8> = messages
            .into_iter()
            .map(|m| self.hash_SHA256(m).unwrap())
            .into_iter()
            .flatten()
            .collect();
        let hash = self.hash_SHA256(&hash)?;
        Ok(hash)
    }

    fn hash_all_SHA384(&self, messages: Vec<&[u8]>) -> Result<[u8; 48], Error> {
        let hash: Vec<u8> = messages
            .into_iter()
            .map(|m| self.hash_SHA384(m).unwrap())
            .into_iter()
            .flatten()
            .collect();
        let hash = self.hash_SHA384(&hash)?;
        Ok(hash)
    }

    fn concat_u8_48(&self, left: [u8; 48], right: [u8; 48]) -> Result<[u8; 96], Error> {
        let mut iter = left.into_iter().chain(right);
        let result = [(); 96].map(|_| iter.next().unwrap());
        Ok(result)
    }

    fn deep_hash_list(
        &self,
        data_len: usize,
        data: Vec<&[u8]>,
        hash: Option<[u8; 48]>,
    ) -> Result<[u8; 48], Error> {
        let mut hash = if let Some(hash) = hash {
            hash
        } else {
            let list_tag = format!("list{}", data_len);
            self.hash_SHA384(list_tag.as_bytes())?
        };

        for blob in data.iter() {
            let blob_tag = format!("blob{}", blob.len());
            let blob_hash = self.hash_all_SHA384(vec![blob_tag.as_bytes(), blob])?;
            hash = self.hash_SHA384(&self.concat_u8_48(hash, blob_hash)?)?;
        }
        Ok(hash)
    }

    fn deep_hash_tags(&self, tags: &Vec<Tag>) -> Result<[u8; 48], Error> {
        let list_tag = format!("list{}", tags.len());
        let mut hash = self.hash_SHA384(list_tag.as_bytes())?;

        for tag_slice in tags.to_slices()?.into_iter() {
            let tag_slice_hash = self.deep_hash_list(tag_slice.len(), tag_slice, None)?;
            hash = self.hash_SHA384(&self.concat_u8_48(hash, tag_slice_hash)?)?;
        }
        Ok(hash)
    }

    /// Calculates deep hash of the required fields.
    ///
    /// Completes the calculation of the root hash to be signed in three steps. First is to calculate the hash for all of
    /// the items up to the tags. Then the hash is calculated for the tags separately and concatenated with the hash
    /// from the first part of the list. This is then used as the starting point to calculate the final hash from
    /// the final two items in the list.
    fn deep_hash(&self, transaction: &Transaction) -> Result<[u8; 48], Error> {
        // Calculate hash for first part of list.
        let pre_tag_hash = self.deep_hash_list(
            9,
            vec![
                &transaction.format.to_string().as_bytes(),
                &transaction.owner.0,
                &transaction.target.0,
                &transaction.quantity.to_string().as_bytes(),
                &transaction.reward.to_string().as_bytes(),
                &transaction.last_tx.0,
            ],
            None,
        )?;

        // Calculate deep hash for tags and concat with has from first part of list.
        let tag_hash = self.deep_hash_tags(&transaction.tags)?;
        let post_tag_hash = self.hash_SHA384(&self.concat_u8_48(pre_tag_hash, tag_hash)?)?;

        // Calculate hash for last part of list starting with hash of first part of list and tags.
        let final_hash = self.deep_hash_list(
            0,
            vec![
                &transaction.data_size.to_string().as_bytes(),
                &transaction.data_root.0,
            ],
            Some(post_tag_hash),
        )?;

        Ok(final_hash)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        crypto::Methods as CryptoMethods,
        transaction::{Base64, FromStrs, Tag},
        Arweave, Error, Methods as ArewaveMethods,
    };
    use std::str::FromStr;

    #[tokio::test]
    async fn test_deep_hash() -> Result<(), Error> {
        let arweave = Arweave::from_keypair_path(
            "tests/fixtures/arweave-key-7eV1qae4qVNqsNChg3Scdi-DpOLJPCogct4ixoq1WNg.json",
            None,
        )
        .await?;

        let file_paths = ["0.png", "1mb.bin"];
        let hashes: [[u8; 48]; 2] = [
            [
                250, 147, 146, 233, 232, 245, 14, 213, 182, 94, 254, 251, 28, 124, 128, 225, 51, 7,
                112, 16, 20, 209, 224, 26, 55, 78, 27, 4, 50, 223, 158, 240, 5, 64, 127, 126, 81,
                156, 153, 245, 207, 219, 8, 108, 158, 120, 212, 214,
            ],
            [
                196, 4, 241, 167, 159, 14, 68, 184, 220, 208, 48, 238, 148, 76, 125, 68, 62, 84,
                192, 99, 165, 188, 36, 73, 249, 200, 16, 52, 193, 249, 190, 60, 85, 148, 252, 195,
                118, 197, 52, 74, 173, 30, 58, 63, 46, 11, 56, 135,
            ],
        ];

        for (file_path, correct_hash) in file_paths.iter().zip(hashes) {
            let last_tx = Base64::from_str("LCwsLCwsLA")?;
            let other_tags = vec![Tag::from_utf8_strs("key2", "value2")?];
            let transaction = arweave
                .create_transaction_from_file_path(
                    &format!("tests/fixtures/{}", file_path),
                    Some(other_tags),
                    Some(last_tx),
                    Some(0),
                )
                .await?;

            let deep_hash = arweave.crypto.deep_hash(&transaction)?;
            assert_eq!(deep_hash, correct_hash);
        }
        Ok(())
    }
}
