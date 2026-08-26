//! 物化器：组合树（Slot desc）→ 布局树（arena LayoutNode）。
//!
//! 从 composer.rs 拆分（SRP）——物化是"组合产物 → 布局树"的独立关注点：
//! - `collect_desc_tree`（SlotTable）产出 `DescNode` 树（保留在 composer.rs——需访问 slot 私有字段）
//! - 本模块消费 DescNode → arena 树（Skip 恢复 / 节点复用 / 降级重建）
//! - `collect_nodes` / `collect_node_keys`：物化后的 arena 收集（缓存/复用索引）

use crate::core::composer::Composer;
use crate::layout::node::NodeArena;

/// 组合产物描述树节点（物化输入）。
pub(crate) struct DescNode {
    pub(crate) key: u64,
    /// Skip 节点（组合期 content 未执行——desc 空但非 scope）：
    /// 物化时从 prev_node_by_key 按 key 恢复缓存节点（不新建）
    pub(crate) skip: bool,
    pub(crate) modifier: crate::modifier::Modifier,
    /// Skip 子树内：本帧 build 是否被调用（容器自身调了 set_skip_modifier——
    /// modifier 是父层重跑传入的新值，应应用；后代未执行——modifier 为 default，
    /// 应保留缓存节点的 modifier，避免视觉被清空）
    pub(crate) preserve_modifier: bool,
    pub(crate) policy: Option<Box<dyn crate::layout::MeasurePolicy>>,
    pub(crate) on_remove: Option<Box<dyn FnOnce() + Send>>,
    pub(crate) dirty: bool,
    /// 文本选择 registrar（物化时写入节点——组合期与物化期分离的传递通道）
    pub(crate) registrar: Option<crate::ui::selection_container::SelectionRegistrar>,
    /// 焦点环颜色（物化时写入节点——组合期捕获，渲染期读取）
    pub(crate) focus_color: Option<crate::modifier::Color>,
    /// 光标（TextField）——组合期写入 desc，物化时应用到节点
    pub(crate) cursor_index: Option<usize>,
    pub(crate) cursor_visible: Option<bool>,
    pub(crate) cursor_callback: Option<Box<dyn Fn(usize) + Send>>,
    /// 显示聚焦标记（text-field-v2 容器化——渲染光标/选区用，回退 node.focused）
    pub(crate) display_focused: Option<bool>,
    pub(crate) ime_callback: Option<Box<dyn Fn(&str, Option<(usize, usize)>) + Send>>,
    /// 外层 Option：None = 非 TextField 未设置；Some(r) = 渲染值（r 可为 None 清空）
    pub(crate) composing_range: Option<Option<std::ops::Range<usize>>>,
    pub(crate) selection_range: Option<Option<std::ops::Range<usize>>>,
    /// 布局方向（组合期捕获——物化直接用，不读 CompositionLocal）
    pub(crate) direction: crate::layout::LayoutDirection,
    pub(crate) children: Vec<DescNode>,
}

/// 物化：组合树（Slot desc）→ 布局树（arena LayoutNode）——完整分离的核心。
/// 由 compose 末尾调用（layout 只测量）。
/// descs 为空时：同帧二次 compose（prev 已被首次物化 drain）保留现有树；
/// 内容确实消失（prev 非空——正常 compose 无产物）清空树（旧行为——避免旧树持续渲染）。
pub(crate) fn materialize(composer: &mut Composer) {
    let mut descs = Vec::new();
    composer.slot_table.collect_desc_tree(&mut descs);
    if descs.is_empty() {
        if !(composer.prev_node_by_key.is_empty() && composer.arena.root.is_some()) {
            composer.arena.root = None;
        }
        return; // 无组合产物（layout 防御调用——树保留；compose 末尾已物化）
    }
    // 同帧多次 compose：第一次已物化并 drain 了 prev_node_by_key——第二次
    // materialize 若直接重建，Skip 恢复全部失败（prev 空）→ 走防御降级（按
    // Enter 重建）——但**降级重建的 Skip 容器会丢失子内容**（desc=None →
    // 空节点 → collect 缓存空 → 按钮等永久消失）。8548718 的守卫（prev 空
    // 直接保留旧树）则错误阻断二次 compose 的真实 Enter 内容（AnimatedContent
    // 切换帧 B 内容被丢弃 → 树永远停留旧内容）。
    // 正解：用**现有树**重建 prev 索引（collect_node_keys）——Skip 节点复用
    // 现有节点（子内容保留），Enter 节点走正常复用+更新路径——内容正确且
    // 不丢失子树（一帧全树重挂的代价仅发生在同帧二次 compose——频率低）。
    if composer.prev_node_by_key.is_empty() && composer.arena.root.is_some() {
        crate::core::materialize::collect_node_keys(&composer.arena, composer.arena.root.unwrap(), &mut composer.prev_node_by_key);
    }
    composer.arena.root = None;
    for desc in descs {
        materialize_node(composer, desc, None);
    }
}

