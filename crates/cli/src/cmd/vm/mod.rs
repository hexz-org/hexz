//! Virtual machine operation commands.

#[cfg(feature = "fuse")]
pub mod boot;

#[cfg(feature = "fuse")]
pub mod install;

pub mod commit;
pub mod snap;

#[cfg(feature = "fuse")]
pub mod mount;

#[cfg(feature = "fuse")]
pub mod unmount;
