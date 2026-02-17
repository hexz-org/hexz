use crate::common::*;
use anyhow::Result;

pub fn run() -> Result<()> {
    let root = find_workspace_root()?;

    println!("{GREEN}Building Rust docs\u{2026}{RESET}");
    cmd(cargo())
        .args([
            "doc",
            "--workspace",
            "--no-deps",
            "--document-private-items",
        ])
        .current_dir(&root)
        .run()?;

    let mkdocs_bin = mkdocs(&root);
    if !std::path::Path::new(&mkdocs_bin).is_absolute() {
        require_cmd(&mkdocs_bin)?;
    }

    println!("{GREEN}Building MkDocs (Guides & Python API)\u{2026}{RESET}");
    cmd(mkdocs_bin.as_str())
        .arg("build")
        .current_dir(&root)
        .run()?;

    println!("{GREEN}Stitching Rust docs into main site\u{2026}{RESET}");
    let site_rust = root.join("site/rust");
    std::fs::create_dir_all(&site_rust)?;
    copy_dir_recursive(&root.join("target/doc"), &site_rust)?;

    println!("{GREEN}Done! Site built in site/{RESET}");
    Ok(())
}
