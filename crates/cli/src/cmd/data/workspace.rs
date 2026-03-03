//! Workspace management logic for Hexz (Git-style workflows).

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    pub base_archive: PathBuf,
}

pub struct Workspace {
    pub root: PathBuf,
    pub config: WorkspaceConfig,
}

impl Workspace {
    pub fn init(path: &Path, base_archive: PathBuf) -> Result<Self> {
        let abs_path = std::fs::canonicalize(path)?;

        // Centralized storage: ~/.hexz/workspaces/<hash_of_abs_path>
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut s = DefaultHasher::new();
        abs_path.hash(&mut s);
        let id = format!("{:x}", s.finish());

        let home = std::env::var("HOME").context("HOME not set")?;
        let hexz_root = PathBuf::from(home).join(".hexz").join("workspaces").join(&id);

        std::fs::create_dir_all(&hexz_root)?;

        let overlay_dir = hexz_root.join("overlay");
        std::fs::create_dir_all(overlay_dir)?;

        let config = WorkspaceConfig {
            base_archive: std::fs::canonicalize(base_archive)?,
        };

        let config_path = hexz_root.join("config.json");
        let f = std::fs::File::create(config_path)?;
        serde_json::to_writer_pretty(f, &config)?;

        Ok(Self {
            root: abs_path,
            config,
        })
    }

    pub fn find(start_path: &Path) -> Result<Option<Self>> {
        let mut current = if start_path.exists() {
            std::fs::canonicalize(start_path)?
        } else {
            return Ok(None);
        };

        loop {
            // First, try reading it locally (this works if FUSE is actively mounted!)
            let local_config = current.join(".hexz").join("config.json");
            if local_config.exists() {
                let f = std::fs::File::open(local_config)?;
                let config: WorkspaceConfig = serde_json::from_reader(f)?;
                return Ok(Some(Self {
                    root: current,
                    config,
                }));
            }

            // Fallback: If not mounted, calculate the global hash to check if it's a known workspace
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut s = DefaultHasher::new();
            current.hash(&mut s);
            let id = format!("{:x}", s.finish());
            let home = std::env::var("HOME").context("HOME not set")?;
            let hexz_root = PathBuf::from(home).join(".hexz").join("workspaces").join(id);
            let config_path = hexz_root.join("config.json");

            if config_path.exists() {
                 let f = std::fs::File::open(config_path)?;
                 let config: WorkspaceConfig = serde_json::from_reader(f)?;
                 return Ok(Some(Self {
                     root: current,
                     config,
                 }));
            }

            if let Some(parent) = current.parent() {
                current = parent.to_path_buf();
            } else {
                break;
            }
        }

        Ok(None)
    }

    pub fn overlay_path(&self) -> PathBuf {
        self.metadata_dir().join("overlay")
    }

    pub fn metadata_dir(&self) -> PathBuf {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut s = DefaultHasher::new();
        self.root.hash(&mut s);
        let id = format!("{:x}", s.finish());
        let home = std::env::var("HOME").unwrap();
        PathBuf::from(home).join(".hexz").join("workspaces").join(id)
    }
}
