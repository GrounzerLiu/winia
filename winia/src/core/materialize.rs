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
    // 同帧多次 compose：第一次已物化并 drain 了 prev_node_by_key——
    // 第二次 materialize 若重建，Skip 恢复全部失败（prev 空）→ 树塌缩。
    // 保留现有树（组合产物差异只影响值/结构微调——布局读最新 State 值，
    // 结构变化下一帧（prev 已重建）自然收敛）。注意：必须在清 root 前判断。
    if composer.prev_node_by_key.is_empty() && composer.arena.root.is_some() {
        return;
    }
    composer.arena.root = None;
    for desc in descs {
        materialize_node(composer, desc, None);
    }
}

/// 物化单个 desc 节点（递归子节点）——Skip 恢复 / 节点复用 / 降级重建。
pub(crate) fn materialize_node(composer: &mut Composer, desc: DescNode, parent: Option<usize>) -> Option<usize> {
    let DescNode { key, skip, modifier, preserve_modifier, policy, on_remove, dirty, registrar, children } = desc;
    let index = if skip {
        // Skip：恢复上帧节点（key 匹配——保留测量/内容；children 清空后
        // 按 slot 树结构重新挂接（子节点逐个从 prev_node_by_key 恢复——
        // 不残留不 free）。无缓存为异常——防御跳过
        match composer.prev_node_by_key.remove(&key) {
            Some(idx) => {
                composer.reused_nodes.insert(idx);
                let n = &mut composer.arena.nodes[idx];
                n.children.clear();
                // 应用本帧组合产物 modifier（容器自身 Skip——外层构造的 modifier
                // 参数可能变化（offset/background 等视觉属性）——不更新则视觉卡旧值；
                // 后代（preserve_modifier）保留缓存节点 modifier——不清空视觉）
                if !preserve_modifier {
                    n.modifier = modifier;
                }
                n.dirty = false; // 恢复缓存——测量折叠（保留测量）
                Some(idx)
            }
            None => {
                // 防御降级：Skip 恢复失败（key 不匹配/prev 缺失）→ 按 Enter 重建
                // （dirty=true 重测）。否则节点缺失 → 子树塌缩（间歇性坐标错乱）。
                // 子树完整优先于测量折叠——下一帧 key 稳定后恢复 Skip。
                // 注意：不能 return（会跳过尾部 add_child/children 挂接）——
                // 返回 Some(idx) 走统一挂接路径。
                let pidx = policy.map(|p| composer.arena.alloc_policy(p));
                let mut node = crate::layout::node::LayoutNode::new(modifier, pidx);
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
                Some(idx)
            }
        }
    } else {
        // 复用节点：policy 替换旧槽（本帧参数生效 + 池不增长——否则每帧 alloc 泄漏）
        let reused_idx = composer.prev_node_by_key.remove(&key);
        let pidx = if reused_idx.is_some() {
            if let Some(p) = policy {
                let old = composer.arena.nodes[reused_idx.unwrap()].measure_policy;
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
            n.modifier = modifier;
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

/// 收集 arena 树中所有节点的 slot_key → 索引映射（阶段D 节点复用用）
pub(crate) fn collect_node_keys(
    arena: &NodeArena,
    idx: usize,
    map: &mut std::collections::HashMap<u64, usize>,
) {
    if let Some(prev) = map.insert(arena.nodes[idx].slot_key, idx) {
        #[cfg(debug_assertions)] {
            if std::env::var("WINIA_KEY_TRACE").is_ok() {
                eprintln!("[dup-key] sk={} idx={} 被 {} 覆盖", arena.nodes[idx].slot_key >> 32, prev, idx);
            }
        }
    }
    let children = arena.nodes[idx].children.clone();
    for c in children {
        collect_node_keys(arena, c, map);
    }
}
