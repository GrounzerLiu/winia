//! UI-test fixture: `DropdownMenu` — a popup menu anchored to a trigger.
//!
//! Drives `dropdown_menu_*` in `ui_test.rs`. The menu had no test of its own before this round, so these
//! tests pin the behaviour the component already had (open, item click, dismiss on an outside click, a
//! disabled item doing nothing) before the Compose-alignment work changes its geometry and API.
//!
//! What the fixture shows in the MAIN tree is the state a test can read without the popup:
//! `dm-open`, and `dm-picked` after an item fires. The menu itself lives in an overlay entry, which the
//! harness reads through `overlay_texts()` / `*_overlay_tag`.

use winia::prelude::*;
use winia::ui::overlay::{
    DropdownMenu, DropdownMenuItem, ExposedDropdownMenuAnchorType, ExposedDropdownMenuBox,
    ExposedDropdownMenuDefaults,
};

#[composable]
fn dropdown_menu_fixture(ctx: &mut ComposeCtx) {
    let expanded = ctx.remember(|| false);
    let picked = ctx.remember(|| String::from("none"));
    let many_open = ctx.remember(|| false);

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0))
        .spacing(8.0)
        .build(ctx, |ctx| {
            Text::new("DropdownMenu fixture").font_size(20.0).build(ctx);

            let e = expanded.clone();
            Button::text()
                .on_click(move || {
                    let v = e.get();
                    e.set(!v);
                })
                .modifier(Modifier::new().test_tag("dm-toggle"))
                .build(ctx, |ctx| Text::new("Open menu").build(ctx));

            Text::new(if expanded.get() { "dm-open: yes" } else { "dm-open: no" }).build(ctx);
            Text::new(format!("dm-picked: {}", picked.get())).build(ctx);

            // Anchor: the menu opens below this row (the trigger sits above it, so the popup lands in
            // the empty space under the header where a click cannot hit anything else).
            DropdownMenu::new(expanded.clone())
                .modifier(Modifier::new().test_tag("dm-container"))
                .on_dismiss_request({
                    let e = expanded.clone();
                    move || e.set(false)
                })
                .build(
                    ctx,
                    |ctx| {
                        Text::new("").font_size(1.0).build(ctx);
                    },
                    {
                        let e_new = expanded.clone();
                        let e_rename = expanded.clone();
                        let p_new = picked.clone();
                        let p_rename = picked.clone();
                        move |ctx| {
                            // The menu closure is `Fn` (it runs again on every recomposition), so each
                            // item gets its own clones — the handles are `State<T>` and cheap to clone.
                            let (e_new, p_new) = (e_new.clone(), p_new.clone());
                            DropdownMenuItem::new("新建文件")
                                .modifier(Modifier::new().test_tag("dm-item-new"))
                                .on_click(move || {
                                    p_new.set(String::from("new"));
                                    e_new.set(false);
                                })
                                .build(ctx);
                            let (e_rename, p_rename) = (e_rename.clone(), p_rename.clone());
                            DropdownMenuItem::new("重命名")
                                .modifier(Modifier::new().test_tag("dm-item-rename"))
                                .on_click(move || {
                                    p_rename.set(String::from("rename"));
                                    e_rename.set(false);
                                })
                                .build(ctx);
                            // Disabled: it must not fire, and (per Compose) it must not dismiss either.
                            let p_delete = picked.clone();
                            DropdownMenuItem::new("删除")
                                .modifier(Modifier::new().test_tag("dm-item-delete"))
                                .enabled(false)
                                .on_click(move || p_delete.set(String::from("deleted")))
                                .build(ctx);
                        }
                    },
                );

            // ── A menu longer than the window: material3 caps it and scrolls it. ──
            let m = many_open.clone();
            Button::text()
                .on_click(move || {
                    let v = m.get();
                    m.set(!v);
                })
                .modifier(Modifier::new().test_tag("dm-many-toggle"))
                .build(ctx, |ctx| Text::new("Open long menu").build(ctx));
            Text::new(if many_open.get() { "dm-many-open: yes" } else { "dm-many-open: no" }).build(ctx);

            DropdownMenu::new(many_open.clone())
                .modifier(Modifier::new().test_tag("dm-many-container"))
                .on_dismiss_request({
                    let m = many_open.clone();
                    move || m.set(false)
                })
                .build(
                    ctx,
                    |ctx| {
                        Text::new("").font_size(1.0).build(ctx);
                    },
                    {
                        let m = many_open.clone();
                        move |ctx| {
                            // 20 items × 48dp = 960dp, far taller than the 520px window: material3's
                            // menu caps at the available space and scrolls (`verticalScroll`). Composed
                            // in a loop, so each item needs its own key — the call site is the same line
                            // for all of them.
                            for i in 0..20usize {
                                let m = m.clone();
                                ctx.key(("dm-many-item", i), |ctx| {
                                    DropdownMenuItem::new(format!("长项 {i}"))
                                        .modifier(Modifier::new().test_tag(format!("dm-many-{i}")))
                                        .on_click(move || m.set(false))
                                        .build(ctx);
                                });
                            }
                        }
                    },
                );

            // ── Icons: material3's `leadingIcon` / `trailingIcon`. ──
            // The slot contents are 24dp-wide spacers: what is under test is the box the item wraps them
            // in, and the 12dp it puts between the label and an icon on that side.
            let icons_open = ctx.remember(|| false);
            Button::text()
                .on_click({
                    let i = icons_open.clone();
                    move || {
                        let v = i.get();
                        i.set(!v);
                    }
                })
                .modifier(Modifier::new().test_tag("dm-icons-toggle"))
                .build(ctx, |ctx| Text::new("Open icon menu").build(ctx));

            DropdownMenu::new(icons_open.clone())
                .modifier(Modifier::new().test_tag("dm-icons-container"))
                .on_dismiss_request({
                    let i = icons_open.clone();
                    move || i.set(false)
                })
                .build(
                    ctx,
                    |ctx| {
                        Text::new("").font_size(1.0).build(ctx);
                    },
                    {
                        let icons_open = icons_open.clone();
                        move |ctx| {
                            let i1 = icons_open.clone();
                            DropdownMenuItem::new("带图标")
                                .modifier(Modifier::new().test_tag("dm-item-lead"))
                                .leading_icon(|ctx| {
                                    Spacer::horizontal(24.0)
                                        .modifier(Modifier::new().test_tag("dm-icon-lead"))
                                        .build(ctx);
                                })
                                .on_click(move || i1.set(false))
                                .build(ctx);
                            DropdownMenuItem::new("尾随")
                                .modifier(Modifier::new().test_tag("dm-item-trail"))
                                .trailing_icon(|ctx| {
                                    Spacer::horizontal(24.0)
                                        .modifier(Modifier::new().test_tag("dm-icon-trail"))
                                        .build(ctx);
                                })
                                .on_click({
                                    let closer = icons_open.clone();
                                    move || closer.set(false)
                                })
                                .build(ctx);
                        }
                    },
                );

            // ── Exposed dropdown: material3's `ExposedDropdownMenuBox`. The anchor is a text field 200dp
            // wide, so the menu's width is checkable against it (`matchAnchorWidth`).
            let exposed_open = ctx.remember(|| false);
            let exposed_value = ctx.remember(|| TextFieldValue::new("已选"));
            ExposedDropdownMenuBox::new(exposed_open.clone())
                .on_expanded_change({
                    let e = exposed_open.clone();
                    move |open| e.set(open)
                })
                .build(
                    ctx,
                    {
                        // `move` (and the clone) because the anchor closure is stored: the box's builder
                        // takes it as `'static`, like every other component's content closure here.
                        let exposed_value = exposed_value.clone();
                        let exposed_open = exposed_open.clone();
                        move |ctx| {
                            TextField::new(exposed_value.clone())
                                .outlined()
                                .read_only(true)
                                .label(|ctx| Text::new("选择").build(ctx))
                                .modifier(Modifier::new().width(200.0).test_tag("dm-exposed-anchor"))
                                .trailing_icon({
                                    let arrow_state = exposed_open.clone();
                                    move |ctx| {
                                        ExposedDropdownMenuDefaults::trailing_icon(
                                            ctx,
                                            arrow_state.clone(),
                                            Modifier::new().test_tag("dm-exposed-arrow"),
                                        );
                                    }
                                })
                                .build(ctx);
                        }
                    },
                    {
                        let exposed_open = exposed_open.clone();
                        let exposed_value = exposed_value.clone();
                        move |ctx| {
                            for (index, label) in ["选项 A", "选项 B"].into_iter().enumerate() {
                                let closer = exposed_open.clone();
                                let field = exposed_value.clone();
                                DropdownMenuItem::new(label)
                                    .modifier(Modifier::new().test_tag(format!("dm-exposed-item-{index}")))
                                    .content_padding(ExposedDropdownMenuDefaults::ITEM_HORIZONTAL_PADDING, 0.0)
                                    .on_click(move || {
                                        // The caller's half of an exposed dropdown: the field shows what was
                                        // picked (material3's own samples do this in `onClick`).
                                        field.set(TextFieldValue::new(label));
                                        closer.set(false);
                                    })
                                    .build(ctx);
                            }
                        }
                    },
                );
            Text::new(if exposed_open.get() {
                "dm-exposed-open: yes"
            } else {
                "dm-exposed-open: no"
            })
            .build(ctx);

            // A `PrimaryEditable` anchor: material3's pointer path toggles for every anchor type
            // (`ExposedDropdownMenu.kt:1430-1433`), and the type only decides whether the POPUP takes
            // focus — this one opens WITHOUT it, so the caret (and, on a device, the IME) stays in the
            // field while the items are showing.
            let editable_open = ctx.remember(|| false);
            let editable_value = ctx.remember(|| TextFieldValue::new(""));
            ExposedDropdownMenuBox::new(editable_open.clone())
                .anchor_type(ExposedDropdownMenuAnchorType::PrimaryEditable)
                .build(
                    ctx,
                    {
                        let editable_value = editable_value.clone();
                        move |ctx| {
                            TextField::new(editable_value.clone())
                                .outlined()
                                .modifier(Modifier::new().width(200.0).test_tag("dm-editable-anchor"))
                                .build(ctx);
                        }
                    },
                    {
                        let editable_open = editable_open.clone();
                        move |ctx| {
                            DropdownMenuItem::new("可编辑项")
                                .modifier(Modifier::new().test_tag("dm-editable-item-0"))
                                .on_click({
                                    let closer = editable_open.clone();
                                    move || closer.set(false)
                                })
                                .build(ctx);
                        }
                    },
                );

            // A `SecondaryEditable` anchor: material3 hangs `menuAnchor(SecondaryEditable)` on an element
            // INSIDE the field — an icon — and that element owns the toggle (`ExposedDropdownMenu.kt:449-482`).
            // `build_with_anchor_modifier` is the winia shape for it: the element gets the modifier, and
            // because it takes the press target the click never reaches the field, which is material3's
            // `downEvent.consume()` (`:1427-1429`) expressed in winia's dispatch rule.
            let secondary_open = ctx.remember(|| false);
            let secondary_value = ctx.remember(|| TextFieldValue::new(""));
            ExposedDropdownMenuBox::new(secondary_open.clone())
                .anchor_type(ExposedDropdownMenuAnchorType::SecondaryEditable)
                .build_with_anchor_modifier(
                    ctx,
                    {
                        let secondary_value = secondary_value.clone();
                        let secondary_open = secondary_open.clone();
                        move |ctx, anchor_modifier| {
                            let arrow_state = secondary_open.clone();
                            TextField::new(secondary_value.clone())
                                .outlined()
                                .modifier(
                                    Modifier::new()
                                        .width(200.0)
                                        .test_tag("dm-secondary-anchor"),
                                )
                                .trailing_icon(move |ctx| {
                                    ExposedDropdownMenuDefaults::trailing_icon(
                                        ctx,
                                        arrow_state.clone(),
                                        Modifier::new()
                                            .test_tag("dm-secondary-icon")
                                            .then(anchor_modifier.clone()),
                                    );
                                })
                                .build(ctx);
                        }
                    },
                    {
                        let secondary_open = secondary_open.clone();
                        move |ctx| {
                            DropdownMenuItem::new("次级项")
                                .modifier(Modifier::new().test_tag("dm-secondary-item-0"))
                                .on_click({
                                    let closer = secondary_open.clone();
                                    move || closer.set(false)
                                })
                                .build(ctx);
                        }
                    },
                );
        });
}

/// Scenario entry: `fixture_all` (the single fixture binary) calls this after selecting the scenario from
/// `argv[1]`. It starts the event loop and never returns.
pub fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();
    winia::run_app!(|ctx| {
        WiniaTheme::light(ctx, |ctx| {
            Window::new()
                .size(420.0, 520.0)
                .title("DropdownMenu Fixture")
                .build(ctx, |ctx| dropdown_menu_fixture(ctx));
        });
    });
}
