use super::*;

// ═══════════════════════════════════════════════════════════
// NavEntryDecorator — entry 包装/生命周期（对标 Nav3 的 NavEntryDecorator）
// ═══════════════════════════════════════════════════════════

/// 导航条目装饰器——包装 entry 内容 + 生命周期回调。
///
/// 对标 Nav3 `NavEntryDecorator(onPop, decorate)`：
/// - `on_pop`：entry 从 back stack 移除时回调（对标 Nav3 onPop——如状态清理）。
///   广播语义：列表内全部装饰器依次收到回调。参数为 **contentKey**（对标 Nav3
///   onPop(contentKey)——按内容身份清理）。
/// - `wrap`：包装 entry 内容（对标 Nav3 decorate）。**链式语义**：列表按顺序
///   依次包裹（首个装饰器最外层），内层是后续装饰器与 entry 内容的组合——
///   每个装饰器的 wrap 都会生效，而非只有最后一个。
pub trait NavEntryDecorator<K: NavKey>: Send + Sync + 'static {
    /// entry 从 back stack 移除时（对标 Nav3 onPop(contentKey)）
    fn on_pop(&self, _content_key: u64) {}

    /// 包装 entry 内容（对标 Nav3 decorate）——在自身作用域内调用 `inner`
    /// （inner 渲染后续装饰器包裹的 entry 内容）。默认原样渲染。
    fn wrap(&self, ctx: &mut ComposeCtx, _entry: &NavEntry<K>, inner: &dyn Fn(&mut ComposeCtx)) {
        inner(ctx);
    }
}

/// 按列表顺序链式应用装饰器的 wrap（首个最外层），末层渲染 entry 内容。
pub(super) fn wrap_entry<K: NavKey>(
    ctx: &mut ComposeCtx,
    entry: &NavEntry<K>,
    decorators: &[Box<dyn NavEntryDecorator<K>>],
) {
    fn rec<K: NavKey>(
        ctx: &mut ComposeCtx,
        entry: &NavEntry<K>,
        decorators: &[Box<dyn NavEntryDecorator<K>>],
        i: usize,
    ) {
        match decorators.get(i) {
            None => entry.build(ctx),
            Some(d) => {
                let inner = |ctx: &mut ComposeCtx| rec(ctx, entry, decorators, i + 1);
                d.wrap(ctx, entry, &inner);
            }
        }
    }
    rec(ctx, entry, decorators, 0);
}

/// 状态保持装饰器——用**固定组合 key** 包裹 entry 内容。
///
/// 对标 Nav3 `SaveableStateHolderNavEntryDecorator`（SaveableStateProvider(contentKey)）：
/// 同一 contentKey 的 entry 每次渲染都在相同组合 key 位置——组合 key 与
/// `remember_entry_state` 状态池槽都按 contentKey 关联，跨 pop/push 保持。
/// 默认随 NavDisplay 启用（对标 Nav3 的默认
/// rememberSaveableStateHolderNavEntryDecorator）。
///
/// ⚠ 保持范围仅限 `remember_entry_state`：entry 内容里的普通 `ctx.remember`
/// 存放在组合槽中，而槽位身份按**组合位置**分配（winia 槽表无 movableContent
/// 级跨位置身份——对标 Nav3 SceneSetupNavEntryDecorator）——双层过渡渲染时
/// 两层内容共享调用序基，pop/push 边界会触发槽 truncate 重建，普通 remember
/// 不保证存活。
pub struct RememberStateDecorator;

impl<K: NavKey> NavEntryDecorator<K> for RememberStateDecorator {
    fn wrap(&self, ctx: &mut ComposeCtx, entry: &NavEntry<K>, inner: &dyn Fn(&mut ComposeCtx)) {
        ctx.key(entry.content_key(), |ctx| inner(ctx));
    }
}

/// Shared-element bridge for navigation — winia's counterpart of Nav3's
/// `sharedEntryInSceneNavEntryDecorator`.
///
/// Compose wraps every entry's content in
/// `Box(Modifier.sharedElement(rememberSharedContentState(entry.key), animatedVisibilityScope =
/// LocalNavAnimatedContentScope.current))`: the ENTRY ITSELF becomes a shared element keyed by the
/// entry, so a scene change can fly it, and its animation rides the nav's own transition clock.
/// This does the same with winia's `shared_bounds`, keyed by the entry's stable content key.
///
/// Usage: put the `NavDisplay` inside a `SharedTransitionLayout` and add this to the display's entry
/// decorators. Per-element markers inside a screen keep working — different key, same scope.
///
/// Differences from Compose, recorded rather than silent: Compose throws when the scope is missing
/// (its composition local has no default); this degrades to rendering the entry unwrapped, so a
/// display without a `SharedTransitionLayout` keeps working. Compose also hands the marker the
/// nav's `AnimatedContentScope`; winia's equivalent is the scene visibility the nav publishes per
/// transition layer (`with_nav_scene`), which is what the flight system pairs the two live ends by.
pub struct SharedEntryInSceneDecorator;

impl<K: NavKey> NavEntryDecorator<K> for SharedEntryInSceneDecorator {
    fn wrap(&self, ctx: &mut ComposeCtx, entry: &NavEntry<K>, inner: &dyn Fn(&mut ComposeCtx)) {
        let Some(scope) = crate::components::shared_transition::current_shared_scope() else {
            inner(ctx);
            return;
        };
        let key = format!("entry:{}", entry.content_key());
        crate::layout::components::Column::new()
            .modifier(
                Modifier::new().fill_max_size().shared_bounds_with_overlay_clip(
                    scope.shared_content_state(&key),
                    crate::components::animated_visibility::VisibilityTransition::fade_in(
                        crate::animation::TweenSpec::default(),
                    ),
                    crate::components::animated_visibility::VisibilityTransition::fade_out(
                        crate::animation::TweenSpec::default(),
                    ),
                    crate::transition::BoundsTransform::default(),
                    crate::transition::ResizeMode::scale_to_bounds(),
                    crate::transition::PlaceHolderSize::AnimatedSize,
                    crate::transition::PathMotion::Linear,
                    0.0,
                    true,
                    crate::transition::OverlayClip::Bounds,
                ),
            )
            .build(ctx, |ctx| inner(ctx));
    }
}

/// 生命周期装饰器——entry 是否在 back stack 中决定渲染。
///
/// 对标 Nav3 `BackStackAwareLifecycleNavEntryDecorator`（栈内 RESUMED / 栈外
/// CREATED）：winia 无 Lifecycle 系统，等价语义 = 仅渲染栈内 entry（SinglePane
/// 天然只渲染栈顶；ListDetail 渲染前两个）。此装饰器目前是占位——真正的
/// 生命周期作用域（如后台 entry 冻结）后续可扩展。
pub struct BackStackAwareDecorator;

impl<K: NavKey> NavEntryDecorator<K> for BackStackAwareDecorator {
    fn on_pop(&self, _content_key: u64) {
        // 占位：entry 移除时清理钩子（后续扩展：取消动画/释放资源；
        // 对标 Nav3 BackStackAwareLifecycle——离栈 CREATED 封顶，见路线图 P1-8）
        debug_log!("[nav] entry popped: content_key={}", _content_key);
    }
}

