use super::*;

// ── 顶层弹出层（Popup/Dialog/DropdownMenu） ──

impl OverlayWindow {
    pub(crate) fn new_with_composer(desc: crate::overlay::OverlayDesc, composer: Composer) -> Self {
        Self {
            id: desc.id,
            composer,
            anchor_slot: desc.anchor_slot,
            direction: desc.direction,
            position: desc.position,
            offset: desc.offset,
            dismiss_on_back_press: desc.dismiss_on_back_press,
            dismiss_defers_to_content: desc.dismiss_defers_to_content,
            anchor_slide: desc.anchor_slide,
            modal: desc.modal,
            dismiss_on_outside: desc.dismiss_on_outside,
            click_passthrough: desc.click_passthrough,
            fit_around_anchor: desc.fit_around_anchor,
            match_anchor_width: desc.match_anchor_width,
            on_dismiss: desc.on_dismiss,
            content: desc.content,
            local_snapshot: desc.local_snapshot,
            screen_pos: (0.0, 0.0),
            anchor_rect: None,
            hovered_slots: std::collections::HashSet::new(),
            pressed_interaction: None,
            // progress is driven only when an enter or exit spec exists (0->1
            // enter, 1->0 exit); otherwise None means always visible (instant).
            progress: (desc.enter_anim.is_some() || desc.exit_anim.is_some())
                .then(|| crate::runtime::state::Animating::new(0.0)),
            enter_anim: desc.enter_anim,
            exit_anim: desc.exit_anim,
            closing: false,
            closing_since: None,
            focused_id: None,
            focused_slot_key: None,
            focus_scope: desc.focus_scope,
            claimed_keyboard: false,
        }
    }

    /// Undo a close in progress: this overlay is visible again, so its show-progress goes back to 1.
    ///
    /// The exit tween drove it to 0 and nothing else brings it back — the enter animation is pushed
    /// only when an overlay is CREATED or when its animation specs flip None<->Some. Without this the
    /// overlay is drawn at alpha 0 while `hit_overlay` (which never looks at alpha) keeps swallowing
    /// clicks in its rect and `keyboard_scope` keeps granting it the keyboard: an invisible panel you
    /// cannot get rid of, reached by closing and letting the caller re-declare the overlay.
    pub(crate) fn resume_after_close(&mut self) {
        self.closing = false;
        self.closing_since = None;
        // It is the keyboard's owner again, so the page's suspended focus has to be suspended afresh
        // (the release already handed it back).
        self.claimed_keyboard = false;
        let Some(progress) = self.progress.clone() else { return };
        match &self.enter_anim {
            Some(spec) => {
                crate::animation::push_animatable_handle(progress, 1.0, spec.animation_spec());
            }
            // No enter animation to replay: the render path reads progress for the exit spec too, so
            // a stale 0 would keep it hidden — show it outright.
            None => progress.as_raw().set_backchannel(1.0),
        }
    }

    fn update(&mut self, desc: crate::overlay::OverlayDesc) {
        // A desc only reaches this method while its overlay is still DECLARED (a closing overlay is not
        // re-registered), so the caller bringing one back — open, close, open again inside the exit fade,
        // where the id is reused — has to cancel the pending close: leaving `closing` set kept the overlay
        // un-interactive (`hit_overlay` skips closing ones) and let the fade remove it a moment after it
        // was reopened, which showed as a panel flashing.
        let was_closing = self.closing;
        self.closing = false;
        self.closing_since = None;
        self.anchor_slot = desc.anchor_slot;
        self.position = desc.position;
        self.anchor_slide = desc.anchor_slide;
        self.offset = desc.offset;
        self.modal = desc.modal;
        self.focus_scope = desc.focus_scope;
        self.dismiss_on_outside = desc.dismiss_on_outside;
        self.dismiss_on_back_press = desc.dismiss_on_back_press;
        self.click_passthrough = desc.click_passthrough;
        self.fit_around_anchor = desc.fit_around_anchor;
        self.match_anchor_width = desc.match_anchor_width;
        self.on_dismiss = desc.on_dismiss;
        self.content = desc.content;
        self.local_snapshot = desc.local_snapshot;
        // Animation spec update (when a reused overlay's specs change) — rebuild
        // progress on None<->Some flip (None->Some: new driver; Some->None:
        // leftover State retired — render falls to the `_` branch, always visible,
        // and close removes instantly with has_exit=false).
        let old_progress = self.progress.clone();
        let spec_flipped = desc.enter_anim.is_some() != self.enter_anim.is_some()
            || desc.exit_anim.is_some() != self.exit_anim.is_some();
        self.enter_anim = desc.enter_anim;
        self.exit_anim = desc.exit_anim;
        if spec_flipped {
            // Cancel any in-flight animation on the old progress handle
            // (otherwise it lingers as an orphan until timeout — P1-4).
            if let Some(old) = old_progress {
                crate::animation::cancel_animation_by_id(old.state_id());
            }
            self.progress = (self.enter_anim.is_some() || self.exit_anim.is_some())
                .then(|| crate::runtime::state::Animating::new(0.0));
            // Update is the reuse path — overlay is already visible; progress=0
            // would render as apply(0) hidden. If the new spec has no enter
            // animation, keep it fully visible (Backchannel-equivalent direct
            // write on the Animating handle — same stored value); otherwise the
            // caller will push the 0->1 enter animation.
            if let Some(p) = self.progress.as_ref() {
                if self.enter_anim.is_none() {
                    p.as_raw().set_backchannel(1.0);
                }
            }
        }
        // The caller brought back an overlay that was fading out (open, close, open inside the exit):
        // the fade has to be undone, not just the flag.
        if was_closing {
            self.resume_after_close();
        }
    }
}

