fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "host_info",
            "contract_probe",
            "resolve_close",
            "connect_direct",
            "read_only_query",
            "disconnect_database",
            "database_status",
        ]),
    ))
    .expect("validate StoneQL native configuration and command permissions");
}
