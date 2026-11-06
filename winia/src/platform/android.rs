//! Android platform integration.
//!
//! JNI-specific helpers live in the `jni` submodule so the rest of the platform facade stays free
//! of JNI types.

use crate::runtime::composer::AndroidUi;

pub mod jni;
pub(crate) mod insets;
pub use jni::AndroidJni;

impl AndroidUi<'_> {
    /// Return the JNI facade for this activity.
    pub fn jni(&self) -> AndroidJni<'_> {
        AndroidJni::new(self.app)
    }
}