/// 主树 compose 后同步 overlay：按 id 匹配（保留 State）——新增/更新/删除。
/// 删除依据 = 组合期显式记录的 active=false（Popup/Dialog/DropdownMenu build
/// 总执行时 record_overlay_active）——主动关闭（visible/expanded=false）→ 删；
/// 注册方 Skip（build 未执行 → 本帧无记录）→ 保留（闪烁根因：旧 retain 把
/// Skip 帧误判为主动关闭）。
pub(crate) fn sync_overlays(pw: &mut PerWindow, _recomposed: bool) {
    let descs = pw.composer.take_overlays();
    for desc in descs {
        if let Some(ov) = pw.overlay.layers.iter_mut().find(|o| o.id == desc.id) {
            // 复用：规格翻转（None↔Some）时 update 重建 progress——新规格有
            // enter 动画则 push 0→1（否则 update 内已 Backchannel 写 1.0 保持显示）
            let had_enter = ov.enter_anim.is_some();
            ov.update(desc);
            // ⚠ The caller just recomposed and handed us a NEW content closure, so the overlay's whole
            // content has to run again. An overlay is a separate composer: nothing inside it can see
            // that its closure was replaced, and a group re-enters only for state it read or
            // parameters it declared — so content built from values the caller captured (a filtered
            // list, a formatted string) kept what the FIRST closure captured. Marking the root alone
            // is not enough: the caller's lambda usually sits inside wrapper groups of its own (a
            // `Column`, a `Surface`), which declare nothing and would skip. Measured on
            // `search_bar_demo`: the fullscreen panel and the docked dropdown both stayed on their
            // first rows while the input field updated.
            //
            // Cost is bounded by how often the caller's group runs, not by this call: a screen whose
            // main tree composes every frame re-runs the overlay content every frame too, while one
            // that idles skips both composers. Nothing leaks across frames (`Slot::dirty` is consumed
            // by the pass that reads it), a `LazyColumn` keeps its scroll position, and a closing
            // overlay is not re-registered at all.
            ov.composer.mark_content_dirty();
            let now_has_enter = ov.enter_anim.is_some();
            if !had_enter && now_has_enter {
                if let Some(p) = ov.progress.clone() {
                    let spec = ov.enter_anim.as_ref().unwrap();
                    crate::animation::push_animatable_handle(
                        p, 1.0,
                        spec.animation_spec(),
                    );
                }
            }
        } else {
            // 新 overlay：创建 + 启动进入动画（progress 0→1；无 enter 规格 =
            // 瞬时显示——但 exit 有规格时 progress 直接置 1.0，退出才能 1→0）
            // 共享主窗 AdaptiveContext，确保 overlay 读 window_size/density 与主窗一致
            let mut composer = Composer::new();
            composer.adaptive = pw.composer.adaptive.clone();
            // The overlay is its own composer, but it belongs to the same window: frame_clock()
            // inside it has to wait on THIS window's frames rather than on window 0's.
            composer.set_window_id(pw.composer.window_id());
            // And it saves into the same place: overlay content that calls remember_saveable has to
            // be snapshotted with the page it belongs to.
            if let Some(registry) = pw.composer.saveable_registry() {
                composer.set_saveable_registry(registry);
            }
            let enter_anim = desc.enter_anim.clone();
            pw.overlay.layers.push(OverlayWindow::new_with_composer(desc, composer));
            if let Some(p) = pw.overlay.layers.last().and_then(|o| o.progress.clone()) {
                if let Some(spec) = &enter_anim {
                    crate::animation::push_animatable_handle(
                        p, 1.0,
                        spec.animation_spec(),
                    );
                } else {
                    // 无进入动画（但 exit 有）：直接完整显示（progress=1）
                    p.as_raw().set_backchannel(1.0);
                }
            }
        }
    }
    // 组合期记录 active=false 的 overlay → 主动关闭 → 启动退出动画
    // （先触发 on_dismiss；动画完成由 finish_closing_overlays 移除——不复位
    // 则保留渲染播放退出动画）
    let to_close: Vec<u64> = pw.composer.overlay_active.iter()
        .filter(|(_, active)| !**active)
        .map(|(&id, _)| id)
        .collect();
    for id in &to_close {
        begin_overlay_close(pw, *id);
    }
    // 退出动画完成检测：closing 且 progress≈0（或无动画规格）→ 真正移除
    finish_closing_overlays(pw);
}

