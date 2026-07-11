use operant_doctor::{build_bundle, BundleInputs, DoctorReport, Finding, Severity};
use std::io::Read;
use zip::ZipArchive;

#[test]
fn bundle_creation_with_seeded_secrets() {
    let inputs = BundleInputs {
        log_text: "Application started.\nConnecting to API with key sk-proj-1234567890abcdefghijklmnopqrst.\nSuccess.".to_string(),
        doctor_report: DoctorReport {
            findings: vec![
                Finding::healthy("disk_check", "Disk space OK", "No action needed"),
                Finding::healthy("model_check", "Model reachable", "No action needed"),
            ],
            text: "[OK] System check complete\n[OK] Model is reachable\n".to_string(),
            exit_code: 0,
        },
        version_info: "OS: Windows 11 Pro 10.0.26200\nOperant: 1.0.0\n".to_string(),
    };

    let bundle_result = build_bundle(inputs);
    assert!(
        bundle_result.is_ok(),
        "Bundle creation failed: {:?}",
        bundle_result.err()
    );

    let bundle_bytes = bundle_result.unwrap();
    assert!(!bundle_bytes.is_empty(), "Bundle is empty");
    assert!(
        bundle_bytes.len() < 10 * 1024 * 1024,
        "Bundle exceeds 10 MB: {} bytes",
        bundle_bytes.len()
    );

    // Verify the seeded secret is not in the bundle
    let bundle_str = String::from_utf8_lossy(&bundle_bytes);
    assert!(
        !bundle_str.contains("sk-proj-1234567890abcdefghijklmnopqrst"),
        "Secret key leaked into bundle"
    );
}

#[test]
fn bundle_contains_all_sections() {
    let inputs = BundleInputs {
        log_text: "Log line 1\nLog line 2\nLog line 3".to_string(),
        doctor_report: DoctorReport {
            findings: vec![],
            text: "[OK] Disk space: 500 GB free\n[OK] Model: reachable\n[OK] Audio: devices present\n"
                .to_string(),
            exit_code: 0,
        },
        version_info: "OS: Windows 11 Pro\nOperant Version: 1.0.0\n".to_string(),
    };

    let bundle_bytes = build_bundle(inputs).expect("Bundle creation failed");

    let reader = std::io::Cursor::new(&bundle_bytes);
    let mut zip = ZipArchive::new(reader).expect("Valid zip archive");

    // Verify we have exactly 3 files
    assert_eq!(zip.len(), 3, "Bundle should contain exactly 3 files");

    // Verify each expected file exists
    let file_names: Vec<_> = (0..zip.len())
        .filter_map(|i| zip.by_index(i).ok().map(|f| f.name().to_string()))
        .collect();

    assert!(
        file_names.contains(&"logs.txt".to_string()),
        "Bundle missing logs.txt"
    );
    assert!(
        file_names.contains(&"doctor_report.txt".to_string()),
        "Bundle missing doctor_report.txt"
    );
    assert!(
        file_names.contains(&"version_info.txt".to_string()),
        "Bundle missing version_info.txt"
    );

    // Verify content is present in each file
    let mut zip = ZipArchive::new(std::io::Cursor::new(&bundle_bytes)).unwrap();

    let mut logs_content = String::new();
    zip.by_name("logs.txt")
        .unwrap()
        .read_to_string(&mut logs_content)
        .unwrap();
    assert!(logs_content.contains("Log line"), "Logs not present in logs.txt");

    let mut report_content = String::new();
    zip.by_name("doctor_report.txt")
        .unwrap()
        .read_to_string(&mut report_content)
        .unwrap();
    assert!(
        report_content.contains("[OK]"),
        "Doctor report not present in doctor_report.txt"
    );

    let mut version_content = String::new();
    zip.by_name("version_info.txt")
        .unwrap()
        .read_to_string(&mut version_content)
        .unwrap();
    assert!(
        version_content.contains("Windows"),
        "Version info not present in version_info.txt"
    );
}

