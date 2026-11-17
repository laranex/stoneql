fn main() {
    // Windows: Tauri's resource manifest reaches only the app binary, so test binaries load the
    // old Common Controls and fail to start (STATUS_ENTRYPOINT_NOT_FOUND). Embed the same
    // manifest (tauri-build's default) with the linker instead, for every binary.
    let windows = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows");
    if windows && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        let manifest =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("windows-app-manifest.xml");
        println!("cargo:rerun-if-changed=windows-app-manifest.xml");
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
    }
    let attributes = if windows {
        tauri_build::Attributes::new()
            .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest())
    } else {
        tauri_build::Attributes::new()
    };
    tauri_build::try_build(
        attributes.app_manifest(tauri_build::AppManifest::new().commands(&[
            "host_info",
            "contract_probe",
            "resolve_close",
            "connect_direct",
            "read_only_query",
            "disconnect_database",
            "database_status",
            "switch_database",
            "pick_sqlite_file",
            "connect_sqlite",
            "ssh_host_key",
            "pick_ssh_key_file",
            "cancel_connect",
            "ssh_host_key_status",
            "trust_ssh_host_key",
            "forget_ssh_host_key",
            "store_secret",
            "delete_secret",
            "pick_export_file",
            "start_export",
            "export_status",
            "cancel_export",
            "submit_job",
            "fetch_job",
            "cancel_job",
            "begin_transaction",
            "end_transaction",
            "list_unresolved_outcomes",
            "acknowledge_outcome",
            "pick_import_file",
            "start_sql_import",
            "sql_import_status",
        ])),
    )
    .expect("validate StoneQL native configuration and command permissions");
}
