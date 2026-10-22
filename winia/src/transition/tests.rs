    use super::*;
    use crate::animation::{AnimatableValue, AnimationSpec, SpringSpec};
    use crate::runtime::state::State;

    /// The flown value is four components, so the engine runs one spring per edge — Compose's
    /// `Rect.VectorConverter` shape. A single scalar (what the flight used to animate) cannot express
    /// this.
    #[test]
    fn the_flown_value_is_four_edges() {
        assert!(<FlightEdges as AnimatableValue>::supports_spring());
        let mut out = [0.0f32; crate::animation::MAX_ANIMATION_COMPONENTS];
        let edges = FlightEdges::new(1.0, 2.0, 30.0, 40.0);
        assert_eq!(edges.write_components(&mut out), 4, "left, top, right, bottom");
        assert_eq!(&out[..4], &[1.0, 2.0, 30.0, 40.0]);
        assert_eq!(FlightEdges::from_components(&out[..4]), edges);
        let bounds = SharedBounds::new(1.0, 2.0, 29.0, 38.0);
        assert_eq!(FlightEdges::of(&bounds), edges, "right/bottom are edge coordinates");
        assert_eq!(edges.bounds(), bounds);
        assert_eq!(edges.lerp(&edges, 0.5), edges);
    }

    /// The point of four springs: an edge that barely moves settles while a far one is still
    /// travelling, which one scalar progress could never do — it put every edge at the same fraction
    /// of its own journey at all times.
    #[test]
    fn a_short_edge_settles_while_a_long_one_is_still_moving() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        // 500 px leftwards travel, 1 px downwards: the vertical edges are inside Compose's one-pixel
        // stop band from the start.
        let from = FlightEdges::new(0.0, 100.0, 200.0, 300.0);
        let to = FlightEdges::new(500.0, 101.0, 700.0, 301.0);
        let state = State::new(from);
        crate::animation::push_animatable(
            state.clone(),
            to,
            AnimationSpec::Spring(SpringSpec {
                threshold: BoundsTransform::VISIBILITY_THRESHOLD_PX,
                ..SpringSpec::default()
            }),
        );
        let mut frames = 0usize;
        let mut short_done_at = None;
        let mut long_done_at = None;
        while crate::animation::update_animations() && frames < 600 {
            std::thread::sleep(std::time::Duration::from_millis(8));
            crate::animation::update_animations();
            frames += 1;
            let now = state.peek();
            if short_done_at.is_none() && (now.top, now.bottom) == (to.top, to.bottom) {
                short_done_at = Some(frames);
            }
            if long_done_at.is_none() && (now.left - to.left).abs() < 1.0 {
                long_done_at = Some(frames);
            }
        }
        let (short_done_at, long_done_at) = (
            short_done_at.expect("the one-pixel edges reach their target"),
            long_done_at.expect("the 500 px edge reaches within a pixel of its target"),
        );
        assert!(
            short_done_at <= long_done_at,
            "the one-pixel edges are done no later than the 500 px one ({short_done_at} vs \
             {long_done_at}) — one scalar progress could not order them at all"
        );
        assert_eq!(state.peek(), to, "and every edge still lands exactly on its target");
    }
