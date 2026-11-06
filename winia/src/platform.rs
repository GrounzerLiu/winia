//! Platform-specific integrations.
//!
//! Android JNI code lives in a separate module so the generic composition and runtime code does not
//! depend on JNI types.

#[cfg(target_os = "android")]
pub mod android;
