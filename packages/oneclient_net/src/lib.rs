mod cache;
mod config;
mod download;
mod error;
mod pause;
mod rate_limit;
mod request;
mod response;
mod service;

pub mod status;

pub use cache::{EtagPolicy, Fetched, commit_etag, fetch_cached};
pub use config::NetConfig;
pub use download::{download_verified, fetch_verified, matches_on_disk};
pub use error::{NetworkFailure, RequestError, classify_network_failure, error_chain};
pub use pause::PauseGate;
pub use rate_limit::{DownloadLimiter, throttle};
pub use request::*;
pub use response::*;
pub use service::RequestClient;
