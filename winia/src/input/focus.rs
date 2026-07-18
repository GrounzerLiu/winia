//! 焦点系统 — FocusManager
//!
//! 管理焦点链（Tab 键导航顺序），深度优先遍历 LayoutNode 树。

use crate::layout::node::LayoutNode;

/// 焦点管理器
#[derive(Debug)]
pub struct FocusManager {
    /// 当前拥有焦点的节点 ID 列表（从根到叶）
    /// None 表示没有焦点
    focused_path: Option<Vec<usize>>,
}

impl FocusManager {
    pub fn new() -> Self {
        FocusManager {
            focused_path: None,
        }
    }

    /// 是否有焦点
    pub fn has_focus(&self) -> bool {
        self.focused_path.is_some()
    }

    /// 清除焦点
    pub fn clear_focus(&mut self) {
        self.focused_path = None;
    }

    /// 收集所有 focusable 节点的路径列表（深度优先）
    pub fn collect_focusable(root: &LayoutNode) -> Vec<Vec<usize>> {
        let mut paths = Vec::new();
        Self::collect_recursive(root, &mut Vec::new(), &mut paths);
        paths
    }

    fn collect_recursive(node: &LayoutNode, current_path: &mut Vec<usize>, paths: &mut Vec<Vec<usize>>) {
        // 检查当前节点是否 focusable
        let is_focusable = node.modifier.elements().iter().any(|el| {
            matches!(el, crate::modifier::ModifierElement::Focusable)
        });

        if is_focusable {
            paths.push(current_path.clone());
        }

        // 递归子节点
        for (i, child) in node.children.iter().enumerate() {
            current_path.push(i);
            Self::collect_recursive(child, current_path, paths);
            current_path.pop();
        }
    }

    /// 根据路径获取节点引用
    fn node_at_path<'a>(root: &'a LayoutNode, path: &[usize]) -> Option<&'a LayoutNode> {
        let mut node = root;
        for &idx in path {
            node = node.children.get(idx)?;
        }
        Some(node)
    }

    /// 切换到下一个 focusable 节点
    /// 返回 true 表示切换成功，false 表示无可切换的节点
    pub fn focus_next(&mut self, root: &LayoutNode) -> bool {
        let paths = Self::collect_focusable(root);
        if paths.is_empty() {
            self.focused_path = None;
            return false;
        }

        let next = match &self.focused_path {
            None => 0, // 没有当前焦点，选第一个
            Some(current) => {
                // 找到当前位置，切换到下一个
                let pos = paths.iter().position(|p| p == current).unwrap_or(0);
                (pos + 1) % paths.len()
            }
        };

        self.focused_path = Some(paths[next].clone());
        true
    }

    /// 切换到上一个 focusable 节点
    pub fn focus_previous(&mut self, root: &LayoutNode) -> bool {
        let paths = Self::collect_focusable(root);
        if paths.is_empty() {
            self.focused_path = None;
            return false;
        }

        let prev = match &self.focused_path {
            None => paths.len() - 1,
            Some(current) => {
                let pos = paths.iter().position(|p| p == current).unwrap_or(0);
                if pos == 0 { paths.len() - 1 } else { pos - 1 }
            }
        };

        self.focused_path = Some(paths[prev].clone());
        true
    }

    /// 获取当前焦点节点的引用
    pub fn focused_node<'a>(&self, root: &'a LayoutNode) -> Option<&'a LayoutNode> {
        self.focused_path
            .as_ref()
            .and_then(|path| Self::node_at_path(root, path))
    }
}

impl Default for FocusManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modifier::Modifier;

    fn make_focusable() -> LayoutNode {
        LayoutNode::leaf(Modifier::new().focusable())
    }

    fn make_leaf() -> LayoutNode {
        LayoutNode::leaf(Modifier::new())
    }

    fn make_container(children: Vec<LayoutNode>) -> LayoutNode {
        let mut node = LayoutNode::leaf(Modifier::new());
        for child in children {
            node.add_child(child);
        }
        node
    }

    #[test]
    fn test_collect_focusable() {
        let root = make_container(vec![
            make_leaf(),
            make_focusable(),
            make_container(vec![
                make_focusable(),
                make_leaf(),
                make_focusable(),
            ]),
            make_focusable(),
        ]);

        let paths = FocusManager::collect_focusable(&root);
        assert_eq!(paths.len(), 4);

        // 验证路径指向正确的节点
        assert!(FocusManager::node_at_path(&root, &paths[0]).unwrap().modifier.elements().iter().any(|el| matches!(el, crate::modifier::ModifierElement::Focusable)));
    }

    #[test]
    fn test_focus_next_empty() {
        let root = make_container(vec![make_leaf(), make_leaf()]);
        let mut fm = FocusManager::new();
        assert!(!fm.focus_next(&root));
        assert!(!fm.has_focus());
    }

    #[test]
    fn test_focus_next_and_prev() {
        let root = make_container(vec![
            make_focusable(),  // path [0]
            make_focusable(),  // path [1]
            make_focusable(),  // path [2]
        ]);

        let mut fm = FocusManager::new();

        // 首次 → 第一个
        assert!(fm.focus_next(&root));
        assert_eq!(fm.focused_path, Some(vec![0]));

        // 再 next → 第二个
        assert!(fm.focus_next(&root));
        assert_eq!(fm.focused_path, Some(vec![1]));

        // previous → 第一个
        assert!(fm.focus_previous(&root));
        assert_eq!(fm.focused_path, Some(vec![0]));

        // previous → 最后一个（循环）
        assert!(fm.focus_previous(&root));
        assert_eq!(fm.focused_path, Some(vec![2]));
    }
}
