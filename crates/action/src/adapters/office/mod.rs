//! `excel`/`word` namespace adapters: Office COM automation
//! (`docs/specs/action.md`: "Office COM: Excel (open workbook, read
//! range, write range, save) and Word (open, get text, replace text,
//! save), each releasing COM objects deterministically").
//!
//! Both adapters are generic over a small backend trait
//! ([`excel::ExcelBackend`], [`word::WordBackend`]) so every test in this
//! crate runs against an in-memory mock ([`excel::MockExcelBackend`],
//! [`word::MockWordBackend`]) and never needs Office installed. The real
//! COM backend ([`com::ComExcelBackend`], [`com::ComWordBackend`]) lives
//! behind the `office-com` cargo feature and is not exercised by
//! `cargo test` (no Office in CI); see FOLLOWUPS in `RESULT.md`.

pub mod excel;
pub mod word;

#[cfg(feature = "office-com")]
pub mod com;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum OfficeError {
    #[error("unknown workbook handle `{0}`")]
    UnknownWorkbook(u64),
    #[error("unknown document handle `{0}`")]
    UnknownDocument(u64),
    #[error("invalid range `{range}`: {reason}")]
    BadRange { range: String, reason: String },
    #[error("missing required argument `{0}`")]
    MissingArg(&'static str),
    #[error("bad handle `{0}`: expected an integer id")]
    BadHandle(String),
    #[error("com automation error: {0}")]
    Com(String),
}

pub use excel::{ExcelAdapter, ExcelBackend, MockExcelBackend};
pub use word::{MockWordBackend, WordAdapter, WordBackend};

#[cfg(feature = "office-com")]
pub use com::{ComExcelBackend, ComWordBackend};
