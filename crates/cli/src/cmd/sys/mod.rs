//! System utility commands.

#[cfg(feature = "diagnostics")]
pub mod doctor;

#[cfg(feature = "diagnostics")]
pub mod bench;

#[cfg(feature = "server")]
pub mod serve;

#[cfg(feature = "signing")]
pub mod keygen;

#[cfg(feature = "signing")]
pub mod sign;

#[cfg(feature = "signing")]
pub mod verify;
