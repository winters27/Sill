fn main() {
    // Test binaries need the same common controls the app gets.
    //
    // `tauri_build` embeds a manifest in the application declaring version 6
    // of `Microsoft.Windows.Common-Controls`. Test binaries get no manifest,
    // so the loader binds them to the version 5 comctl32 that ships in
    // System32, which does not export `TaskDialogIndirect`. Anything reaching
    // the dialog plugin then produces a test binary that cannot start at all:
    // STATUS_ENTRYPOINT_NOT_FOUND, before a single test runs, with no hint
    // about which import is missing.
    //
    // Two flags because they do different halves of the job.
    // `/MANIFESTDEPENDENCY` says what belongs in the manifest, and
    // `/MANIFEST:EMBED` is what puts it inside the binary rather than in a
    // loose `.manifest` file beside it, which the loader ignores for a
    // harness cargo runs from elsewhere.
    //
    // **Test targets only.** The catch-all `rustc-link-arg` was tried and
    // breaks the application: it already embeds a manifest of its own, and
    // asking the linker to add a second produces `LNK1123: failure during
    // conversion to COFF`. That also constrains where tests can live, which
    // is why the action registry's tests are an integration test rather than
    // a unit test.
    #[cfg(windows)]
    {
        println!("cargo:rustc-link-arg-tests=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg-tests=/MANIFESTDEPENDENCY:type='win32' \
             name='Microsoft.Windows.Common-Controls' version='6.0.0.0' \
             processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'"
        );
    }

    // The application's own manifest, replacing the one `tauri_build` embeds.
    //
    // Only to add `longPathAware`: Sill walks whatever folders somebody
    // indexes, and a `node_modules` tree passes 260 characters without
    // trying, at which point every path API refuses the entry and it is
    // missing from the index with nothing said. `sill.manifest` carries
    // Tauri's Common-Controls dependency across verbatim, for the reason
    // written above about test binaries.
    #[cfg(windows)]
    {
        let attributes = tauri_build::Attributes::new().windows_attributes(
            tauri_build::WindowsAttributes::new().app_manifest(include_str!("sill.manifest")),
        );

        // `try_build` rather than `build`, so a manifest this file gets wrong
        // stops the build with the reason rather than being embedded.
        tauri_build::try_build(attributes).expect("the application manifest is valid");
    }

    #[cfg(not(windows))]
    tauri_build::build()
}
