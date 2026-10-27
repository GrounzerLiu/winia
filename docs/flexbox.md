# FlexBox

A CSS-flex container: a main axis with grow / shrink / basis, line breaking, justify-content /
align-items / align-self / align-content, and a per-item order. This is Compose's FlexBox from
foundation-layout 1.13, not the Row/Column family.

## Sources

| Compose | Here |
| --- | --- |
| FlexBox(modifier, config, content) (FlexBox.kt:133) | FlexBox::new().config(..).build(ctx, ..) |
| FlexBoxConfig (FlexBox.kt:1791-1835) + FlexBoxConfigScope (:1836-2030) | FlexBoxConfig (a plain struct) + FlexBox::config_for |
| FlexConfig (:2215-2253) + FlexConfigScope (:2254-2418) | FlexItemConfig + Modifier::flex(..) |
| FlexDirection / FlexWrap / FlexJustifyContent / FlexAlignItems / FlexAlignSelf / FlexAlignContent / FlexBasis | the same names, same value sets |
| createFlexItem (:354-406), buildFlexLines (:417-496), resolveFlexibleLengths (:529-676), calculateLineCrossAxisSize (:916-968), applyAlignContentStretch (:677-713), measureFlexItems (:714-770), measureItem (:969-1010), calculateLineCrossPositions (:771-810), positionItemsOnMainAxis (:811-871), calculateItemCrossPosition (:872-915), placeFlexItems (:313-353), measureFlexBox (:192-311) | the six phases in winia/src/layout/flexbox.rs |

## API

    FlexBox::new()
        .config(
            FlexBoxConfig::new()
                .direction(FlexDirection::Row)
                .wrap(FlexWrap::Wrap)
                .justify_content(FlexJustifyContent::SpaceBetween)
                .align_items(FlexAlignItems::Center)
                .align_content(FlexAlignContent::Start)
                .gap(8.0)                       // row_gap + column_gap
                .max_items_in_each_line(3),
        )
        .modifier(Modifier::new().fill_max_width().height(320.0))
        .build(ctx, |ctx| {
            Stack::new()
                .modifier(
                    Modifier::new().flex(
                        FlexItemConfig::new()
                            .grow(1.0)
                            .basis_dp(96.0)
                            .align_self(FlexAlignSelf::Center),
                    ),
                )
                .build(ctx, |_| {});
        });

FlexItemConfig defaults to CSS: grow 0, shrink 1, basis Auto, align-self Auto, order 0.

A config that depends on the incoming constraints is resolved at measure time, like Compose's
FlexBoxConfig scope:

    FlexBox::new()
        .config_for(|input| {
            if input.constraints.max_width < 600.0 {
                FlexBoxConfig::new().direction(FlexDirection::Column)
            } else {
                FlexBoxConfig::new()
            }
        })

## Layout

Six phases, in the order Compose's measureFlexBox runs them:

1. Items. Each child's basis resolves to Dp (fixed), Percent x the container's main axis
   (max-content when the axis is unbounded) or Auto (max-content: the child measured with an
   unbounded main axis). The hypothetical main size is max(basis, min-content); that floor is what
   stops a shrink later. order re-sorts the item list (stable).
2. Lines. Greedy: an item joins the current line while the accumulated hypothetical sizes fit the
   container's main axis and the line holds fewer than max_items_in_each_line; NoWrap keeps one line.
   Each line's targets are resolved by the CSS 9.7 loop: free = container - sum(basis) - gaps;
   positive free space goes out by grow, negative by shrink x basis (scaled shrink), and an item that
   hits its min-content floor is frozen and the remainder re-distributed. A total grow/shrink below 1
   may only take its share of the INITIAL free space (9.7 step 4.b).
3. Cross sizes, in the order Compose runs them. (a) Items are measured at a tight main size, each line
   clamped to the cross space left for it (remainingCrossAxisSize); a line is as tall as its tallest
   item, Baseline lines use the union of every item's space above and below its first baseline (a
   child without a published line hangs entirely below it, so its baseline is its own height), and a
   NoWrap line is at least the incoming cross minimum. (b) align-content: Stretch then grows every
   line by an equal share of the leftover cross space. (c) Only now is a stretched item re-measured at
   its line's final extent, so it fills the grown line instead of the intrinsic one.
