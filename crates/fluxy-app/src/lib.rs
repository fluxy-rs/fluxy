//! Application assembly: config, paths, identity/trust bootstrap, status.

pub mod config;
pub mod engine;
pub mod error;
pub mod headless;
pub mod logging;

pub use config::{AppConfig, AppPaths, NetworkPreference};
pub use engine::{Engine, EngineStatus};
pub use error::{Error, ErrorCode};
pub use headless::{
    DEFAULT_BLOCK_SIZE, HeadlessError, HeadlessTransfer, OFFER_TIMEOUT, OfferView, PreparedSend,
    ReceiveOutcome, SendOutcome, SendRequest,
};
