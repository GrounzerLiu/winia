//! TextField 演示——M3 外观 + text-field-v2 容器化（label/placeholder/图标闭包）
//!
//! 默认样式由 TextField 经 CompositionLocal 提供（对齐 M3 specs）：
//! - label：展开 16sp ↔ 悬浮 12sp 动画字号 + 状态色（LOCAL_TEXT_STYLE）
//! - placeholder：16sp + on_surface_variant + 淡入 alpha（LOCAL_TEXT_STYLE）
//! - prefix/suffix：16sp + affix 色（LOCAL_TEXT_STYLE）
//! - leading/trailing icon：内容色 = on_surface_variant（LOCAL_CONTENT_COLOR）
//! 闭包内组件无需显式样式；可自行覆盖（.font_size()/.color()/.tint()）
//!
//! 分组展示不同配置组合的外观：
//! - 基础：Filled/Outlined + label（展开/悬浮/placeholder）
//! - 图标组合：只有 leading / 只有 trailing / 两者
//! - 前后缀组合：只有 prefix / 只有 suffix / 两者
//! - 全组合：leading + prefix + suffix + trailing
//! - 无 label / 无图标
//! - 多行 / 只读 / 错误 + 支持文本
//!
//! 运行：`cargo run -p winia --example text_field_demo`

use winia::prelude::*;
use winia::ui::{TextField, TextFieldValue};
use winia::ui::text_transformation::{OffsetMapping, PasswordTransformation, TransformedText, VisualTransformation};

/// 自定义视觉变换：仅保留数字，每 4 位加空格分组（123456789012 → 1234 5678 9012）。
///
/// 展示 `VisualTransformation` + `OffsetMapping` 自定义实现（对标 Compose
/// 自定义变换）：显示文本 ≠ 编辑文本，光标/选区/点击定位经 OffsetMapping
/// 自动转换（文本仍存原始数字，显示为分组格式）。
#[derive(Debug)]
struct GroupedDigitTransformation;

impl VisualTransformation for GroupedDigitTransformation {
    fn filter(&self, text: &str) -> TransformedText {
        // 原始文本中每个数字的字节起点（非数字被丢弃——演示"只接受数字"）
        let digit_offsets: Vec<usize> = text
            .char_indices()
            .filter(|(_, c)| c.is_ascii_digit())
            .map(|(i, _)| i)
            .collect();
        // 每 4 位分组加空格
        let mut grouped = String::new();
        for (i, &off) in digit_offsets.iter().enumerate() {
            if i > 0 && i % 4 == 0 {
                grouped.push(' ');
            }
            grouped.push(text[off..].chars().next().unwrap());
        }
        TransformedText {
            text: grouped,
            offset_mapping: std::sync::Arc::new(GroupedDigitMapping {
                digit_offsets,
                orig_len: text.len(),
            }),
        }
    }
}

impl From<GroupedDigitTransformation> for std::sync::Arc<dyn VisualTransformation> {
    fn from(t: GroupedDigitTransformation) -> Self {
        std::sync::Arc::new(t)
    }
}

/// 分组偏移映射：显示偏移 ↔ 编辑偏移。
/// - 编辑偏移 o → 显示：o 前 n 个数字 → 每 4 个数字后 1 空格
///   （第 4k 个后，光标在末尾时最后组无空格 → (n-1)/4）
/// - 显示偏移 d → 编辑：每 5 显示字节 = 4 数字 + 1 空格（除末尾组）→ d - d/5
#[derive(Debug)]
struct GroupedDigitMapping {
    /// 原始文本每个数字的字节起点
    digit_offsets: Vec<usize>,
    /// 原始文本总字节
    orig_len: usize,
}

impl OffsetMapping for GroupedDigitMapping {
    fn original_to_transformed(&self, offset: usize) -> usize {
        let n = self.digit_offsets.partition_point(|&d| d < offset.min(self.orig_len));
        n + n.saturating_sub(1) / 4
    }

    fn transformed_to_original(&self, offset: usize) -> usize {
        let digits = offset - offset / 5;
        if digits == 0 {
            0
        } else if digits >= self.digit_offsets.len() {
            self.orig_len
        } else {
            self.digit_offsets[digits]
        }
    }
}