4. align-content. The free cross space is distributed over the lines; the gap between them is
   subtracted exactly once. WrapReverse stacks the lines from the cross-end.
5. Container size. Main = max(widest line, main minimum), clamped into the incoming range; cross = the
   lines' union, floored by the cross minimum and clamped into the incoming maximum.
6. Placement. justify-content positions the items inside each line (a single item is packed to the
   main-end when the direction is reversed, as Compose does), align-self / align-items positions each
   item inside its line, and under RTL only the x coordinate is mirrored (placeRelative).

## Behaviour

| Knob | Values | Notes |
| --- | --- | --- |
| direction | Row, RowReverse, Column, ColumnReverse | reverse flips the main-axis order, not the cross axis |
| wrap | NoWrap, Wrap, WrapReverse | WrapReverse is the cross-axis reverse |
| justify_content | Start, End, Center, SpaceBetween, SpaceAround, SpaceEvenly | inside one line |
| align_items / align_self | Start, End, Center, Stretch, Baseline / + Auto | Auto falls back to align-items |
| align_content | Start, End, Center, Stretch, SpaceBetween, SpaceAround | Compose has no SpaceEvenly here |
| basis | Auto, Dp, Percent | Percent is of the container's main axis |
| grow / shrink | f32 | shrink is weighted by the basis; a main-axis fill (fill_max_width) implies growth when grow is not declared |
| order | i32 | smaller first |

## Differences from Compose

- Modifier.flex(..) has no scope. Compose's FlexBoxScope makes the modifier only valid inside a
  FlexBox; in winia it is a plain Modifier and is silently ignored by any other container, the same
  convention layout_weight and align_self follow.
- Modifier.weight is not mapped to grow. winia's weight means "share the leftover", which is not CSS
  grow; a main-axis FILL is mapped (Compose FlexBox.kt:365-377), weight is not. Write grow explicitly.
- align-content in a wrap-reverse container is MIRRORED rather than walked backwards. Compose adds up the
  reversed lines from the free-space edge, which for SpaceBetween / SpaceAround leaves the block outside
  the cross extent (three 20 dp lines with 164 dp free in a 240 dp box land at 164 / 274 / 384). winia
  computes the distribution in the cross-start frame and mirrors it (220 / 110 / 0, the first line at
  the cross-start, the bottom).
- A resolver-bearing FlexBox never Skips. The config is a closure, and a closure is neither comparable
  nor installed on a Skip (the policy only changes when the group Enters), so a captured value would
  freeze. FlexBox::build therefore bumps a generation counter that makes the group Enter on every
  build; the cost is a rebuild of this subtree whenever the enclosing scope composes, which is exactly
  when the config could have changed. Compose compares the config OBJECT instead, so a changed lambda
  is a changed parameter there.
- The config cannot be built from a borrowed scope. Compose's FlexBoxConfig lambda receives a
  FlexBoxConfigScope with live accessors; winia hands config_for a FlexBoxConfigInput (the constraints
  and the density) by value.
- Baseline uses the child's first baseline unless the child sets a line with
  Modifier::align_by_baseline. Compose also accepts a line block on the container
  (alignItems(alignmentLineBlock)); that is not implemented.

## Tests

cargo test -p winia --lib -- layout::flexbox — 24 cases:

