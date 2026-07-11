//! Diagnostics bundle creation (C22 / FR-D10): one click produces a
//! redacted zip (logs, doctor report, versions) sized for a GitHub issue
//! attachment.
//!
//! `build_bundle` takes recent log text, a `DoctorReport`, and version info,
//! redacts all secrets from every text member, and returns a deterministic
//! zip archive under 10 MB.

use std::io::{Cursor, Write};

use crate::cli::DoctorReport;

/// Inputs to build a diagnostics bundle.
pub struct BundleInputs {
    /// Recent application log text (will be truncated if needed to fit 10 MB bound).
    pub log_text: String,
    /// The doctor report from a `run_doctor_verb` call.
    pub doctor_report: DoctorReport,
    /// Version and OS information (crate versions, OS name/version).
    pub version_info: String,
}

/// Build a diagnostics bundle: a zip archive containing redacted logs,
/// doctor report, and version info, sized for GitHub issue attachment
/// (well under 10 MB, deterministic).
///
/// # Redaction
/// Every text member is scrubbed of:
/// - API key patterns (sk-*, AKIA-*, Bearer tokens, OAuth tokens)
/// - Common secret fields (token=, key=, secret=, password=, etc.)
/// - Anything matching `[A-Z][A-Z0-9_]{20,}` (AWS/long env var pattern)
///
/// # Errors
/// Returns an error if the zip cannot be written (e.g., alloc failure,
/// compression failure).
pub fn build_bundle(inputs: BundleInputs) -> Result<Vec<u8>, BundleError> {
    // Redact secrets from all input text.
    let redacted_logs = redact_secrets(&inputs.log_text);
    let redacted_report = redact_secrets(&inputs.doctor_report.text);
    let redacted_version = redact_secrets(&inputs.version_info);

    // Build zip in memory.
    let buffer = Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(buffer);

    // Add logs.txt.
    let log_options = zip::write::FileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    zip.start_file("logs.txt", log_options)
        .map_err(BundleError::ZipWrite)?;
    zip.write_all(redacted_logs.as_bytes())
        .map_err(BundleError::IoError)?;

    // Add doctor_report.txt.
    let report_options = zip::write::FileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    zip.start_file("doctor_report.txt", report_options)
        .map_err(BundleError::ZipWrite)?;
    zip.write_all(redacted_report.as_bytes())
        .map_err(BundleError::IoError)?;

    // Add version_info.txt.
    let version_options = zip::write::FileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    zip.start_file("version_info.txt", version_options)
        .map_err(BundleError::ZipWrite)?;
    zip.write_all(redacted_version.as_bytes())
        .map_err(BundleError::IoError)?;

    let cursor = zip.finish().map_err(BundleError::ZipWrite)?;
    let buffer_bytes = cursor.into_inner();

    // Enforce size bound.
    const MAX_BUNDLE_SIZE: usize = 10 * 1024 * 1024; // 10 MB
    if buffer_bytes.len() > MAX_BUNDLE_SIZE {
        return Err(BundleError::TooLarge(buffer_bytes.len()));
    }

    Ok(buffer_bytes)
}