/// 清除 LayoutNode 上残留的 TextField 专用字段（内容类型切换时调用）。
/// 当节点从 TextField 输入叶子切为普通 Text/Image/RichText 时，旧 cursor/IME/
/// selection/registrar 值会污染新节点的渲染路径。
fn clear_textfield_state(n: &mut crate::layout::node::LayoutNode) {
    *n.cursor_callback.borrow_mut() = None;
    *n.ime_callback.borrow_mut() = None;
    *n.composing_range.borrow_mut() = None;
    *n.selection_range.borrow_mut() = None;
    *n.registrar.borrow_mut() = None;
    n.cursor_x.set(0.0);
    n.cursor_height.set(0.0);
    n.cursor_index.set(0);
    n.cursor_visible.set(false);
    n.display_focused.set(false);
    n.focus_color.set(crate::modifier::Color::TRANSPARENT);
    n.focused = false;
}

/// 物化单个 desc 节点（递归子节点）——Skip 恢复 / 节点复用 / 降级重建。
pub(crate) fn materialize_node(composer: &mut Composer, desc: DescNode, parent: Option<usize>) -> Option<usize> {
    let DescNode { key, skip, modifier, preserve_modifier, policy, on_remove, dirty, registrar, focus_color, cursor_index, cursor_visible, cursor_callback, display_focused, ime_callback, composing_range, selection_range, direction, children } = desc;
    let index = if skip {
        // Skip：恢复上帧节点（key 匹配——保留测量/内容；children 清空后
        // 按 slot 树结构重新挂接（子节点逐个从 prev_node_by_key 恢复——
        // 不残留不 free）。无缓存为异常——防御跳过。
        // 结构签名（P3-1）：本帧 desc 直接子数 vs 缓存节点直接子数——子树结构
        // 增删（if 分支/列表项）后同位置 slot_key 仍相同，签名不等则放弃恢复
        // （走 None 降级 → Enter 重建），防旧内容缓存张冠李戴（塌缩类 bug 根因）。
        // 注意：签名不等时**不 remove**——key 留待 compose 末尾回收（free），
        // 否则旧节点成为 arena 孤儿（泄漏）。
        match composer.prev_node_by_key.get(&key) {
            Some(&idx) if children.len() == composer.arena.nodes[idx].children.len() => {
                let idx = composer.prev_node_by_key.remove(&key).unwrap();
                composer.reused_nodes.insert(idx);
                let n = &mut composer.arena.nodes[idx];
                n.children.clear();
                // 应用本帧组合产物 modifier（容器自身 Skip——外层构造的 modifier
                // 参数可能变化（offset/background 等视觉属性）——不更新则视觉卡旧值；
                // 后代（preserve_modifier）保留缓存节点 modifier——不清空视觉）
                if !preserve_modifier {
                    n.modifier = modifier;
                    // 刷新方向快照（组合期捕获值——复用节点必须与新建路径一致）
                    n.layout_direction = desc.direction;
                }
                #[cfg(debug_assertions)]
                if std::env::var("WINIA_MAT_PROBE").is_ok() {
                    let sz = n.measured_size;
                    eprintln!(
                        "[mat] skip key={:x} preserve={} size=({:.0},{:.0}) text={:?}",
                        key, preserve_modifier, sz.width, sz.height,
                        n.modifier.elements().iter().find_map(|el| match el {
                            crate::modifier::ModifierElement::TextContent { content, .. } => Some(content.clone()),
                            _ => None,
                        })
                    );
                }
                // 恢复缓存——测量折叠（保留测量）。⚠ 不能无条件清 dirty：
                // 同帧二次 compose 时（动画/交互状态 pending 触发），父容器
                // Skip 恢复会覆盖第一次物化刚设置的 dirty=true（文本内容已变需
                // 重测）→ cached_paragraph 保留旧内容 → 渲染画旧文本，直到外部
                // 事件触发重组。正常 Skip 的节点来自上帧 layout（dirty 恒 false），
                // 保留现状即可；同帧二次物化则保留第一次设置的 dirty。
                Some(idx)
            }
            _ => {
                // 防御降级：Skip 恢复失败（无缓存/结构签名不等）→ 按 Enter 重建
                // （dirty=true 重测）。否则节点缺失 → 子树塌缩（间歇性坐标错乱）。
                // 子树完整优先于测量折叠——下一帧 key 稳定后恢复 Skip。
                // 注意：不能 return（会跳过尾部 add_child/children 挂接）——
                // 返回 Some(idx) 走统一挂接路径。
                let pidx = policy.map(|p| composer.arena.alloc_policy(p));
                let mut node = crate::layout::node::LayoutNode::new(modifier, pidx);
                // 方向用组合期捕获值（desc.direction）——物化期读不到 CompositionLocal
                node.layout_direction = desc.direction;
                node.on_remove = on_remove;
                node.slot_key = key;
                // 降级节点：Skip 的 desc 通常已带 policy（skip_policy 保存外层传入值），
                // 此处为最终兜底——从 prev_nodes 恢复缓存测量折叠
                // （policy 仍缺失时避免测量出 0 尺寸）
                if let Some(cached) = composer.prev_nodes.get(&key) {
                    node.measured_size = cached.measured_size;
                    node.cached_constraints = cached.cached_constraints;
                    node.dirty = false;
                } else {
                    node.dirty = true;
                }
                // 文本内容差异检测（与 Enter 路径一致）：dirty=false 折叠测量时
                // 若 TextContent 变化（输入/选择）→ 强制重测，避免缓存 paragraph 旧内容
                if !node.dirty {
                    if let Some(cached) = composer.prev_nodes.get(&key) {
                        if crate::layout::node::modifier_text_content_differs(&cached.modifier, &node.modifier) {
                            node.dirty = true;
                        }
                    }
                }
                let idx = composer.arena.alloc(node);
                #[cfg(debug_assertions)]
                if std::env::var("WINIA_MAT_PROBE").is_ok() {
                    eprintln!(
                        "[mat-fb] key={:x} text={:?}",
                        key,
                        composer.arena.nodes[idx].modifier.elements().iter().find_map(|el| match el {
                            crate::modifier::ModifierElement::TextContent { content, .. } => Some(content.clone()),
                            _ => None,
                        })
                    );
                }
                Some(idx)
            }
        }
    } else {
        // 复用节点：policy 替换旧槽（本帧参数生效 + 池不增长——否则每帧 alloc 泄漏）
        let reused_idx = composer.prev_node_by_key.remove(&key);
        let pidx = if let Some(ridx) = reused_idx {
            if let Some(p) = policy {
                let old = composer.arena.nodes[ridx].measure_policy;
                if let Some(op) = old {
                    composer.arena.policies[op] = p;
                    Some(op)
                } else {
                    Some(composer.arena.alloc_policy(p))
                }
            } else { None }
        } else {
            policy.map(|p| composer.arena.alloc_policy(p))
        };
        let idx = if let Some(idx) = reused_idx {
            composer.reused_nodes.insert(idx);
            let n = &mut composer.arena.nodes[idx];
            n.children.clear();
            // 按新 modifier 重新判定内容类型（复用路径不重建节点——必须同步
            // content-kind 标记，否则 measure_node 按旧标记走错路径：
            // 文本→普通叶子（宽度塌缩）或普通→文本（误走文本排版））
            let old_has_text = n.has_text_content;
            let old_has_richtext = n.has_richtext_content;
            let old_has_image = n.has_image_content;
            n.has_text_content = crate::layout::node::modifier_has_text(&modifier);
            n.has_richtext_content = crate::layout::node::modifier_has_richtext(&modifier);
            n.has_image_content = crate::layout::node::modifier_has_image(&modifier);
            // 仅当内容类型变化时清空缓存段落（静态文本跨帧复用保留缓存——
            // 每帧清空会导致渲染时每帧重建 Skia Paragraph，影响性能）
            if n.has_text_content != old_has_text
                || n.has_richtext_content != old_has_richtext
                || n.has_image_content != old_has_image
            {
                *n.cached_paragraph.borrow_mut() = None;
                // 内容类型切换时重置 TextField 专用字段（旧语义残留不适用新类型；
                // 新类型若需要这些字段，由 desc 条件覆盖写回正确值）
                clear_textfield_state(n);
            }
            n.modifier = modifier;
            // 刷新方向快照（复用节点与新建路径一致——组合期捕获值）
            n.layout_direction = desc.direction;
            n.measure_policy = pidx; // 显式赋值（None 清空——防类型切换残留旧 policy）
            n.on_remove = on_remove;
            n.slot_key = key;
            n.dirty = dirty; // Dirty → 重测；Clean → 折叠（保留测量）
            // 文本内容变化检测：依赖注册在父容器 → leaf Slot Clean 但 TextContent 变了
            // （输入/选择）——不重测则 cached_paragraph 旧内容（输入不显示）
            if !dirty {
                if let Some(cached) = composer.prev_nodes.get(&key) {
                    if crate::layout::node::modifier_text_content_differs(&cached.modifier, &n.modifier) {
                        n.dirty = true;
                    }
                }
            }
            idx
        } else {
            let mut node = crate::layout::node::LayoutNode::new(modifier, pidx);
            // 方向用组合期捕获值（desc.direction）——物化期读不到 CompositionLocal
            node.layout_direction = desc.direction;
            node.on_remove = on_remove;
            node.slot_key = key;
            if !dirty {
                // Clean slot：从上一帧缓存恢复布局部分（measured_size/cached_constraints）——
                // modifier 用本帧 build 的值（恢复旧 modifier 会覆盖本帧新值，如按钮 label 切换）
                if let Some(cached) = composer.prev_nodes.get(&key) {
                    node.restore_layout(cached);
                }
            }
            composer.arena.alloc(node)
        };
        Some(idx)
    };
    let Some(index) = index else {
        // Skip 恢复失败已在上方降级为 Enter（重建节点）——此处仅 Enter 恒 Some
        // 兜底（children 已由降级/Enter 路径递归处理）
        return None;
    };
    // 应用文本选择 registrar（组合期写入 desc——物化时落到节点；
    // Skip 恢复路径的节点保留缓存 registrar，不走此处）
    if let Some(reg) = registrar {
        *composer.arena.nodes[index].registrar.borrow_mut() = Some(reg);
    }
    // 应用焦点环颜色（组合期捕获——Enter 路径写入；Skip 恢复保留缓存值）
    if let Some(color) = focus_color {
        composer.arena.nodes[index].focus_color.set(color);
    }
    // 应用光标/IME/选区（组合期写入 desc——Enter 路径；Skip 恢复保留缓存值）
    // 先记录 desc 是否提供这些字段（条件式会 move 值，需提前缓存）
    let desc_has_ime = ime_callback.is_some();
    let desc_has_cursor = cursor_callback.is_some();
    if let Some(ci) = cursor_index {
        composer.arena.nodes[index].cursor_index.set(ci);
    }
    if let Some(cv) = cursor_visible {
        composer.arena.nodes[index].cursor_visible.set(cv);
    }
    if let Some(cb) = cursor_callback {
        *composer.arena.nodes[index].cursor_callback.borrow_mut() = Some(cb);
    }
    if let Some(df) = display_focused {
        composer.arena.nodes[index].display_focused.set(df);
    }
    if let Some(icb) = ime_callback {
        *composer.arena.nodes[index].ime_callback.borrow_mut() = Some(icb);
    }
    // 组合/选区范围：Some(r) 即应用（r=None 清空；None = 非 TextField 不碰）
    if let Some(r) = composing_range {
        *composer.arena.nodes[index].composing_range.borrow_mut() = r;
    }
    if let Some(r) = selection_range {
        *composer.arena.nodes[index].selection_range.borrow_mut() = r;
    }
    // Enter 路径且 desc 不提供 IME/cursor 回调时，若旧节点有残留 TextField 状态，
    // 清理之（语义角色切换：TextField 输入叶子→普通 Text）。Skip 路径保留旧值。
    if !skip && !desc_has_ime && !desc_has_cursor {
        let n = &mut composer.arena.nodes[index];
        if n.ime_callback.borrow().is_some() || n.cursor_callback.borrow().is_some() {
            clear_textfield_state(n);
        }
    }
    if let Some(p) = parent {
        composer.arena.add_child(p, index);
    } else {
        composer.arena.root = Some(index);
    }
    for child in children {
        materialize_node(composer, child, Some(index));
    }
    Some(index)
}

