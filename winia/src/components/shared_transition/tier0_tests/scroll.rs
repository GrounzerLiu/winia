use super::*;

#[test]
fn tier0_flight_scroll_addback_exact() {
    let _g = lock_serial();
    crate::animation::clear_all_animations();
    let mut composer = Composer::new();
    let show = State::new(true);
    let scroll = crate::modifier::ScrollState::new();

    scroll_frame(&mut composer, &show, &scroll);
    // Scroll AFTER first layout so the offset does not disturb settling.
    scroll.offset.set(40.0);
    scroll_frame(&mut composer, &show, &scroll);

    // Switch list → detail under a live (0, 40) ancestor scroll.
    show.set(false);
    scroll_frame(&mut composer, &show, &scroll);
    assert_eq!(composer.shared_flights.len(), 1, "flight opened under scroll");

    // Frozen scroll bookkeeping: detached source sums to zero, the live
    // target freezes the (0, 40) ancestor sum.
    {
        let a = composer.shared_flights.values().next().expect("flight");
        assert_eq!(a.start_scroll, (0.0, 0.0), "detached source is rootless");
        assert_eq!(a.end_scroll, (0.0, 40.0), "target freezes the ancestor scroll");
        let e = a.flight.end.expect("end resolved after first poll");
        // Render invariant: lerped end + add-back == pure layout origin,
        // i.e. the flight transform paints exactly on the lerped rect.
        let tslot = a.flight.target_slot.expect("target slot");
        let root = composer.layout_root_idx().expect("root");
        let tidx = find_idx_by_slot(composer.arena_nodes(), root, tslot).expect("target");
        let (px, py) = pure_accumulation(&composer, tidx);
        assert!(
            (e.x + a.end_scroll.0 - px).abs() < 1e-3
                && (e.y + a.end_scroll.1 - py).abs() < 1e-3,
            "end + scroll == pure layout origin ({} + {} vs {px},{py})",
            e.x,
            e.y
        );
    }
    // Visuals: the source is detached (rootless) and the target is
    // elevated (renderInOverlay defaults to true), so BOTH ends render
    // rootless from the layer and carry a zero add-back. The layer
    // supplies each end's scroll-corrected absolute origin instead, and
    // the paint still lands exactly on the lerped rect (pixel proof below).
    {
        let nodes = composer.arena_nodes();
        let a = composer.shared_flights.values().next().expect("flight");
        let sslot = a.flight.source_slot.expect("source slot");
        let sidx = a.source_idx.expect("retained source");
        assert!(nodes.get(sidx).is_some_and(|n| n.slot_key == sslot));
        let svis = nodes[sidx].transition.as_ref().expect("source visual");
        assert!(svis.elevated, "detached sources always fly in the layer");
        assert_eq!(svis.scroll, (0.0, 0.0));
        let tslot = a.flight.target_slot.expect("target slot");
        let root = composer.layout_root_idx().expect("root");
        let tidx = find_idx_by_slot(nodes, root, tslot).expect("target");
        let tvis = nodes[tidx].transition.as_ref().expect("target visual");
        assert!(tvis.elevated, "renderInOverlay defaults to true");
        assert_eq!(
            tvis.scroll,
            (0.0, 0.0),
            "layer canvas carries no ancestor translate — no add-back"
        );
        // The layer paints this node at its scroll-corrected absolute
        // origin, which differs from the pure layout accumulation by
        // exactly the frozen ancestor sum the in-tree path adds back.
        let id_to_idx: HashMap<u64, usize> =
            nodes.iter().enumerate().map(|(i, n)| (n.id, i)).collect();
        let abs = abs_rect_upward(nodes, &id_to_idx, tidx);
        let pure = pure_accumulation(&composer, tidx);
        assert!(
            (abs.y - (pure.1 - a.end_scroll.1)).abs() < 1e-3,
            "layer origin compensates the ancestor scroll exactly ({} vs {})",
            abs.y,
            pure.1 - a.end_scroll.1
        );
        let l = tvis.lerped();
        let clip = tvis.canvas_rrect();
        assert!(
            (clip.rect().left - (l.x + tvis.scroll.0)).abs() < 1e-3
                && (clip.rect().top - (l.y + tvis.scroll.1)).abs() < 1e-3,
            "canvas clip lands on the lerped rect"
        );
        let dev = tvis.screen_rrect();
        assert!(
            (dev.rect().left - l.x).abs() < 1e-3 && (dev.rect().top - l.y).abs() < 1e-3,
            "device clip lands on the lerped rect (hit-test frame)"
        );
    }

    // Pixel proof: the flight paints exactly on the lerped rect even though
    // the hero's ancestor is scrolled (with elevation the layer supplies
    // the absolute origin; the in-tree path supplies it as an add-back —
    // same pixels either way).
    let mut painted = false;
    for _ in 0..200 {
        if composer.shared_flights.is_empty() {
            break;
        }
        let probe = composer.shared_flights.values().next().map(|a| {
            let p = a.progress.peek();
            let e = a.flight.end.expect("end resolved after first poll");
            (p, a.start.lerp(&e, p))
        });
        if let Some((p, l)) = probe {
            if p > 0.3 && p < 0.7 && l.y > 24.0 {
                let mut surf = render_heads(&composer);
                let cx = (l.x + l.width / 2.0) as i32;
                let cy = (l.y + l.height / 2.0) as i32;
                let c = pixel_rgb(&mut surf, cx, cy);
                assert!(
                    c.0 > 140 && c.2 > 50,
                    "lerped center shows blended flight paint, got {c:?}"
                );
                let mut surf2 = render_heads(&composer);
                let above = pixel_rgb(&mut surf2, cx, (l.y - 12.0) as i32);
                assert!(
                    close_enough(above, (255, 255, 255), 40),
                    "no flight paint above the lerped rect, got {above:?}"
                );
                painted = true;
                break;
            }
        }
        scroll_advance(&mut composer, &show, &scroll);
    }
    assert!(painted, "flight must pass through the scroll-alignment window");

    for _ in 0..200 {
        if composer.shared_flights.is_empty() {
            break;
        }
        scroll_advance(&mut composer, &show, &scroll);
    }
    assert!(composer.shared_flights.is_empty(), "scrolled flight completes");
    crate::animation::clear_all_animations();
}

