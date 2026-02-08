use anyhow::Result;
use std::path::PathBuf;
use strata_common::sign;

pub fn run(output_dir: Option<PathBuf>) -> Result<()> {
    let dir = output_dir.unwrap_or_else(|| std::env::current_dir().unwrap());
    let priv_path = dir.join("private.key");
    let pub_path = dir.join("public.key");

    println!("Generating Ed25519 keypair...");
    sign::generate_keypair(&priv_path, &pub_path)?;

    println!("Keys generated:");
    println!("  Private: {:?}", priv_path);
    println!("  Public:  {:?}", pub_path);
    println!("Keep the private key safe!");

    Ok(())
}
