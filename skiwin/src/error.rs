/// Skiwin render backend errors.
#[derive(Debug, thiserror::Error)]
pub enum SkiwinError {
    #[error("Vulkan error: {0}")]
    Vulkan(String),

    #[error("OpenGL error: {0}")]
    OpenGl(String),

    #[error("softbuffer error: {0}")]
    SoftBuffer(String),

    #[error("the {0} backend is not compiled into this build")]
    UnsupportedBackend(&'static str),

    #[error("surface lost, needs recreation")]
    SurfaceLost,

    #[error("device lost")]
    DeviceLost,

    #[error("no suitable GPU device found")]
    NoDevice,

    #[error("surface format not supported")]
    UnsupportedSurfaceFormat,
}

/// Convenience alias.
pub type SkiwinResult<T> = Result<T, SkiwinError>;
