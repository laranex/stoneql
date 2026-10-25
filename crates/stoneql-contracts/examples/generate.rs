use std::{env, error::Error, fs, path::PathBuf};
fn main() -> Result<(), Box<dyn Error>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../contracts/generated/v1.ts");
    let generated = stoneql_contracts::v1::typescript();
    if env::args().any(|arg| arg == "--check") {
        if fs::read_to_string(&path)? != generated {
            return Err("generated TypeScript is stale; run npm run contracts:generate".into());
        }
    } else {
        fs::create_dir_all(path.parent().ok_or("invalid output path")?)?;
        fs::write(path, generated)?;
    }
    Ok(())
}
