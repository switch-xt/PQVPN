#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(target_os = "windows")]
pub mod manager;


#[cfg(target_os = "windows")]
pub use windows::*;
