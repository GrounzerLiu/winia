use super::*;

/// 计算节点在布局树中的绝对位置（从根累加 position）。
///
/// ⚠ 与 `hit_test`/`scene_to_node_local` 同空间：累加时**减去祖先 scroll 偏移**
/// （渲染时滚动容器 `canvas.translate(-offset)` → 命中测试的 scene 坐标就是
/// 减过滚动量的"滚动画布坐标"）。此前纯累加 layout position，滚动容器内
/// 节点绝对 y 被滚动量污染 → 段落局部坐标（scene - abs）偏负 → skia 最近
/// glyph 恒为第一行——多行 TextField 点击/拖拽只能定位到第一行。
pub(crate) fn node_abs_position(nodes: &[LayoutNode], root: usize, id: u64) -> (f32, f32) {
    pub(crate) fn walk(nodes: &[LayoutNode], idx: usize, target: u64, abs_x: f32, abs_y: f32) -> Option<(f32, f32)> {
        let node = &nodes[idx];
        let nx = abs_x + node.position.x;
        let ny = abs_y + node.position.y;
        if node.id == target { return Some((nx, ny)); }
        // 命中测试同路径：子节点坐标 = 本节点坐标 - 本节点 scroll 偏移
        let (dx, dy) = crate::layout::node::scroll_offset_for_node(node);
        for &c in &node.children {
            if let Some(r) = walk(nodes, c, target, nx - dx, ny - dy) { return Some(r); }
        }
        None
    }
    walk(nodes, root, id, 0.0, 0.0).unwrap_or((0.0, 0.0))
}

/// 分发指针事件到 hit_test 路径（pre: outer→inner, bubble: inner→outer）
pub(crate) fn dispatch_ptr_event(
    nodes: &[LayoutNode],
    root: usize,
    path: &[usize],
    event: &crate::input::PointerEvent,
    scene_pos: (f32, f32),
    captured_id: Option<u64>,
) -> bool {
    // 如果指针被一个节点捕获（Down 后未释放），用 root 查找节点并构建祖先链
    let captured_path: Vec<usize> = if let Some(cid) = captured_id {
        let mut ancestors: Vec<usize> = Vec::new();
        if let Some(nid) = crate::layout::node::find_node_id_by_slot_key(nodes, root, cid).and_then(|nid| crate::layout::node::find_node_by_id(nodes, root, nid)) {
            ancestors.push(nid);
            let mut pid = nodes[nid].parent_id;
            while let Some(id) = pid {
                if let Some(anc) = crate::layout::node::find_node_by_id(nodes, root, id) {
                    ancestors.push(anc);
                    pid = nodes[anc].parent_id;
                } else { break; }
            }
            ancestors.reverse(); // root → ... → captured
            ancestors
        } else {
            path.to_vec()
        }
    } else { path.to_vec() };
    let use_path = &captured_path;
    // 计算路径累积偏移（每个节点的 position 是相对于父节点的偏移）。
    // 与 hit_test_recursive 一致：节点 i 的视觉坐标 = Σposition[0..=i]
    // − Σscroll[0..i-1]（祖先的 scroll offset；节点自身 offset 只影响子节点，
    // 不影响自身左上角——见 node.rs hit_test_recursive 的 child_px/nx 逻辑）。
    let mut abs_x = 0.0f32;
    let mut abs_y = 0.0f32;
    let mut scroll_x = 0.0f32;
    let mut scroll_y = 0.0f32;
    let abs_positions: Vec<(f32, f32)> = use_path.iter().map(|&i| {
        abs_x += nodes[i].position.x;
        abs_y += nodes[i].position.y;
        // 当前节点坐标 = 累积位置 − 祖先 scroll 累积（自身 offset 稍后累加）
        let pos = (abs_x - scroll_x, abs_y - scroll_y);
        let (sdx, sdy) = crate::layout::node::scroll_offset_for_node(&nodes[i]);
        scroll_x += sdx;
        scroll_y += sdy;
        pos
    }).collect();

    // on_pre_ptr: outer → inner
    // 双轨（exp/modifier-node）：枚举 on_pre_ptr 先行，同节点 node.on_pre 紧随。
    for (i, &ni) in use_path.iter().enumerate() {
        let local_x = scene_pos.0 - abs_positions[i].0;
        let local_y = scene_pos.1 - abs_positions[i].1;
        let mut ev = event.clone();
        ev.position = (local_x, local_y);
        for el in nodes[ni].modifier.elements() {
            if let crate::modifier::ModifierElement::PointerEvent { on_pre_ptr: Some(handler), .. } = el {
                if handler(&ev) { return true; }
            }
        }
        for p in nodes[ni].modifier.pointer_nodes() {
            if p.on_pre(&ev) { return true; }
        }
    }

    // on_ptr: inner → outer
    for (i, &ni) in use_path.iter().enumerate().rev() {
        let local_x = scene_pos.0 - abs_positions[i].0;
        let local_y = scene_pos.1 - abs_positions[i].1;
        let mut ev = event.clone();
        ev.position = (local_x, local_y);
        for el in nodes[ni].modifier.elements() {
            if let crate::modifier::ModifierElement::PointerEvent { on_ptr: Some(handler), .. } = el {
                if handler(&ev) { return true; }
            }
        }
        for p in nodes[ni].modifier.pointer_nodes() {
            if p.on_event(&ev) { return true; }
        }
    }
    false
}

/// request 节流判断（纯函数——边界单测）：
/// 距上次 request >= 帧间隔才允许请求（WM_PAINT 生成频率受控为刷新率）。
/// 独立于渲染节流（last_render_time）——避免 WM_PAINT 晚于 request（ε>0）导致
/// 定时器唤醒时 now-last_render = I-ε < I 恒拦截 → 渲染频率减半（2I 间隔）。
#[allow(dead_code)] // the tests in this file call it
pub(crate) fn should_request_redraw(last_request: std::time::Instant, now: std::time::Instant, interval: std::time::Duration) -> bool {
    now.duration_since(last_request) >= interval
}
