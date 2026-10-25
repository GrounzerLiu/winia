//! What the last measure pass saw — Compose's `LazyListLayoutInfo` and `LazyListItemInfo`.
//!
//! A caller reads this through [`crate::layout::LazyListState::layout_info`]: the visible items with their
//! keys, offsets and sizes, plus the viewport bounds. Custom snapping, accessibility, a debug panel
//! and "is the first item fully visible" all start here, and none of them can be answered from the
//! anchor alone.
//!
//! Offsets follow Compose: an item's `offset` is its main-axis start RELATIVE TO THE LIST
//! CONTAINER, so the first item is negative once the list is scrolled past it, and
//! `viewport_start_offset` is `-before_content_padding` (negative when the content padding is
//! visible, which is what makes "fully visible" computable: `offset >= viewport_start`).

/// One visible item — Compose's `LazyListItemInfo`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LazyListItemInfo {
    /// The item's global index in the data.
    pub index: usize,
    /// The key it was registered with (an index-derived key for a section without a key factory).
    pub key: u64,
    /// Main-axis start relative to the list container. Negative when the item starts above/left of
    /// the viewport, which is the normal state of a partially visible first item.
    pub offset: f32,
    /// Main-axis size.
    pub size: f32,
}

/// The layout information of the last measure pass — Compose's `LazyListLayoutInfo`.
///
/// Only the items that intersect the viewport are listed: the composition window also covers
/// `beyond_bounds` items on each side, and those are not visible.
#[derive(Debug, Clone, PartialEq)]
pub struct LazyListLayoutInfo {
    /// The items intersecting the viewport, in data order.
    pub visible_items: Vec<LazyListItemInfo>,
    /// `-before_content_padding` (Compose): the smallest offset that is still visible.
    pub viewport_start_offset: f32,
    /// The viewport's main size plus `after_content_padding`: the largest visible offset edge.
    pub viewport_end_offset: f32,
    /// The total count of items in the data.
    pub total_items: usize,
    /// The viewport's size in both axes — the lazy node's own box, content padding not included.
    pub viewport_size: (f32, f32),
    /// Whether the main axis is vertical (`LazyColumn`) rather than horizontal (`LazyRow`).
    pub vertical: bool,
    /// The `reverse_layout` this list was built with.
    pub reverse_layout: bool,
    /// The resolved layout direction: an RTL `LazyRow` mirrors its main axis.
    pub rtl: bool,
    /// Content padding before the first item along the main axis.
    pub before_content_padding: f32,
    /// Content padding after the last item along the main axis.
    pub after_content_padding: f32,
    /// Spacing between consecutive items.
    pub spacing: f32,
}

impl Default for LazyListLayoutInfo {
    fn default() -> Self {
        Self {
            visible_items: Vec::new(),
            viewport_start_offset: 0.0,
            viewport_end_offset: 0.0,
            total_items: 0,
            viewport_size: (0.0, 0.0),
            vertical: true,
            reverse_layout: false,
            rtl: false,
            before_content_padding: 0.0,
            after_content_padding: 0.0,
            spacing: 0.0,
        }
    }
}

impl LazyListLayoutInfo {
    /// Whether the main axis ended up mirrored: `reverse_layout` flips it, and so does RTL on a
    /// horizontal list.
    pub fn mirrored(&self) -> bool {
        self.reverse_layout ^ (!self.vertical && self.rtl)
    }

    /// Whether an item at `offset` with `size` is entirely inside the viewport — Compose's
    /// `offset >= viewportStartOffset && offset + size <= viewportEndOffset`.
    pub fn is_fully_visible(&self, offset: f32, size: f32) -> bool {
        offset >= self.viewport_start_offset && offset + size <= self.viewport_end_offset
    }
}