/// 收集 arena 树 → slot_key 索引的缓存（后序：dirty 子→父冒泡）
pub(crate) fn collect_nodes(
    arena: &mut NodeArena,
    idx: usize,
    map: &mut std::collections::HashMap<u64, crate::layout::node::CachedNode>,
) {
    // 先递归子节点（后序），以便 dirty 从子向父冒泡
    let children = arena.nodes[idx].children.clone();
    for c in children {
        collect_nodes(arena, c, map);
        if arena.nodes[c].dirty {
            arena.nodes[idx].dirty = true;
        }
    }
    // 缓存当前节点的可缓存子集
    map.insert(arena.nodes[idx].slot_key, arena.nodes[idx].to_cached());
}

/// 收集 arena 树中所有节点的 slot_key → 索引映射（阶段D 节点复用用）。
/// 同 slot_key 两节点 = key 冲突（fail-fast panic——不静默覆盖：
/// 覆盖意味着前一节点状态丢失 + 节点身份错位——dup-key 是组合 bug
/// 的最终防线，调试信息含 key/节点索引/被覆盖位置）。
pub(crate) fn collect_node_keys(
    arena: &NodeArena,
    idx: usize,
    map: &mut std::collections::HashMap<u64, usize>,
) {
    collect_node_keys_with_parent(arena, idx, None, map, 0);
}