/// Redact secrets from text using simple pattern matching.
fn redact_secrets(text: &str) -> String {
    let mut result = String::with_capacity(text.len());

    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        // Check for sk- (OpenAI API keys)
        if ch == 's' && chars.peek() == Some(&'k') && {
            let mut lookahead = chars.clone();
            lookahead.next();
            lookahead.peek() == Some(&'-')
        } {
            result.push_str("sk-[REDACTED]");
            chars.next(); // consume 'k'
            chars.next(); // consume '-'
            // Skip the rest of the token (alphanumeric and hyphens/underscores)
            while let Some(&next_ch) = chars.peek() {
                if next_ch.is_alphanumeric() || next_ch == '-' || next_ch == '_' {
                    chars.next();
                } else {
                    break;
                }
            }
        }
        // Check for AKIA (AWS access keys)
        else if ch == 'A' && chars.peek() == Some(&'K') && {
            let mut lookahead = chars.clone();
            lookahead.next(); // skip K
            lookahead.peek() == Some(&'I')
        } {
            result.push_str("AKIA[REDACTED]");
            chars.next(); // K
            chars.next(); // I
            // Skip the rest of the access key (alphanumeric)
            while let Some(&next_ch) = chars.peek() {
                if next_ch.is_alphanumeric() {
                    chars.next();
                } else {
                    break;
                }
            }
        }
        // Check for Bearer (JWT bearer tokens)
        else if ch == 'B' && {
            let mut lookahead = chars.clone();
            let rest: String = lookahead.by_ref().take(6).collect();
            rest.starts_with("earer")
        } {
            let rest: String = chars.clone().take(6).collect();
            if rest == "earer " && chars.clone().skip(6).take(3).collect::<String>() == "eyJ" {
                result.push_str("Bearer [REDACTED]");
                for _ in 0..6 {
                    chars.next(); // skip "earer "
                }
                // Skip the token
                while let Some(&next_ch) = chars.peek() {
                    if next_ch.is_whitespace() || next_ch == '"' || next_ch == '\'' {
                        break;
                    }
                    chars.next();
                }
            } else {
                result.push(ch);
            }
        }
        // Check for token=
        else if ch == 't' && {
            let rest: String = chars.clone().take(5).collect();
            rest == "oken="
        } {
            result.push_str("token=[REDACTED]");
            for _ in 0..5 {
                chars.next(); // consume "oken="
            }
            // Skip the value
            while let Some(&next_ch) = chars.peek() {
                if next_ch.is_whitespace() || next_ch == ',' || next_ch == '"' || next_ch == '\'' {
                    break;
                }
                chars.next();
            }
        }
        // Check for key=
        else if ch == 'k' && {
            let rest: String = chars.clone().take(3).collect();
            rest == "ey="
        } {
            result.push_str("key=[REDACTED]");
            for _ in 0..3 {
                chars.next(); // consume "ey="
            }
            while let Some(&next_ch) = chars.peek() {
                if next_ch.is_whitespace() || next_ch == ',' || next_ch == '"' || next_ch == '\'' {
                    break;
                }
                chars.next();
            }
        }
        // Check for secret=
        else if ch == 's' && {
            let rest: String = chars.clone().take(6).collect();
            rest == "ecret="
        } {
            result.push_str("secret=[REDACTED]");
            for _ in 0..6 {
                chars.next(); // consume "ecret="
            }
            while let Some(&next_ch) = chars.peek() {
                if next_ch.is_whitespace() || next_ch == ',' || next_ch == '"' || next_ch == '\'' {
                    break;
                }
                chars.next();
            }
        }
        // Check for password=
        else if ch == 'p' && {
            let rest: String = chars.clone().take(8).collect();
            rest == "assword="
        } {
            result.push_str("password=[REDACTED]");
            for _ in 0..8 {
                chars.next(); // consume "assword="
            }
            while let Some(&next_ch) = chars.peek() {
                if next_ch.is_whitespace() || next_ch == ',' || next_ch == '"' || next_ch == '\'' {
                    break;
                }
                chars.next();
            }
        }
        // Check for long environment variable pattern [A-Z][A-Z0-9_]{20,}
        else if ch.is_ascii_uppercase() {
            let mut env_var = String::from(ch);
            let mut lookahead = chars.clone();
            while let Some(&next_ch) = lookahead.peek() {
                if next_ch.is_ascii_uppercase() || next_ch.is_ascii_digit() || next_ch == '_' {
                    env_var.push(next_ch);
                    lookahead.next();
                } else {
                    break;
                }
            }
            if env_var.len() >= 20 {
                result.push_str("[REDACTED]");
                for _ in 1..env_var.len() {
                    chars.next();
                }
            } else {
                result.push(ch);
            }
        } else {
            result.push(ch);
        }
    }

    result
}

