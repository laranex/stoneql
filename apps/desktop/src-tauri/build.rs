fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&["host_info", "contract_probe", "resolve_close"]),
    ))
    .expect("validate dbAdmin native configuration and command permissions");
}