fn collect_node_keys_with_parent(
    arena: &NodeArena,
    idx: usize,
    parent: Option<usize>,
    map: &mut std::collections::HashMap<u64, usize>,
    depth: usize,
) {
    if let Some(prev) = map.insert(arena.nodes[idx].slot_key, idx) {
        let (a, b) = (&arena.nodes[idx], &arena.nodes[prev]);
        let parent_desc = parent
            .map(|p| format!("父 idx={} sk={:#x}", p, arena.nodes[p].slot_key))
            .unwrap_or_else(|| "根".to_string());
        panic!(
            "[dup-key] slot_key 冲突：sk={:#x} 节点 idx={} pos={:?} size={:?} {}（depth={}）\
             覆盖了已有节点 idx={} pos={:?} size={:?}\
             ——同一组合位置出现两个节点（key 漂移/结构变化漏配 ctx.key？）。\
             修复：①结构变化处加 ctx.key() ②检查列表实例隔离 ③组件调用点在 \
             #[composable] 内",
            arena.nodes[idx].slot_key, idx, a.position, a.measured_size, parent_desc, depth,
            prev, b.position, b.measured_size
        );
    }
    let children = arena.nodes[idx].children.clone();
    for c in children {
        collect_node_keys_with_parent(arena, c, Some(idx), map, depth + 1);
    }
}
