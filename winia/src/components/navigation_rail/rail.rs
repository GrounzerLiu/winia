use super::*;

// ── NavigationRail 容器 ──

pub struct NavigationRail {
    pub(crate) content: Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>,
    pub(crate) header: Option<Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>>,
    pub(crate) container_color: Option<Color>,
    pub(crate) window_insets: WindowInsets,
    pub(crate) modifier: Modifier,
}

impl NavigationRail {
    pub fn new(content: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        Self {
            content: Box::new(content),
            header: None,
            container_color: None,
            window_insets: WindowInsets::default(),
            modifier: Modifier::new(),
        }
    }

    /// 容器底色（默认 theme.surface——CollapsedTokens.ContainerColor）
    pub fn container_color(mut self, c: Color) -> Self {
        self.container_color = Some(c);
        self
    }

    /// header 槽（通常放 FAB 或 logo）——之后自动插入 8dp Spacer
    pub fn header(mut self, h: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        self.header = Some(Box::new(h));
        self
    }

    /// 窗口避让（桌面默认零值——见 [WindowInsets] 文档）
    pub fn window_insets(mut self, insets: WindowInsets) -> Self {
        self.window_insets = insets;
        self
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        ctx.changed(&self.container_color);
        let theme = WiniaTheme::colors();
        let container = self.container_color.unwrap_or(theme.surface);
        let insets = self.window_insets;
        let header = self.header;
        let content = self.content;
        // Surface(container) → Column(fillMaxHeight, widthIn(min 80),
        //   windowInsetsPadding, padding(vertical 4), spacedBy(4),
        //   CenterHorizontally)
        Column::new()
            .alignment(Alignment::Center)
            .spacing(RAIL_VERTICAL_PADDING)
            .modifier(
                Modifier::new()
                    .fill_max_height()
                    .min_width(NAVIGATION_RAIL_WIDTH)
                    .background(container, Shape::Rectangle)
                    .padding_top(insets.top)
                    .padding_bottom(insets.bottom)
                    .padding_start(insets.left)
                    .padding_end(insets.right)
                    .padding_vertical(RAIL_VERTICAL_PADDING)
                    .then(self.modifier),
            )
            .build(ctx, |ctx| {
                WiniaTheme::with_content_color(theme.on_surface, ctx, |ctx| {
                    if let Some(header) = header {
                        header(ctx);
                        crate::layout::Spacer::vertical(HEADER_SPACER).build(ctx);
                    }
                    content(ctx);
                });
            });
    }
}