#[test]
fn ancestor_scroll_sum_accumulates_nested_scrollers() {
    use crate::layout::node::LayoutNode;
    let outer_v = crate::modifier::ScrollState::new();
    let inner_v = crate::modifier::ScrollState::new();
    let inner_h = crate::modifier::ScrollState::new();
    outer_v.offset.set(25.0);
    inner_v.offset.set(15.0);
    inner_h.offset.set(10.0);
    let root = LayoutNode::leaf(Modifier::new());
    let mut outer = LayoutNode::leaf(Modifier::new().vertical_scroll(outer_v));
    let mut inner = LayoutNode::leaf(
        Modifier::new().vertical_scroll(inner_v).horizontal_scroll(inner_h),
    );
    let mut leaf = LayoutNode::leaf(Modifier::new());
    let (rid, oid, iid) = (root.id, outer.id, inner.id);
    outer.parent_id = Some(rid);
    inner.parent_id = Some(oid);
    leaf.parent_id = Some(iid);
    let nodes = vec![root, outer, inner, leaf];
    let id_to_idx: HashMap<u64, usize> =
        nodes.iter().enumerate().map(|(i, n)| (n.id, i)).collect();
    // Canvas nests additively: 25 + 15 vertical, 10 horizontal.
    assert_eq!(ancestor_scroll_sum(&nodes, &id_to_idx, 3), (10.0, 40.0));
    // The node's own offset never counts (only strict ancestors).
    assert_eq!(ancestor_scroll_sum(&nodes, &id_to_idx, 2), (0.0, 25.0));
    // Rootless (detached) nodes sum to zero.
    assert_eq!(ancestor_scroll_sum(&nodes, &id_to_idx, 0), (0.0, 0.0));
}
