//! System utility commands for diagnostics and services.

pub mod doctor;

#[cfg(feature = "server")]
pub mod serve;

#[cfg(feature = "signing")]
pub mod keygen;

#[cfg(feature = "signing")]
pub mod sign;

#[cfg(feature = "signing")]
pub mod verify;
