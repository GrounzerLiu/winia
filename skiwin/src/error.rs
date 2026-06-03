/// skiwin 渲染后端错误类型
#[derive(Debug, thiserror::Error)]
pub enum SkiwinError {
    #[error("Vulkan error: {0}")]
    Vulkan(String),

    #[error("OpenGL error: {0}")]
    OpenGl(String),

    #[error("softbuffer error: {0}")]
    SoftBuffer(String),

    #[error("surface lost, needs recreation")]
    SurfaceLost,

    #[error("device lost")]
    DeviceLost,

    #[error("no suitable GPU device found")]
    NoDevice,

    #[error("surface format not supported")]
    UnsupportedSurfaceFormat,
}

/// 便捷 Result 类型别名
pub type SkiwinResult<T> = Result<T, SkiwinError>;
