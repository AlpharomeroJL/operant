//! Native adapters: filesystem, email (IMAP/SMTP), OCR/PDF, Office COM
//! (Excel/Word), and the browser (C5). Each registers into the
//! [`crate::adapter`] framework (`docs/specs/action.md`: a namespace, a
//! JSON schema per verb, a risk class per verb, an idempotency hint).
//! [`crate::AdapterRegistry::call`] validates every `adapter_call`
//! against the registered schema before dispatch, so the adapters below
//! do not re-validate their own args.
//!
//! - [`browser`]: `browser` namespace (C5, `docs/ARCHITECTURE.md`).
//!   `attach`/`snapshot`/`click`/`type`/`assert` against a
//!   [`browser::Browser`] backend: [`browser::FixtureBrowser`] (always
//!   built) parses a fixture webapp HTML file with no real browser
//!   involved; a minimal real CDP backend
//!   ([`browser::CdpBrowser`]) lives behind the `cdp` cargo feature. DOM
//!   plus accessibility tree emit as Perception Snapshots
//!   (`source: "browser"`); DOM actions emit as Action IR with css
//!   selectors, so web steps record, compile, and replay identically to
//!   native steps.
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

pub mod browser;
pub mod email;
pub mod filesystem;
pub mod ocr;
pub mod office;
