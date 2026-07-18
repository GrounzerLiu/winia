//! Button 组件 — 可点击按钮
//!
//! 用法:
//! ```ignore
//! Button::new()
//!     .on_click(|| println!("clicked"))
//!     .modifier(Modifier::new().size(200.0, 48.0).background(Color::BLUE))
//!     .build(ctx, |ctx| {
//!         Text::new("Click me").color(Color::WHITE).build(ctx);
//!     });
//! ```

use crate::core::composer::ComposeCtx;
use crate::layout::BoxLayout;
use crate::modifier::Modifier;
use std::sync::Arc;
use std::fmt;

/// Button 变体风格
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonStyle {
    /// 实心填充按钮（主要操作）
    Filled,
    /// 轮廓按钮（次要操作）
    Outlined,
    /// 纯文本按钮（最低强调）
    Text,
    /// 带浅色背景的填充按钮
    Tonal,
}

impl Default for ButtonStyle {
    fn default() -> Self {
        ButtonStyle::Filled
    }
}

/// Button 组件 Builder
///
/// 声明式按钮组件。通过链式方法配置点击行为、样式、状态等。
/// 子内容通过 `build(ctx, |ctx| { ... })` 传入（通常是 Text + Icon）。
///
/// # 示例
/// ```ignore
/// Button::new()
///     .style(ButtonStyle::Filled)
///     .on_click(|| count.update(|v| *v += 1))
///     .enabled(true)
///     .modifier(Modifier::new()
///         .size(200.0, 48.0)
///         .background(Color::BLUE, Shape::rounded(8.0))
///     )
///     .build(ctx, |ctx| {
///         Text::new("Increment").color(Color::WHITE).build(ctx);
///     });
/// ```
#[derive(Clone)]
pub struct Button {
    /// 点击回调
    on_click: Option<Arc<dyn Fn() + Send + Sync>>,
    /// 是否启用
    enabled: bool,
    /// 按钮风格
    style: ButtonStyle,
    /// 修饰符链（尺寸、颜色、形状等）
    modifier: Modifier,
}

impl Button {
    /// 创建新的 Button 组件
    pub fn new() -> Self {
        Button {
            on_click: None,
            enabled: true,
            style: ButtonStyle::default(),
            modifier: Modifier::new(),
        }
    }

    /// 设置点击回调
    pub fn on_click(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_click = Some(Arc::new(f));
        self
    }

    /// 设置启用状态
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// 设置按钮风格
    pub fn style(mut self, style: ButtonStyle) -> Self {
        self.style = style;
        self
    }

    /// 设置修饰符链
    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = modifier;
        self
    }

    /// 注册到组合树并执行子内容。
    pub fn build(self, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        let key = ctx.next_key();

        // 合并 modifier：用户 modifier + clickable 行为
        let mut modifier = self.modifier;
        if self.enabled {
            if let Some(on_click) = &self.on_click {
                let cb = on_click.clone();
                modifier = modifier.clickable(move || cb());
            }
        }

        // 注册为容器节点（用 BoxLayout 层叠子内容）
        ctx.start_container(key, modifier, BoxLayout::new());
        content(ctx);
        ctx.end_node();
    }

    // ── Getters（测试用）──
    pub fn get_enabled(&self) -> bool { self.enabled }
    pub fn get_style(&self) -> ButtonStyle { self.style }
    pub fn get_modifier(&self) -> &Modifier { &self.modifier }
}

impl Default for Button {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for Button {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Button")
            .field("enabled", &self.enabled)
            .field("style", &self.style)
            .field("modifier", &self.modifier)
            .field("on_click", &self.on_click.as_ref().map(|_| "<fn>"))
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::composer::Composer;

    #[test]
    fn test_button_defaults() {
        let btn = Button::new();
        assert!(btn.get_enabled());
        assert_eq!(btn.get_style(), ButtonStyle::Filled);
        assert_eq!(btn.get_modifier().elements().len(), 0);
    }

    #[test]
    fn test_button_builder() {
        let clicked = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let clicked_clone = clicked.clone();

        let btn = Button::new()
            .on_click(move || clicked_clone.store(true, std::sync::atomic::Ordering::SeqCst))
            .enabled(false)
            .style(ButtonStyle::Outlined)
            .modifier(Modifier::new().size(200.0, 48.0));

        assert!(!btn.get_enabled());
        assert_eq!(btn.get_style(), ButtonStyle::Outlined);
        assert_eq!(btn.get_modifier().elements().len(), 1);

        // 由于 on_click 被移动到 Button 中，测试完成后 drop
    }

    #[test]
    fn test_button_composition() {
        let mut composer = Composer::new();

        composer.compose(|ctx| {
            let _btn = Button::new()
                .modifier(Modifier::new().size(100.0, 40.0));

            // 模拟 build 过程：Button 是容器
            let key = ctx.next_key();
            ctx.start_container(key, Modifier::new().size(100.0, 40.0), BoxLayout::new());
            // 子内容（模拟 Text）
            let text_key = ctx.next_key();
            ctx.start_leaf(text_key, Modifier::new());
            ctx.end_node();
            ctx.end_node();
        });

        // 组合树应该正确构建
        assert!(composer.layout_root().is_some());
    }
}
