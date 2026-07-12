fn main() {
    tauri_build::build();

    // The target triple, so the shell can resolve the bundled core sidecar
    // (`operant-<triple>.exe`) beside its own executable at runtime
    // (src/bridge/mod.rs::resolve_core_bin, matching Tauri's externalBin
    // naming). TARGET is always set for build scripts.
    println!(
        "cargo:rustc-env=OPERANT_TARGET_TRIPLE={}",
        std::env::var("TARGET").unwrap_or_default()
    );

    // Common-Controls v6 manifest for cargo test and example binaries on
    // Windows.
    //
    // tao (the windowing layer under Tauri) statically imports
    // comctl32!TaskDialogIndirect. That symbol is only exported by the Common
    // Controls v6 assembly shipped in WinSxS; the plain System32 comctl32.dll
    // is v5.82 and does not export it. tauri-build embeds the Common-Controls
    // v6 application manifest only for bin targets (it emits
    // cargo:rustc-link-arg-bins), so cargo test and [[example]] binaries link
    // without that manifest, bind against System32 v5.82, and crash at process
    // load with STATUS_ENTRYPOINT_NOT_FOUND (0xC0000139) before main runs.
    //
    // Emitting the same manifest dependency for the test and example target
    // kinds makes those binaries bind the v6 assembly and load normally. This
    // is deterministic Tauri-on-Windows behavior, not a machine-specific quirk.
    // See docs/adr/0001-tauri-test-binaries-common-controls-manifest.md.
    //
    // The filesystem check is required. cargo errors with "does not have a
    // test/example target" if a rustc-link-arg-tests or -examples arg is
    // emitted for a target kind that has no source, which would break a plain
    // `cargo build` that has no tests or examples to link. So only emit each
    // arg when a matching .rs source actually exists.
    #[cfg(windows)]
    {
        let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        for kind in ["tests", "examples"] {
            let has = std::fs::read_dir(std::path::Path::new(&dir).join(kind))
                .map(|rd| rd.flatten().any(|e| e.path().extension().is_some_and(|x| x == "rs")))
                .unwrap_or(false);
            if has {
                println!("cargo:rustc-link-arg-{kind}=/MANIFEST:EMBED");
                println!("cargo:rustc-link-arg-{kind}=/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'");
            }
        }
    }
}
