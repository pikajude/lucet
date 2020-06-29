// CPU affinity using the `core_affinity` crate

#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
mod affinity_core_affinity;

#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
pub use affinity_core_affinity::tune;

// CPU affinity using the `hwloc` library

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
mod affinity_hwloc;

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
pub use affinity_hwloc::tune;
