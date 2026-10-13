//! 加密层（架构文档 §2）。全部使用 RustCrypto 成熟 crate，零自研密码学。

pub mod aead;
pub mod kdf;
pub mod keys;
pub mod rng;
pub mod verify;

pub use keys::{ItemKey, Kek, MasterKey, VaultKey, VerifyKey};
