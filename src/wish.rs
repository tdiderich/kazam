use anyhow::Result;

pub fn list(_json: bool) -> Result<()> {
    println!("  No wishes available yet.");
    Ok(())
}

pub fn init(_name: &str, _dir: Option<std::path::PathBuf>, _force: bool) -> Result<()> {
    anyhow::bail!("not yet implemented")
}