fn section_title(ctx: &mut ComposeCtx, title: &str) {
    Text::new(title)
        .font_size(14.0)
        .color(WiniaTheme::colors().on_surface_variant)
        .build(ctx);
}

#[composable]
fn text_field_ui(ctx: &mut ComposeCtx) {
    // ── 状态值 ──
    // 垂直滚动（内容超出窗口高度时滚动查看）
    let scroll_y = ctx.remember(|| ScrollState::new()).get();

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0).vertical_scroll(scroll_y))
        .build(ctx, |ctx| {
            // ═══ 1. 基础：label 展开（未聚焦空值）═══
            section_title(ctx, "1. Filled + label（展开态——空值未聚焦）");
            TextField::new(ctx.remember(|| TextFieldValue::new("")), |_| {})
                .filled()
                .label(|ctx| { Text::new("Username").build(ctx); })
                .placeholder(|ctx| { Text::new("Enter username").build(ctx); })
                .build(ctx);

            section_title(ctx, "2. Outlined + label（展开态）");
            TextField::new(ctx.remember(|| TextFieldValue::new("")), |_| {})
                .outlined()
                .label(|ctx| { Text::new("Email").build(ctx); })
                .placeholder(|ctx| { Text::new("you@example.com").build(ctx); })
                .build(ctx);

            // ═══ 3. 只有 leading 图标 ═══
            section_title(ctx, "3. Filled + leading icon only");
            TextField::new(ctx.remember(|| TextFieldValue::new("")), |_| {})
                .filled()
                .label(|ctx| { Text::new("Search").build(ctx); })
                .placeholder(|ctx| { Text::new("Search anything").build(ctx); })
                .leading_icon(|ctx| {
                    Icon::new(IconSource::svg_path("M15.5 14h-.79l-.28-.27C15.41 12.59 16 11.11 16 9.5 16 5.91 13.09 3 9.5 3S3 5.91 3 9.5 5.91 16 9.5 16c1.61 0 3.09-.59 4.23-1.57l.27.28v.79l5 4.99L20.49 19l-4.99-5zm-6 0C7.01 14 5 11.99 5 9.5S7.01 5 9.5 5 14 7.01 14 9.5 11.99 14 9.5 14z"))
                        .tint(WiniaTheme::colors().on_surface_variant)
                        .build(ctx);
                })
                .build(ctx);

            // ═══ 4. 只有 trailing 图标 ═══
            section_title(ctx, "4. Outlined + trailing icon only");
            TextField::new(ctx.remember(|| TextFieldValue::new("Alice")), |_| {})
                .outlined()
                .label(|ctx| { Text::new("Name").build(ctx); })
                .trailing_icon(|ctx| {
                    Icon::new(IconSource::svg_path("M19 6.41 17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12z"))
                        .tint(WiniaTheme::colors().on_surface_variant)
                        .build(ctx);
                })
                .build(ctx);

            // ═══ 5. leading + trailing（无前后缀）═══
            section_title(ctx, "5. Filled + leading + trailing icons");
            TextField::new(ctx.remember(|| TextFieldValue::new("13812345678")), |_| {})
                .filled()
                .label(|ctx| { Text::new("Phone").build(ctx); })
                .leading_icon(|ctx| {
                    Icon::new(IconSource::svg_path("M6.62 10.79c1.44 2.83 3.76 5.14 6.59 6.59l2.2-2.2c.27-.27.67-.36 1.02-.24 1.12.37 2.33.57 3.57.57.55 0 1 .45 1 1V20c0 .55-.45 1-1 1-9.39 0-17-7.61-17-17 0-.55.45-1 1-1h3.5c.55 0 1 .45 1 1 0 1.25.2 2.45.57 3.57.11.35.03.74-.25 1.02l-2.2 2.2z"))
                        .tint(WiniaTheme::colors().on_surface_variant)
                        .build(ctx);
                })
                .trailing_icon(|ctx| {
                    Icon::new(IconSource::svg_path("M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm-2 15-5-5 1.41-1.41L10 14.17l7.59-7.59L19 8l-9 9z"))
                        .tint(WiniaTheme::colors().on_surface_variant)
                        .build(ctx);
                })
                .build(ctx);

            // ═══ 6. 只有 prefix ═══
            section_title(ctx, "6. Outlined + prefix only");
            TextField::new(ctx.remember(|| TextFieldValue::new("")), |_| {})
                .outlined()
                .label(|ctx| { Text::new("Amount").build(ctx); })
                .placeholder(|ctx| { Text::new("0.00").build(ctx); })
                .prefix(|ctx| {
                    Text::new("$ ").font_size(16.0).color(WiniaTheme::colors().on_surface_variant).build(ctx);
                })
                .build(ctx);

            // ═══ 7. 只有 suffix ═══
            section_title(ctx, "7. Filled + suffix only");
            TextField::new(ctx.remember(|| TextFieldValue::new("1234.56")), |_| {})
                .filled()
                .label(|ctx| { Text::new("Total").build(ctx); })
                .suffix(|ctx| {
                    Text::new(" USD").font_size(16.0).color(WiniaTheme::colors().on_surface_variant).build(ctx);
                })
                .build(ctx);

            // ═══ 8. prefix + suffix（无图标）═══
            section_title(ctx, "8. Outlined + prefix + suffix");
            TextField::new(ctx.remember(|| TextFieldValue::new("")), |_| {})
                .outlined()
                .label(|ctx| { Text::new("Range").build(ctx); })
                .placeholder(|ctx| { Text::new("0 - 100").build(ctx); })
                .prefix(|ctx| {
                    Text::new("[ ").font_size(16.0).color(WiniaTheme::colors().on_surface_variant).build(ctx);
                })
                .suffix(|ctx| {
                    Text::new(" ]").font_size(16.0).color(WiniaTheme::colors().on_surface_variant).build(ctx);
                })
                .build(ctx);

            // ═══ 9. 全组合：leading + prefix + input + suffix + trailing ═══
            section_title(ctx, "9. Filled + leading + prefix + suffix + trailing");
            TextField::new(ctx.remember(|| TextFieldValue::new("13812345678")), |_| {})
                .filled()
                .label(|ctx| { Text::new("Phone").build(ctx); })
                .leading_icon(|ctx| {
                    Icon::new(IconSource::svg_path("M12 12c2.21 0 4-1.79 4-4s-1.79-4-4-4-4 1.79-4 4 1.79 4 4 4zm0 2c-2.67 0-8 1.34-8 4v2h16v-2c0-2.66-5.33-4-8-4z"))
                        .tint(WiniaTheme::colors().on_surface_variant)
                        .build(ctx);
                })
                .prefix(|ctx| {
                    Text::new("+86 ").font_size(16.0).color(WiniaTheme::colors().on_surface_variant).build(ctx);
                })
                .suffix(|ctx| {
                    Text::new(" 🇨🇳").font_size(16.0).color(WiniaTheme::colors().on_surface_variant).build(ctx);
                })
                .trailing_icon(|ctx| {
                    Icon::new(IconSource::svg_path("M19 6.41 17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12z"))
                        .tint(WiniaTheme::colors().on_surface_variant)
                        .build(ctx);
                })
                .build(ctx);

            // ═══ 10. 无 label（placeholder 直接显示）═══
            section_title(ctx, "10. 无 label——placeholder 恒显示（空值时）");
            TextField::new(ctx.remember(|| TextFieldValue::new("")), |_| {})
                .filled()
                .placeholder(|ctx| { Text::new("No label, just placeholder").build(ctx); })
                .build(ctx);

            // ═══ 11. 长内容（折行/超宽）═══
            section_title(ctx, "11. 长内容 + suffix（折行）");
            TextField::new(ctx.remember(|| TextFieldValue::new("A very long input value that should wrap or overflow the container to test layout behavior")), |_| {})
                .filled()
                .label(|ctx| { Text::new("Description").build(ctx); })
                .suffix(|ctx| {
                    Text::new(" #tag").font_size(16.0).color(WiniaTheme::colors().on_surface_variant).build(ctx);
                })
                .build(ctx);

            // ═══ 12. 多行 ═══
            section_title(ctx, "12. 多行（min_lines 3）");
            TextField::new(ctx.remember(|| TextFieldValue::new("Line one
Line two
Line three")), |_| {})
                .filled()
                .label(|ctx| { Text::new("Notes").build(ctx); })
                .min_lines(3)
                .build(ctx);

            // ═══ 13. 只读 ═══
            section_title(ctx, "13. Outlined + 只读");
            TextField::new(ctx.remember(|| TextFieldValue::new("")), |_| {})
                .outlined()
                .read_only(true)
                .label(|ctx| { Text::new("Readonly").build(ctx); })
                .placeholder(|ctx| { Text::new("Cannot edit").build(ctx); })
                .leading_icon(|ctx| {
                    Icon::new(IconSource::svg_path("M18 8h-1V6c0-2.76-2.24-5-5-5S7 3.24 7 6v2H6c-1.1 0-2 .9-2 2v10c0 1.1.9 2 2 2h12c1.1 0 2-.9 2-2V10c0-1.1-.9-2-2-2zm-6 9c-1.1 0-2-.9-2-2s.9-2 2-2 2 .9 2 2-.9 2-2 2zm3.1-9H8.9V6c0-1.71 1.39-3.1 3.1-3.1 1.71 0 3.1 1.39 3.1 3.1v2z"))
                        .tint(WiniaTheme::colors().on_surface_variant)
                        .build(ctx);
                })
                .build(ctx);

            // ═══ 14. 错误 + 支持文本 ═══
            section_title(ctx, "14. Outlined 错误 + supporting");
            TextField::new(ctx.remember(|| TextFieldValue::new("")), |_| {})
                .outlined()
                .label(|ctx| { Text::new("Password").build(ctx); })
                .placeholder(|ctx| { Text::new("8+ characters").build(ctx); })
                .is_error(true)
                .supporting_text("Password must be at least 8 characters")
                .build(ctx);

            // ═══ 15. Filled 错误 + 图标 + 支持文本（全状态）═══
            section_title(ctx, "15. Filled 错误 + leading + supporting");
            TextField::new(ctx.remember(|| TextFieldValue::new("")), |_| {})
                .filled()
                .label(|ctx| { Text::new("Card number").build(ctx); })
                .placeholder(|ctx| { Text::new("1234 5678 9012 3456").build(ctx); })
                .leading_icon(|ctx| {
                    Icon::new(IconSource::svg_path("M20 4H4c-1.11 0-1.99.89-1.99 2L2 18c0 1.11.89 2 2 2h16c1.11 0 2-.89 2-2V6c0-1.11-.89-2-2-2zm0 14H4v-6h16v6zm0-10H4V6h16v2z"))
                        .tint(WiniaTheme::colors().error)
                        .build(ctx);
                })
                .is_error(true)
                .supporting_text("Card number invalid")
                .build(ctx);

            // ═══ 16. 禁用 ═══
            section_title(ctx, "16. 禁用（Filled + Outlined）");
            TextField::new(ctx.remember(|| TextFieldValue::new("Locked filled")), |_| {})
                .filled()
                .label(|ctx| { Text::new("Filled disabled").build(ctx); })
                .enabled(false)
                .build(ctx);
            TextField::new(ctx.remember(|| TextFieldValue::new("Locked outlined")), |_| {})
                .outlined()
                .label(|ctx| { Text::new("Outlined disabled").build(ctx); })
                .enabled(false)
                .build(ctx);

            // ═══ 17. 视觉变换（visual_transformation）═══
            section_title(ctx, "17. 视觉变换（密码掩码 + 格式化分组）");
            TextField::new(ctx.remember(|| TextFieldValue::new("")), |_| {})
                .outlined()
                .label(|ctx| { Text::new("Password").build(ctx); })
                .placeholder(|ctx| { Text::new("Hidden input").build(ctx); })
                .visual_transformation(PasswordTransformation::new('•'))
                .supporting_text("内置：输入显示为圆点掩码（编辑内容保留）")
                .build(ctx);
            TextField::new(ctx.remember(|| TextFieldValue::new("1234567890123456")), |_| {})
                .filled()
                .label(|ctx| { Text::new("Card number").build(ctx); })
                .visual_transformation(GroupedDigitTransformation)
                .supporting_text("自定义：每 4 位自动分组（非数字丢弃）")
                .build(ctx);
            TextField::new(ctx.remember(|| TextFieldValue::new("")), |_| {})
                .no_container()
                .label(|ctx| { Text::new("PIN").build(ctx); })
                .visual_transformation(PasswordTransformation::new('•'))
                .supporting_text("裸输入（no_container）+ 掩码——变换不依赖 M3 容器")
                .build(ctx);
        });
}

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(520.0, 700.0)
                .title("TextField Test")
                .build(ctx, text_field_ui);
        });
    });
}
