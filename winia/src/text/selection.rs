//! The selection model — what a `SelectionContainer` registers and what a text node reports into.
//!
//! It is held by `LayoutNode` (a text node keeps the registrar of the container above it) and read
//! by the renderer, so it cannot live in the component that provides it. `SelectionContainer` stays
//! where it is and provides the local declared here.

use crate::runtime::composition_local::CompositionLocal;
use parking_lot::Mutex;
use std::ops::Range;
use std::collections::HashMap;
use std::sync::Arc;

// ═══════════════════════════════════════════════════════════
// 辅助类型
// ═══════════════════════════════════════════════════════════

/// 注册的文本段信息
#[derive(Debug, Clone)]
pub(crate) struct RegisteredSegment {
    pub slot_key: u64,
    pub global_offset: usize,
    pub text_len: usize,
    /// 段实际文本（注册时保存——selected_text 拼接用，用户无需自维护平行字符串）
    pub text: Arc<str>,
}

// ═══════════════════════════════════════════════════════════
// SelectionRegistrar
// ═══════════════════════════════════════════════════════════

type OnChangeFn = Arc<dyn Fn(&Selection) + Send + Sync>;

/// 选择结果——回调参数（对标 Compose `Selection`：自带选中文本）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    start: usize,
    end: usize,
    text: String,
}

impl Selection {
    /// 容器全局起始偏移（含容器内所有已注册段）
    pub fn start(&self) -> usize { self.start }
    /// 容器全局结束偏移（含容器内所有已注册段）
    pub fn end(&self) -> usize { self.end }
    /// 选中的文本（框架按注册段自动拼接——含跨段/emoji 边界安全）
    pub fn text(&self) -> &str { &self.text }
}

#[derive(Clone)]
struct RegistrarInner {
    selection_start: Option<usize>,
    selection_end: Option<usize>,
    next_global_offset: usize,
    segments: HashMap<u64, RegisteredSegment>,
    on_change: Option<OnChangeFn>,
}

impl std::fmt::Debug for RegistrarInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RegistrarInner").field("selection_start", &self.selection_start).field("selection_end", &self.selection_end).field("has_cb", &self.on_change.is_some()).finish()
    }
}

#[derive(Debug, Clone)]
pub struct SelectionRegistrar {
    inner: Arc<Mutex<RegistrarInner>>,
}

impl SelectionRegistrar {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(RegistrarInner {
                selection_start: None,
                selection_end: None,
                next_global_offset: 0,
                segments: HashMap::new(),
                on_change: None,
            })),
        }
    }

    pub fn register(&self, slot_key: u64, text: &str) -> usize {
        let mut inner = self.inner.lock();
        let text_len = text.len();
        let offset = if let Some(existing) = inner.segments.get(&slot_key) {
            existing.global_offset
        } else {
            let off = inner.next_global_offset;
            inner.next_global_offset += text_len;
            off
        };
        // 保持拼接不变量：段区间互不重叠——dedup 覆盖更长文本时推进全局偏移，
        // 否则后续段起点落在本段内部（build_selection 拼接重复/丢段）
        inner.next_global_offset = inner.next_global_offset.max(offset + text_len);
        inner.segments.insert(slot_key, RegisteredSegment {
            slot_key, global_offset: offset, text_len,
            text: text.into(),
        });
        offset
    }

    pub fn set_selection(&self, start: usize, end: usize) {
        let mut inner = self.inner.lock();
        inner.selection_start = Some(start.min(end));
        inner.selection_end = Some(start.max(end));
    }

    pub fn clear_selection(&self) {
        let mut inner = self.inner.lock();
        inner.selection_start = None;
        inner.selection_end = None;
    }

    /// 是否同一实例（Arc 身份——跨容器拖动的 anchor 归属判断：拖到别的
    /// SelectionContainer 的文本上时，用 anchor 容器做 edge snap，不切偏移空间）
    pub fn is_same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }

    pub fn segment_info(&self, slot_key: u64) -> Option<(usize, usize)> {
        let inner = self.inner.lock();
        inner.segments.get(&slot_key).map(|s| (s.global_offset, s.text_len))
    }

    pub(crate) fn reset_offsets(&self) {
        let mut inner = self.inner.lock();
        inner.next_global_offset = 0;
        inner.segments.clear();
    }

    pub fn total_text_len(&self) -> usize {
        self.inner.lock().next_global_offset
    }

    pub fn selected_range(&self, slot_key: u64) -> Option<Range<usize>> {
        let inner = self.inner.lock();
        let (global_start, global_end) = (inner.selection_start?, inner.selection_end?);
        let seg = inner.segments.get(&slot_key)?;
        let local_start = global_start.saturating_sub(seg.global_offset);
        let local_end = global_end.saturating_sub(seg.global_offset);
        if local_start >= seg.text_len || local_end == 0 { return None; }
        let len = seg.text_len;
        Some(local_start.min(len)..local_end.min(len))
    }

    /// 设置选区变化回调（参数为 Selection——含选中文本，用户无需自维护平行字符串）
    pub fn set_on_change(&self, f: impl Fn(&Selection) + Send + Sync + 'static) {
        self.inner.lock().on_change = Some(Arc::new(f));
    }

    /// 构造当前选择的 Selection（按注册段自动拼接文本——跨段/emoji 边界安全）
    pub(crate) fn build_selection(&self) -> Option<Selection> {
        let inner = self.inner.lock();
        let total = inner.next_global_offset;
        let (s, e) = (inner.selection_start?, inner.selection_end?);
        let (s, e) = (s.min(e), s.max(e));
        // 零宽（点击未拖动）→ 无选择（与 selected_range 一致）
        if s == e { return None; }
        // 越界 clamp 到容器总长度（防御：异常偏移不暴露空文本+越界值）
        let (s, e) = (s.min(total), e.min(total));
        if s == e { return None; }
        let mut segs: Vec<&RegisteredSegment> = inner.segments.values().collect();
        segs.sort_by_key(|seg| seg.global_offset);
        let mut text = String::new();
        for seg in segs {
            let seg_s = seg.global_offset;
            let seg_e = seg_s + seg.text_len;
            if seg_e <= s || seg_s >= e { continue; }
            let start = s.max(seg_s) - seg_s;
            let end = e.min(seg_e) - seg_s;
            // 防御：偏移理论上是字符边界（get_closest 保证），但跨段拼接时
            // 用 get 避免任何越界/非边界 panic（异常时跳过该段）
            if let Some(part) = seg.text.get(start..end) {
                text.push_str(part);
            }
        }
        // RichText 内联元素（image/placeholder）以 U+FFFC 占位——选中文本对
        // 用户无意义（不可复制），剔除
        if text.contains('\u{FFFC}') {
            text = text.replace('\u{FFFC}', "");
        }
        Some(Selection { start: s, end: e, text })
    }

    pub(crate) fn fire_on_change(&self) {
        let cb = self.inner.lock().on_change.clone();
        if let Some(cb) = cb {
            if let Some(sel) = self.build_selection() {
                cb(&sel);
            }
        }
    }
}

impl Default for SelectionRegistrar {
    fn default() -> Self { Self::new() }
}

// ═══════════════════════════════════════════════════════════
// LOCAL_SELECTION_REGISTRAR
// ═══════════════════════════════════════════════════════════

pub static LOCAL_SELECTION_REGISTRAR: std::sync::LazyLock<CompositionLocal<SelectionRegistrar>> =
    std::sync::LazyLock::new(|| CompositionLocal::new(|| SelectionRegistrar::new()));
