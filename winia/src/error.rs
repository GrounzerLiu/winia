/// Winia 框架核心错误类型
#[derive(Debug, thiserror::Error)]
pub enum WiniaError {
    // ── 窗口管理 ──
    #[error("failed to create window: {0}")]
    WindowCreate(String),

    #[error("window not found: id={0}")]
    WindowNotFound(u32),

    #[error("event loop not initialized")]
    EventLoopNotInitialized,

    // ── 渲染 ──
    #[error("render error: {0}")]
    Render(String),

    #[error("surface lost, needs recreation")]
    SurfaceLost,

    // ── 布局/测量 ──
    #[error("layout error: {0}")]
    Layout(String),

    // ── 文本 ──
    #[error("text index {index} out of bounds (length={length})")]
    TextIndexOutOfBounds { index: usize, length: usize },

    #[error("invalid text range: {start}..{end} (length={length})")]
    InvalidTextRange {
        start: usize,
        end: usize,
        length: usize,
    },

    #[error("font load failed: {0}")]
    FontLoad(String),

    // ── 动画 ──
    #[error("keyframes animation requires at least 2 keyframes, got {0}")]
    InsufficientKeyframes(usize),

    // ── 主题 ──
    #[error("theme color not found: {0}")]
    ThemeColorNotFound(String),

    // ── 通用 ──
    #[error("internal error: {0}")]
    Internal(String),
}

/// 便捷 Result 类型别名
pub type WiniaResult<T> = Result<T, WiniaError>;