| Test | What it pins |
| --- | --- |
| a_row_packs_its_items_at_the_start | a row is as wide as its content |
| the_column_gap_separates_the_items | gaps are part of the line |
| justify_content_distributes_the_leftover | all six distributions at 300 dp |
| wrap_breaks_the_line_when_the_next_item_does_not_fit | lines of 2/1, container = widest line |
| max_items_in_each_line_breaks_a_line | the Compose-only line cap |
| grow_shares_the_free_space | equal grow weights split the space evenly |
| shrink_takes_space_proportionally | CSS shrink is weighted by the basis (and a fixed size cannot shrink) |
| a_filling_child_grows_into_the_free_space | a main-axis fill implies growth (Compose's mainAxisFillFraction fallback) |
| a_growing_item_cannot_grow_below_its_min_content | a min-content violation freezes the item and the rest is re-distributed, so the line never exceeds its container |
| a_percentage_basis_uses_the_container_main | Percent basis |
| row_reverse_puts_the_first_item_at_the_end | main-axis reversal |
| rtl_mirrors_the_main_axis | only x mirrors |
| align_items_centre_puts_the_item_in_the_middle_of_the_line | cross alignment inside a line |
| align_items_stretch_fills_the_line_cross | stretch re-measure |
| align_self_overrides_align_items | per-item override |
| align_content_space_between_distributes_the_lines | line distribution |
| align_content_does_not_subtract_the_cross_gap_twice | the gap is subtracted once (3 lines of 20 in 120 with a 10 gap sit at 0/50/100) |
| align_content_stretch_grows_the_stretched_items | stretch grows the lines BEFORE the items are measured |
| wrap_reverse_keeps_the_lines_inside_the_cross_extent | the line distribution is mirrored in wrap-reverse (see the deviation below) |
| order_reorders_the_items | CSS order |
| a_column_direction_swaps_the_axes | column mapping |
| an_empty_flex_box_takes_the_minimum | empty container |
| a_baseline_without_a_published_line_falls_back_to_the_item_height | an item without a line hangs below the baseline (its own height), so the two boxes align their bottoms |
| a_baseline_line_lines_up_the_texts | two real Texts share one first baseline |
| a_baseline_line_lines_up_the_texts | two real Texts of different sizes share one first baseline |

Lifecycle (a measure-only unit test cannot see these): cargo test -p winia --test flexbox_lifecycle

| Test | What it pins |
| --- | --- |
| a_config_for_resolver_sees_a_state_change | a State read inside config_for registers a LAYOUT dependency, so the resolver re-runs and the direction flips |
| a_child_flex_change_reaches_the_parent | a child's own Modifier::flex(..) changing re-measures the parent, even though the container's parameters did not change |
| a_resolver_capturing_an_ancestor_value_still_updates | a value read in an ANCESTOR scope still reaches config_for (the non-skippable path) |

### Combination sweep

cargo test -p winia --test flexbox_matrix — 9 sweeps, roughly 100 000 measured cases:

| Sweep | What it sweeps |
| --- | --- |
| the_matrix_respects_the_layout_invariants | 4 directions x 3 wraps x 6 justifies x 5 align-items x 6 align-contents x 4 counts x 3 item patterns x 4 constraint sets: finite sizes, the incoming range, placements matching the measured children, determinism |
| the_matrix_respects_the_behavioural_invariants | order under Start, no overlap inside a line, the single line inside the container, the exact justify arithmetic (Start/End/Center/SpaceBetween) and the exact align placement |
| the_matrix_mirrors_with_the_reverse_and_rtl_directions | RowReverse and RTL mirror the forward geometry |
| the_matrix_holds_on_the_column_axis_and_unbounded_axes | the same invariants on a column, plus f32::MAX axes |
| the_matrix_distributes_the_wrapped_lines | line block start, pitch and block containment for all six align-contents in both wrap modes |
| the_matrix_distributes_the_wrapped_lines_on_the_column_axis | the same on a column (cross = x) |
| the_matrix_covers_rtl_the_line_cap_order_and_nesting | RTL leaves the cross axis alone, max_items_in_each_line, order, a FlexBox inside a FlexBox |
| the_matrix_splits_a_line_evenly_between_equal_items | equal grow factors split a line evenly and fill the container |
| the_matrix_stretches_each_line_to_its_own_extent | with wrap, stretch uses each line's own cross extent |

examples/flexbox_demo.rs picks every parameter with material3 SEGMENTED BUTTONS (one row per knob, one
segment per value) and shows the resulting lines live; `cargo test -p winia --features debug-server --test
ui_flexbox_test` drives it
through the debug server: six tiles shrink to the frame, wrap stops the shrink, justify-content packs
a line to the end, and the grow strip fills its width from grow 2 : 1 plus a fixed basis.

## History

Added 2026-10-27. Compose's FlexBox is stable in foundation-layout 1.13 (no experimental annotation).
