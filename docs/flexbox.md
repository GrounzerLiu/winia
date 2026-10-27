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
- config_for must read its inputs INSIDE the resolver. A State read there is a layout dependency, so a
  write re-measures; a value captured from the enclosing composition is frozen once the container
  Skips, because a new closure is not one of the parameters the Skip decision compares. Compose has no
  such contract — its policy reads a rememberUpdatedState — and winia has no equivalent channel.
- The config cannot be built from a borrowed scope. Compose's FlexBoxConfig lambda receives a
  FlexBoxConfigScope with live accessors; winia hands config_for a FlexBoxConfigInput (the constraints
  and the density) by value.
- Baseline uses the child's first baseline unless the child sets a line with
  Modifier::align_by_baseline. Compose also accepts a line block on the container
  (alignItems(alignmentLineBlock)); that is not implemented.

## Tests

cargo test -p winia --lib -- layout::flexbox — 23 cases:

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

cargo test -p winia --features debug-server --test ui_flexbox_test drives examples/flexbox_demo.rs
through the debug server: six tiles shrink to the frame, wrap stops the shrink, justify-content packs
a line to the end, and the grow strip fills its width from grow 2 : 1 plus a fixed basis.

## History

Added 2026-10-27. Compose's FlexBox is stable in foundation-layout 1.13 (no experimental annotation).
