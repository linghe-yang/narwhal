use thiserror::Error;

#[derive(Debug, Error)]
pub enum DrbError {
    #[error("Common core has not been decided")]
    NoCommonCore,
    #[error("Index out of bound")]
    InvalidIndex,
}