#[test]
fn bundle_redacts_all_secret_patterns() {
    let inputs = BundleInputs {
        log_text: vec![
            "API key sk-proj-ABC123DEF456GHIJKLMNOPabcdef",
            "AWS AKIA6QAGCDEFGHIJKLMN secret",
            "Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.TJVA95OrM7E2cBab30RMHrHDcEfxjoYZgeFONFh7HgQ",
            "token=super_secret_value_here",
            "password=letmein123!456",
            "secret=classified_information",
            "api_key=abcdefghijklmnop",
            "LONGCREDENTIALVALUEPATTERNABC=value",
        ]
        .join("\n"),
        doctor_report: DoctorReport {
            findings: vec![],
            text: "Config: token=internal_token_12345\nStatus: OK".to_string(),
            exit_code: 0,
        },
        version_info: "Version: 1.0.0\nEnv: HOME_DIR_PATH_VARIABLE=secret".to_string(),
    };

    let bundle_bytes = build_bundle(inputs).expect("Bundle creation failed");
    assert!(bundle_bytes.len() < 10 * 1024 * 1024, "Bundle size exceeded");

    // Read and check all contents
    let reader = std::io::Cursor::new(&bundle_bytes);
    let mut zip = ZipArchive::new(reader).expect("Valid zip");

    let mut all_content = String::new();
    for i in 0..zip.len() {
        let mut file = zip.by_index(i).unwrap();
        file.read_to_string(&mut all_content).unwrap();
    }

    // Verify no secrets are present
    assert!(
        !all_content.contains("sk-proj-ABC123"),
        "API key leaked into bundle"
    );
    assert!(
        !all_content.contains("AKIA6QAGCD"),
        "AWS key leaked into bundle"
    );
    assert!(
        !all_content.contains("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9"),
        "Bearer token leaked into bundle"
    );
    assert!(
        !all_content.contains("super_secret_value_here"),
        "Token value leaked into bundle"
    );
    assert!(
        !all_content.contains("letmein123"),
        "Password leaked into bundle"
    );
    assert!(
        !all_content.contains("classified_information"),
        "Secret value leaked into bundle"
    );
    assert!(
        !all_content.contains("internal_token_12345"),
        "Internal token leaked into bundle"
    );

    // Verify redaction markers are present
    assert!(all_content.contains("[REDACTED]"), "No redaction markers found");
}

#[test]
fn bundle_respects_size_limit() {
    // Create a log that's larger than the 10 MB limit when compressed
    let large_log = "x".repeat(11 * 1024 * 1024); // 11 MB of 'x' characters

    let inputs = BundleInputs {
        log_text: large_log,
        doctor_report: DoctorReport {
            findings: vec![],
            text: "Report text".to_string(),
            exit_code: 0,
        },
        version_info: "Version 1.0.0".to_string(),
    };

    let result = build_bundle(inputs);

    // The bundle should either fail or be under the limit
    match result {
        Ok(bundle_bytes) => {
            assert!(
                bundle_bytes.len() < 10 * 1024 * 1024,
                "Bundle exceeded 10 MB limit: {} bytes",
                bundle_bytes.len()
            );
        }
        Err(_) => {
            // This is expected when the bundle is too large
        }
    }
}

#[test]
fn bundle_is_deterministic() {
    let inputs1 = BundleInputs {
        log_text: "Consistent log output".to_string(),
        doctor_report: DoctorReport {
            findings: vec![],
            text: "[OK] All systems nominal\n".to_string(),
            exit_code: 0,
        },
        version_info: "OS: Windows 11\nVersion: 1.0.0\n".to_string(),
    };

    let inputs2 = BundleInputs {
        log_text: "Consistent log output".to_string(),
        doctor_report: DoctorReport {
            findings: vec![],
            text: "[OK] All systems nominal\n".to_string(),
            exit_code: 0,
        },
        version_info: "OS: Windows 11\nVersion: 1.0.0\n".to_string(),
    };

    let bundle1 = build_bundle(inputs1).expect("First bundle creation failed");
    let bundle2 = build_bundle(inputs2).expect("Second bundle creation failed");

    // Bundles should be identical when inputs are identical
    assert_eq!(bundle1, bundle2, "Bundles are not deterministic");
}

#[test]
fn bundle_from_doctor_report_with_findings() {
    let inputs = BundleInputs {
        log_text: "System initialized\nDiagnostic checks running...\n".to_string(),
        doctor_report: DoctorReport {
            findings: vec![
                Finding {
                    finding_id: "disk_free".to_string(),
                    severity: Severity::Info,
                    what: "There is enough free disk space.".to_string(),
                    why: "Free space is above the 1 GB threshold.".to_string(),
                    action: "No action needed.".to_string(),
                    fix_command: None,
                },
                Finding {
                    finding_id: "model_reachable".to_string(),
                    severity: Severity::Info,
                    what: "The model is reachable.".to_string(),
                    why: "Operant was able to connect to it just now.".to_string(),
                    action: "No action needed.".to_string(),
                    fix_command: None,
                },
            ],
            text: "[OK] Disk: There is enough free disk space.\n[OK] Model: The model is reachable.\n"
                .to_string(),
            exit_code: 0,
        },
        version_info: "Operant Version: 1.0.0\nRust Version: 1.90\nOS: Windows 11 Pro".to_string(),
    };

    let bundle_bytes = build_bundle(inputs).expect("Bundle creation failed");

    let reader = std::io::Cursor::new(&bundle_bytes);
    let mut zip = ZipArchive::new(reader).expect("Valid zip");

    let mut doctor_content = String::new();
    zip.by_name("doctor_report.txt")
        .unwrap()
        .read_to_string(&mut doctor_content)
        .unwrap();

    // Verify doctor findings are in the bundle
    assert!(
        doctor_content.contains("[OK] Disk"),
        "Disk finding missing from doctor report"
    );
    assert!(
        doctor_content.contains("[OK] Model"),
        "Model finding missing from doctor report"
    );
}