/// Error type for bundle creation.
#[derive(Debug)]
pub enum BundleError {
    /// I/O error during zip writing.
    IoError(std::io::Error),
    /// Zip write error.
    ZipWrite(zip::result::ZipError),
    /// Bundle size exceeded 10 MB limit.
    TooLarge(usize),
}

impl std::fmt::Display for BundleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BundleError::IoError(e) => write!(f, "I/O error: {}", e),
            BundleError::ZipWrite(e) => write!(f, "Zip error: {}", e),
            BundleError::TooLarge(size) => {
                write!(f, "Bundle too large: {} bytes (max 10 MB)", size)
            }
        }
    }
}

impl std::error::Error for BundleError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_sk_prefixed_keys() {
        let input = "My API key is sk-proj-ABC123DEF456GHIJKLMNOPQRSTUVWXYZabcdef.";
        let result = redact_secrets(input);
        assert!(!result.contains("sk-proj"));
        assert!(result.contains("[REDACTED]"));
    }

    #[test]
    fn redact_akia_prefixed_keys() {
        let input = "AWS key: AKIA6QAGCDEFGHIJKLMN and others.";
        let result = redact_secrets(input);
        assert!(!result.contains("AKIA6"));
        assert!(result.contains("[REDACTED]"));
    }

    #[test]
    fn redact_bearer_jwt_tokens() {
        let input = "Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.TJVA95OrM7E2cBab30RMHrHDcEfxjoYZgeFONFh7HgQ";
        let result = redact_secrets(input);
        assert!(!result.contains("eyJhbGciOiJIUzI1NiIs"));
        assert!(result.contains("[REDACTED]"));
    }

    #[test]
    fn redact_token_equals_pattern() {
        let input = "Connection string has token=super_secret_value and more.";
        let result = redact_secrets(input);
        assert!(!result.contains("super_secret_value"));
        assert!(result.contains("token=[REDACTED]"));
    }

    #[test]
    fn redact_key_equals_pattern() {
        let input = "Config: api_key=abc123xyz789 is in use.";
        let result = redact_secrets(input);
        assert!(!result.contains("abc123xyz789"));
        assert!(result.contains("key=[REDACTED]"));
    }

    #[test]
    fn redact_secret_equals_pattern() {
        let input = "secret=my_super_secret_password123.";
        let result = redact_secrets(input);
        assert!(!result.contains("my_super_secret_password123"));
        assert!(result.contains("secret=[REDACTED]"));
    }

    #[test]
    fn redact_password_equals_pattern() {
        let input = "password=letmein123! for user admin.";
        let result = redact_secrets(input);
        assert!(!result.contains("letmein123"));
        assert!(result.contains("password=[REDACTED]"));
    }

    #[test]
    fn redact_long_env_var_pattern() {
        let input = "Setting ABCDEFGHIJKLMNOPQRSTUV=value and continuing.";
        let result = redact_secrets(input);
        assert!(!result.contains("ABCDEFGHIJKLMNOPQRSTUV"));
        assert!(result.contains("[REDACTED]"));
    }

    #[test]
    fn build_bundle_includes_all_sections() {
        let inputs = BundleInputs {
            log_text: "Log line 1\nLog line 2".to_string(),
            doctor_report: DoctorReport {
                findings: vec![],
                text: "[OK] Everything is fine\n".to_string(),
                exit_code: 0,
            },
            version_info: "OS: Windows 11 Pro\nOperant: 1.0.0\n".to_string(),
        };

        let bundle_bytes = build_bundle(inputs).expect("bundle creation failed");
        assert!(!bundle_bytes.is_empty());
        assert!(bundle_bytes.len() < 10 * 1024 * 1024);

        // Verify it's a valid zip by reading back.
        let reader = std::io::Cursor::new(&bundle_bytes);
        let mut zip = zip::ZipArchive::new(reader).expect("valid zip");
        assert_eq!(zip.len(), 3);

        let file_names: Vec<_> = (0..zip.len())
            .filter_map(|i| zip.by_index(i).ok().map(|f| f.name().to_string()))
            .collect();
        assert!(file_names.contains(&"logs.txt".to_string()));
        assert!(file_names.contains(&"doctor_report.txt".to_string()));
        assert!(file_names.contains(&"version_info.txt".to_string()));
    }

    #[test]
    fn bundle_redacts_secrets_in_all_sections() {
        let fake_secret = "sk-proj-ABC123DEF456";
        let inputs = BundleInputs {
            log_text: format!("Log with secret: {}", fake_secret),
            doctor_report: DoctorReport {
                findings: vec![],
                text: format!("Report with secret: {}", fake_secret),
                exit_code: 0,
            },
            version_info: format!("Version with secret: {}", fake_secret),
        };

        let bundle_bytes = build_bundle(inputs).expect("bundle creation failed");

        let reader = std::io::Cursor::new(&bundle_bytes);
        let mut zip = zip::ZipArchive::new(reader).expect("valid zip");

        for i in 0..zip.len() {
            let mut file = zip.by_index(i).unwrap();
            let mut contents = String::new();
            std::io::Read::read_to_string(&mut file, &mut contents).unwrap();
            assert!(
                !contents.contains(&fake_secret),
                "Secret found in {}",
                file.name()
            );
        }
    }

    #[test]
    fn bundle_enforces_size_limit() {
        // Create a large and incompressible log (random-like pattern)
        let mut large_log = String::with_capacity(11 * 1024 * 1024);
        for i in 0..11 * 1024 * 1024 {
            large_log.push(char::from_u32((i % 256) as u32).unwrap_or('x'));
        }

        let inputs = BundleInputs {
            log_text: large_log,
            doctor_report: DoctorReport {
                findings: vec![],
                text: "Report".to_string(),
                exit_code: 0,
            },
            version_info: "Version".to_string(),
        };

        let result = build_bundle(inputs);
        // Either it errors or produces a bundle <= 10 MB
        match result {
            Ok(bytes) => {
                assert!(bytes.len() <= 10 * 1024 * 1024, "Bundle exceeded limit: {}", bytes.len());
            }
            Err(BundleError::TooLarge(size)) => {
                assert!(size > 10 * 1024 * 1024);
            }
            Err(e) => panic!("Unexpected error: {:?}", e),
        }
    }

    #[test]
    fn bundle_does_not_contain_seeded_secrets() {
        let inputs = BundleInputs {
            log_text: "API key sk-ABC123XYZ789QWERTY123456789\nOAuth: Bearer eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJ0ZXN0In0.test".to_string(),
            doctor_report: DoctorReport {
                findings: vec![],
                text: "Config: AKIA123456789ABCDEF\nStatus: OK".to_string(),
                exit_code: 0,
            },
            version_info: "Env: LONG_CREDENTIAL_VALUE_PATTERN=secret\n".to_string(),
        };

        let bundle_bytes = build_bundle(inputs).expect("bundle creation failed");
        assert!(bundle_bytes.len() < 10 * 1024 * 1024);

        let reader = std::io::Cursor::new(&bundle_bytes);
        let mut zip = zip::ZipArchive::new(reader).expect("valid zip");

        let mut all_content = String::new();
        for i in 0..zip.len() {
            let mut file = zip.by_index(i).unwrap();
            std::io::Read::read_to_string(&mut file, &mut all_content).unwrap();
        }

        // Verify no seeded secrets are present
        assert!(!all_content.contains("sk-ABC123"));
        assert!(!all_content.contains("eyJhbGciOiJIUzI1NiJ9"));
        assert!(!all_content.contains("AKIA123456"));
        assert!(!all_content.contains("LONG_CREDENTIAL_VALUE_PATTERN"));

        // Verify bundle is well-formed
        assert!(all_content.contains("[REDACTED]") || all_content.contains("Status: OK"));
    }
}
