//! Native adapters: filesystem, email (IMAP/SMTP), OCR/PDF, and Office COM
//! (Excel/Word). Each registers into the [`crate::adapter`] framework
//! (`docs/specs/action.md`: a namespace, a JSON schema per verb, a risk
//! class per verb, an idempotency hint). [`crate::AdapterRegistry::call`]
//! validates every `adapter_call` against the registered schema before
//! dispatch, so the adapters below do not re-validate their own args.
//!
//! - [`filesystem`]: `fs` namespace. read/write/copy/move/delete; delete
//!   is destructive and recycles rather than unlinking.
//! - [`email`]: `email` namespace. IMAP-shaped fetch/search over a
//!   [`email::MailStore`] plus SMTP send over a [`email::Mailer`]; send is
//!   destructive/irreversible.
//! - [`ocr`]: `ocr` namespace. On-device text plus word-box extraction
//!   from PDF and PNG, behind the default-on `ocr` cargo feature.
//! - [`office`]: `excel`/`word` namespaces. Office COM automation behind
//!   the `office-com` cargo feature, against an [`office::OfficeBackend`]
//!   trait so tests run against a mock without Office installed.

pub mod email;
pub mod filesystem;
pub mod ocr;
pub mod office;
