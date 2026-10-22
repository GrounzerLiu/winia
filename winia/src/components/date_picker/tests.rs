    use super::*;

    /// Compose's `rememberDatePickerState` ends with `apply { this.selectableDates =
    /// selectableDates }` (`DatePicker.kt:384-389`, the property being `by mutableStateOf`), so the
    /// policy is LIVE while the initial values are the ones taken once.
    ///
    /// Both halves are asserted on purpose: a test that only looked at the second composition would
    /// pass if the state were rebuilt from scratch, and one that only looked at the first would pass
    /// if the policy were frozen at construction.
    #[test]
    fn remember_date_picker_state_takes_a_new_policy_but_keeps_its_initial_values() {
        struct RefusesEpoch;
        impl SelectableDates for RefusesEpoch {
            fn is_selectable_date(&self, utc_time_millis: i64) -> bool {
                utc_time_millis != 0
            }
        }

        let mut composer = crate::runtime::composer::Composer::new();
        let mut seen: Option<DatePickerState> = None;
        let mut frame = |composer: &mut crate::runtime::composer::Composer,
                         policy: Arc<dyn SelectableDates>,
                         seen: &mut Option<DatePickerState>| {
            composer.compose(|ctx| {
                let state = remember_date_picker_state(
                    ctx,
                    CalendarLocale::default(),
                    DatePickerStateInit {
                        initial_selected_date_millis: Some(0),
                        selectable_dates: policy.clone(),
                        ..DatePickerStateInit::default()
                    },
                );
                *seen = Some(state);
            });
        };

        frame(&mut composer, Arc::new(AllDates), &mut seen);
        let first = seen.clone().expect("a state");
        assert!(
            first.selectable_dates().is_selectable_date(0),
            "the first composition's policy is the one in force"
        );
        assert_eq!(first.selected_date_millis(), Some(0), "and its initial selection");

        // A NEW Arc, which is what a caller building its policy inline hands over.
        frame(&mut composer, Arc::new(RefusesEpoch), &mut seen);
        let second = seen.clone().expect("a state");
        assert!(
            !second.selectable_dates().is_selectable_date(0),
            "the second composition's policy is taken up, not the one remembered with"
        );
        assert_eq!(
            second.selected_date_millis(),
            Some(0),
            "while the initial values stay the ones taken at construction"
        );
    }

    /// A locale whose input format orders the fields a particular way.
    fn locale_with_input_format(pattern: &str) -> CalendarLocale {
        CalendarLocale {
            date_input_format: DateInputFormat::from_pattern(pattern)
                .unwrap_or_else(|| panic!("{pattern} is a valid input pattern")),
            ..CalendarLocale::default()
        }
    }

    /// A locale format pattern is widened to two-digit day and month and a four-digit year, and
    /// everything that is not `d`, `M`, `y` or a separator is dropped — the cleanup
    /// `datePatternAsInputFormat` does (`CalendarModel.kt:296-315`).
    #[test]
    fn a_locale_pattern_is_normalized_into_an_input_format() {
        let cases = [
            ("M/d/yyyy", "MM/dd/yyyy", '/'),
            ("dd.MM.yyyy", "dd.MM.yyyy", '.'),
            ("y/M/d", "yyyy/MM/dd", '/'),
            ("yyyy-MM-dd", "yyyy-MM-dd", '-'),
            ("d/M/yy", "dd/MM/yyyy", '/'),
        ];
        for (locale_pattern, expected, delimiter) in cases {
            let format = DateInputFormat::from_pattern(locale_pattern).expect(locale_pattern);
            assert_eq!(format.pattern_with_delimiters(), expected, "{locale_pattern}");
            assert_eq!(format.delimiter(), delimiter, "{locale_pattern}");
            assert_eq!(format.pattern_length(), 8, "{locale_pattern}");
        }
    }

    /// A pattern that does not name each field exactly once, or that carries no separator, has
    /// no input format: there would be nowhere to put a separator and no way to cut the digits.
    #[test]
    fn a_pattern_that_cannot_name_three_fields_yields_no_format() {
        for pattern in ["MM/dd", "MM/dd/yyyy/yyyy", "dd/MM", "", "HH:mm", "dMy"] {
            assert!(
                DateInputFormat::from_pattern(pattern).is_none(),
                "{pattern} should not become an input format"
            );
        }
    }

    /// The field order is what the locale decides, and the delimiters sit where the pattern puts
    /// them — both of which the visual transformation and the parser key off.
    #[test]
    fn the_field_order_and_delimiter_offsets_come_off_the_pattern() {
        let cases = [
            ("MM/dd/yyyy", DateInputFieldOrder::MonthDayYear, 2, 5),
            ("dd/MM/yyyy", DateInputFieldOrder::DayMonthYear, 2, 5),
            ("yyyy/MM/dd", DateInputFieldOrder::YearMonthDay, 4, 7),
            ("dd.MM.yyyy", DateInputFieldOrder::DayMonthYear, 2, 5),
        ];
        for (pattern, order, first, last) in cases {
            let format = DateInputFormat::from_pattern(pattern).expect(pattern);
            assert_eq!(format.field_order(), order, "{pattern}");
            assert_eq!(format.first_delimiter_offset(), first, "{pattern}");
            assert_eq!(format.last_delimiter_offset(), last, "{pattern}");
            assert_eq!(format.pattern_without_delimiters().len(), 8, "{pattern}");
        }
    }

    /// The same digits read through three orderings are three different dates, which is the whole
    /// point of the pattern carrying the order.
    #[test]
    fn the_same_digits_read_into_three_different_dates() {
        let model = CalendarModel::new(CalendarLocale::default());
        let march_first = CalendarDate::new(2024, 3, 1).expect("2024-03-01 is a date");

        let us = model.date_input_format().clone();
        assert_eq!(us.field_order(), DateInputFieldOrder::MonthDayYear);
        assert_eq!(model.parse("03012024", &us), Some(march_first));
        assert_eq!(model.format_with_pattern(march_first.start_of_day_millis(), &us), "03012024");

        let gb = DateInputFormat::from_pattern("dd/MM/yyyy").expect("dd/MM/yyyy");
        assert_eq!(gb.field_order(), DateInputFieldOrder::DayMonthYear);
        assert_eq!(model.parse("01032024", &gb), Some(march_first));
        assert_eq!(model.format_with_pattern(march_first.start_of_day_millis(), &gb), "01032024");

        let jp = DateInputFormat::from_pattern("yyyy/MM/dd").expect("yyyy/MM/dd");
        assert_eq!(jp.field_order(), DateInputFieldOrder::YearMonthDay);
        assert_eq!(model.parse("20240301", &jp), Some(march_first));
        assert_eq!(model.format_with_pattern(march_first.start_of_day_millis(), &jp), "20240301");
    }

    /// Every day of a leap February round-trips through every field order: this is the property
    /// the field falls back on when the user edits the selection, not just the placeholder.
    #[test]
    fn a_leap_day_round_trips_through_every_field_order() {
        let model = CalendarModel::new(CalendarLocale::default());
        let leap_day = CalendarDate::new(2024, 2, 29).expect("2024 is a leap year");
        for pattern in ["MM/dd/yyyy", "dd/MM/yyyy", "yyyy/MM/dd"] {
            let format = DateInputFormat::from_pattern(pattern).expect(pattern);
            let digits = model.format_with_pattern(leap_day.start_of_day_millis(), &format);
            assert_eq!(digits.len(), 8, "{pattern}");
            assert_eq!(model.parse(&digits, &format), Some(leap_day), "{pattern}");
        }
    }

    /// An incomplete or non-numeric entry has no answer, and so does a complete one that names a
    /// day its month does not have. This is the first of the validator's three checks.
    #[test]
    fn parsing_refuses_partial_non_numeric_and_impossible_dates() {
        let model = CalendarModel::new(CalendarLocale::default());
        let us = model.date_input_format();
        for digits in ["", "0", "03", "030", "03012", "0301202", "030120242"] {
            assert_eq!(model.parse(digits, us), None, "{digits:?} is incomplete");
        }
        for digits in [" 3012024", "03-012024", "0301202a", "０３０１"] {
            assert_eq!(model.parse(digits, us), None, "{digits:?} is not digits");
        }
        // 2024-02-30, 2023-02-29 (not a leap year), month 13, day 0.
        for digits in ["02302024", "02292023", "13312024", "00012024"] {
            assert_eq!(model.parse(digits, us), None, "{digits:?} is not a date");
        }
        // The same day the month does have, one field-order over, is not a rescue.
        let gb = DateInputFormat::from_pattern("dd/MM/yyyy").expect("dd/MM/yyyy");
        assert_eq!(model.parse("30022024", &gb), None);
    }

    /// Dates before the epoch keep their sign-free four-digit year, because the year is written
    /// and read as exactly four digits.
    #[test]
    fn a_date_before_the_epoch_keeps_a_four_digit_year() {
        let model = CalendarModel::new(CalendarLocale::default());
        let us = model.date_input_format().clone();
        let digits = model.format_with_pattern(0, &us);
        assert_eq!(digits, "01011970");
        assert_eq!(model.parse(&digits, &us), CalendarDate::new(1970, 1, 1));
    }

    /// A picker with everything at its defaults: 1900..=2100, every date selectable.
    fn picker() -> DatePickerState {
        DatePickerState::new(CalendarLocale::default())
    }

    /// The three checks run in material3's order and each says what it has to say. The order is
    /// the point of the first two cases: an unparseable entry never reaches the range check, and a
    /// date outside the range is reported as the range rather than as the policy refusing it.
    #[test]
    fn the_input_field_reports_pattern_range_and_policy_failures_apart() {
        let state = picker();

        assert_eq!(
            state.validate_date_input(None),
            "Date does not match expected pattern: MM/DD/YYYY"
        );

        let outside = CalendarDate::new(1800, 5, 4).expect("1800-05-04 is a date");
        assert_eq!(
            state.validate_date_input(Some(outside)),
            "Date out of expected year range 1900 - 2100"
        );

        assert_eq!(state.validate_date_input(CalendarDate::new(2024, 3, 1)), "");
    }

    /// The year range's two ends land in their own slots. A formatter that filled both with one
    /// value would still look plausible on a single-year range, so this uses a wide one.
    #[test]
    fn the_year_range_message_names_both_ends() {
        let message = picker().validate_date_input(CalendarDate::new(1800, 5, 4));
        assert_eq!(message, "Date out of expected year range 1900 - 2100");
        assert!(message.contains("1900"), "{message}");
        assert!(message.contains("2100"), "{message}");
    }

    /// A policy that refuses a day is told so by name and date, not by a generic failure.
    #[test]
    fn a_refused_day_is_named_rather_than_reported_as_a_range_problem() {
        struct OnlyWeekdays;
        impl SelectableDates for OnlyWeekdays {
            fn is_selectable_date(&self, utc_time_millis: i64) -> bool {
                // 2024-03-01 is a Friday; 2024-03-02 a Saturday.
                date_of_millis(utc_time_millis).day != 2
            }
        }
        let state = DatePickerState::with(
            CalendarLocale::default(),
            DatePickerStateInit {
                selectable_dates: Arc::new(OnlyWeekdays),
                ..DatePickerStateInit::default()
            },
        );
        assert_eq!(state.validate_date_input(CalendarDate::new(2024, 3, 1)), "");
        let refused = state.validate_date_input(CalendarDate::new(2024, 3, 2));
        assert_eq!(refused, "Date not allowed: Mar 2, 2024");
    }

    /// A year the policy refuses stops every date in it, even a day the policy would otherwise
    /// allow — material3 checks `isSelectableYear` in the same condition (`DateInput.kt:330-333`).
    #[test]
    fn a_year_the_policy_refuses_stops_every_date_in_it() {
        struct NotTwentyTwentyFour;
        impl SelectableDates for NotTwentyTwentyFour {
            fn is_selectable_year(&self, year: i32) -> bool {
                year != 2024
            }
        }
        let state = DatePickerState::with(
            CalendarLocale::default(),
            DatePickerStateInit {
                selectable_dates: Arc::new(NotTwentyTwentyFour),
                ..DatePickerStateInit::default()
            },
        );
        // Every March date would otherwise pass, so only the year check can reject this.
        let refused = state.validate_date_input(CalendarDate::new(2024, 3, 1));
        assert_eq!(refused, "Date not allowed: Mar 1, 2024");
        assert_eq!(state.validate_date_input(CalendarDate::new(2025, 3, 1)), "");
    }

    /// The pattern message carries the locale's own pattern, uppercased, so a reader knows what
    /// shape was expected — and a non-en-US locale says its own.
    #[test]
    fn the_pattern_message_names_the_locales_own_pattern() {
        let state = DatePickerState::new(locale_with_input_format("yyyy/MM/dd"));
        assert_eq!(
            state.validate_date_input(None),
            "Date does not match expected pattern: YYYY/MM/DD"
        );
    }

    /// The substitution Compose relies on: two arguments fill their own slots, a missing one
    /// leaves an empty hole instead of panicking, and text that only looks like a placeholder
    /// passes through.
    #[test]
    fn format_string_fills_numbered_placeholders_in_order() {
        assert_eq!(format_string("a {1} b {2} c", &["1", "2"]), "a 1 b 2 c");
        assert_eq!(format_string("{2} then {1}", &["first", "second"]), "second then first");
        assert_eq!(format_string("{1} only", &[]), " only");
        assert_eq!(format_string("no placeholders", &["x"]), "no placeholders");
        assert_eq!(format_string("{name} stays", &["x"]), "{name} stays");
        // One-based, as `%1$s` is: `{0}` is out of range and fills nothing.
        assert_eq!(format_string("{1} {0} {2}", &["a", "b"]), "a  b");
        assert_eq!(format_string("unclosed {1", &["a"]), "unclosed {1");
    }

    /// The delimiters appear as the digits are typed, one after the field they close, and a full
    /// entry shows the pattern exactly.
    #[test]
    fn the_visual_transformation_writes_the_delimiters_as_the_digits_arrive() {
        let format = DateInputFormat::from_pattern("MM/dd/yyyy").expect("MM/dd/yyyy");
        let transformation = DateVisualTransformation::new(&format);
        let shown = |digits: &str| transformation.filter(digits).text;
        assert_eq!(shown(""), "");
        assert_eq!(shown("0"), "0");
        assert_eq!(shown("03"), "03/");
        assert_eq!(shown("030"), "03/0");
        assert_eq!(shown("0301"), "03/01/");
        assert_eq!(shown("030120"), "03/01/20");
        assert_eq!(shown("03012024"), "03/01/2024");
        // A longer entry is cut at a full field's width rather than pushing the end out.
        assert_eq!(shown("03012024999"), "03/01/2024");
    }

    /// A non-default field order puts its delimiters where its own pattern does: `yyyy/MM/dd`
    /// closes a four-digit year first, so the first separator lands after the fourth digit and
    /// the display is two characters longer before it appears.
    #[test]
    fn the_visual_transformation_follows_the_locale_field_order() {
        let cases = [
            ("MM/dd/yyyy", "03012024", "03/01/2024", [(2, "03/"), (4, "03/01/")]),
            ("dd/MM/yyyy", "01032024", "01/03/2024", [(2, "01/"), (4, "01/03/")]),
            ("yyyy/MM/dd", "20240301", "2024/03/01", [(4, "2024/"), (6, "2024/03/")]),
            ("dd.MM.yyyy", "01032024", "01.03.2024", [(2, "01."), (4, "01.03.")]),
        ];
        for (pattern, digits, full, prefixes) in cases {
            let format = DateInputFormat::from_pattern(pattern).expect(pattern);
            let transformation = DateVisualTransformation::new(&format);
            assert_eq!(transformation.filter(digits).text, full, "{pattern}");
            for (width, prefix) in prefixes {
                assert_eq!(
                    transformation.filter(&digits[..width]).text,
                    prefix,
                    "{pattern} at {width} digits"
                );
            }
        }
    }

    /// Every caret position maps to the position of the same digit both ways, and past the end
    /// both clamp. This is the whole contract of the two methods: get one wrong and the caret
    /// jumps a character as soon as it crosses a delimiter.
    #[test]
    fn every_caret_position_maps_to_the_same_digit_both_ways() {
        let format = DateInputFormat::from_pattern("MM/dd/yyyy").expect("MM/dd/yyyy");
        let transformation = DateVisualTransformation::new(&format);
        let mapping = transformation.filter("03012024").offset_mapping;

        // Caret before each digit: typed offset -> shown offset -> back.
        for original in 0..=8 {
            let shown = mapping.original_to_transformed(original);
            assert!(shown <= 10, "{original} -> {shown} ran past the end");
            assert_eq!(mapping.transformed_to_original(shown), original, "offset {original}");
        }
        // The exact pairs, so a change in one direction cannot hide behind the round trip.
        let pairs = [(0, 0), (1, 1), (2, 3), (3, 4), (4, 5), (5, 7), (6, 8), (7, 9), (8, 10)];
        for (original, shown) in pairs {
            assert_eq!(mapping.original_to_transformed(original), shown, "forward {original}");
            assert_eq!(mapping.transformed_to_original(shown), original, "back {shown}");
        }
        // Past the entry, both directions clamp to its end rather than walking off.
        assert_eq!(mapping.original_to_transformed(9), 10);
        assert_eq!(mapping.original_to_transformed(200), 10);
        assert_eq!(mapping.transformed_to_original(11), 8);
        assert_eq!(mapping.transformed_to_original(200), 8);
    }

    /// The offsets are read off the pattern, so the year-first order's mapping is not the
    /// month-first one's shifted — it is a different function.
    #[test]
    fn the_year_first_order_maps_its_own_offsets() {
        let format = DateInputFormat::from_pattern("yyyy/MM/dd").expect("yyyy/MM/dd");
        let mapping = DateVisualTransformation::new(&format).filter("20240301").offset_mapping;
        let pairs = [(0, 0), (3, 3), (4, 5), (6, 7), (7, 9), (8, 10)];
        for (original, shown) in pairs {
            assert_eq!(mapping.original_to_transformed(original), shown, "forward {original}");
            assert_eq!(mapping.transformed_to_original(shown), original, "back {shown}");
        }
        for original in 0..=8 {
            let shown = mapping.original_to_transformed(original);
            assert_eq!(mapping.transformed_to_original(shown), original, "offset {original}");
        }
    }

    /// The month arrows step one page per press, in BOTH directions, however fast the presses come.
    ///
    /// The asymmetry this pins was reported from the demo: three presses 120 ms apart on `prev`
    /// advanced two months, the same three on `next` advanced none. The cause is that `first_visible`
    /// names the page whose span still contains the offset, so during a forward animation it is the page
    /// being LEFT — `anchor + 1` then names the page already in flight, the request deduplicates against
    /// it, and the press is lost. Backward flips the anchor the moment the offset leaves the old page,
    /// so `anchor - 1` names something new.
    ///
    /// Stepping from the page already REQUESTED removes the direction from the outcome. Each case below
    /// asserts the whole trajectory of targets, not just the last one: a fix that happened to land on
    /// the right page while skipping one would pass a final-state check.
    #[test]
    fn a_second_press_during_the_animation_steps_again_in_both_directions() {
        const LAST: usize = 2411;

        // Three presses during one animation, starting from page 100. Forward: 101, 102, 103.
        let mut in_flight = None;
        let mut targets = Vec::new();
        for _ in 0..3 {
            let t = arrow_target(100, in_flight, true, LAST);
            in_flight = Some(t);
            targets.push(t);
        }
        assert_eq!(
            targets,
            vec![101, 102, 103],
            "three forward presses during one animation must be three pages, not one repeated target"
        );

        // And backward, from the same anchor: 99, 98, 97.
        let mut in_flight = None;
        let mut targets = Vec::new();
        for _ in 0..3 {
            let t = arrow_target(100, in_flight, false, LAST);
            in_flight = Some(t);
            targets.push(t);
        }
        assert_eq!(targets, vec![99, 98, 97], "and the two directions must agree in shape");

        // A press after the motion is honoured falls back to the anchor, which by then agrees with the
        // in-flight page — the sync clears the memory when the list arrives, so this is the same answer
        // either way rather than a stale one.
        assert_eq!(
            arrow_target(103, None, true, LAST),
            104,
            "with nothing in flight a press steps from where the list actually is"
        );
    }

    /// The ends. An arrow is disabled there, but a step must still be well-defined rather than wrapping.
    #[test]
    fn the_arrow_targets_clamp_at_both_ends_of_the_range() {
        assert_eq!(arrow_target(0, None, false, 10), 0, "no page before the first");
        assert_eq!(arrow_target(0, Some(0), false, 10), 0, "nor from an in-flight request at the start");
        assert_eq!(arrow_target(10, None, true, 10), 10, "no page past the last");
        assert_eq!(
            arrow_target(10, Some(10), true, 10),
            10,
            "nor from an in-flight request at the end — the request cannot climb past the clamp"
        );
        assert_eq!(arrow_target(usize::MAX, None, false, 10), usize::MAX - 1, "a step down never wraps");
    }

    /// The bound on [`sync_month_pages`]'s two waits, and the reason it exists.
    ///
    /// `is_scrolling()` is NOT a flag with one owner — it is the same `State` the drag, fling,
    /// programmatic-jump and cancellation paths all write, and at least two of those can leave it set:
    /// `cancel_animation_by_id` / `clear_animations_for_states` drop an animation with `retain` and never
    /// run its finish callback (`animation.rs:580-583`, `:43-49`), and both `drag_scroll_up` paths return
    /// before their reset when the drag's target node has gone (`app.rs:3021`, `app.rs:3936`). Compose's
    /// guard is safe because a single owner maintains that flag with a guaranteed reset; here it is
    /// shared, so an unbounded wait would turn ONE leaked flag into a month label frozen for the life of
    /// the window — strictly worse than the gesture cancellation the guard exists to prevent.
    ///
    /// The observable is `jump_request`: the jump is issued through `scroll_to_item`, which parks the
    /// request for the measure to consume, and a bare `compose` never measures — so the request is still
    /// sitting there to be read. Two things are asserted, and both matter: that the deferral happens at
    /// all (a sync that barged in immediately would cancel the gesture it is guarding), and that it ENDS
    /// (which is the anti-freeze half).
    #[test]
    fn a_stuck_scroll_flag_cannot_freeze_the_month_sync_forever() {
        let list = LazyListState::new();
        let model = CalendarModel::new(CalendarLocale::default());
        let state = DatePickerState::with(
            CalendarLocale::default(),
            DatePickerStateInit {
                initial_displayed_month_millis: Some(millis(2024, 9, 1)),
                today_millis: Some(millis(2024, 9, 5)),
                ..Default::default()
            },
        );
        let first_month = model.month_of(*state.year_range().start(), 1).start_utc_time_millis;
        // A gesture that never finishes. `first_visible` stays 0, so the sync is asked to take the list
        // to September 2024's page (1496) and can never see it arrive.
        list.is_scrolling.set(true);
        assert_ne!(
            state.displayed_month_millis(),
            model.plus_months(first_month, list.first_visible() as i64).start_utc_time_millis,
            "the test is vacuous unless the state's month differs from the page the list is on"
        );

        let mut composer = crate::runtime::composer::Composer::new();
        let mut deferred = 0usize;
        let mut issued_on = None;
        let in_flight = State::new(None::<usize>);
        for frame in 1..=(MAX_WAIT_FRAMES as usize + 4) {
            composer.compose(|ctx| {
                sync_month_pages(ctx, &state, &model, &list, first_month, &in_flight);
            });
            if list.jump_request.peek().is_some() {
                issued_on = Some(frame);
                break;
            }
            deferred += 1;
        }

        let issued_on = issued_on.expect(
            "the sync never gave up waiting: a leaked `is_scrolling` would freeze the month forever",
        );
        assert!(
            issued_on > 1,
            "the sync must defer at least a frame before giving up — it waited {deferred} frame(s) \
             and then jumped on frame {issued_on}, which is the gesture cancellation the guard prevents"
        );
        assert_eq!(
            issued_on,
            MAX_WAIT_FRAMES as usize + 1,
            "the deferral ends exactly at the bound, not early and not late"
        );

        // And the flag is still the input, not the output: a list that reports it is idle jumps on the
        // very first frame.
        let idle = LazyListState::new();
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            sync_month_pages(ctx, &state, &model, &idle, first_month, &State::new(None::<usize>));
        });
        assert!(
            idle.jump_request.peek().is_some(),
            "an idle list is taken to the month immediately — the wait is only for a scroll in flight"
        );
    }

    #[test]
    fn the_epoch_is_a_thursday() {
        // 1970-01-01 is the reference point of the whole module, and Thursday is 4 when Monday is 1.
        assert_eq!(CalendarDate::new(1970, 1, 1).unwrap().day_of_week(), 4);
        assert_eq!(CalendarDate::new(1970, 1, 1).unwrap().days_since_epoch(), 0);
    }

    /// "Today" is the LOCAL calendar date, stamped at UTC midnight — Compose's `LocalDate.now()` then
    /// `.atTime(MIDNIGHT).atZone(utcTimeZoneId)` (`CalendarModelImpl.android.kt:48-62`). Reading the UTC
    /// instant directly rings the previous day for every user whose local date has already rolled over.
    #[test]
    fn today_is_the_local_date_stamped_at_utc_midnight() {
        const HOUR: i64 = 3_600_000;
        /// 2024-09-01T00:00Z, the UTC midnight the local dates below are stamped at.
        const SEP_1: i64 = 1_725_148_800_000;
        // UTC+8 at 01:00 local on the 2nd: the UTC instant is still the 1st, the local date is not.
        let after_local_midnight = SEP_1 + 17 * HOUR; // 2024-09-01T17:00Z
        assert_eq!(date_of_millis(after_local_midnight), CalendarDate { year: 2024, month: 9, day: 1 });
        assert_eq!(
            date_of_millis(today_at(after_local_midnight, 8 * HOUR)),
            CalendarDate { year: 2024, month: 9, day: 2 },
            "at UTC+8 the local date is already the 2nd"
        );
        // The same instant in UTC is still the 1st — which is the bug this offset exists to avoid.
        assert_eq!(
            date_of_millis(today_at(after_local_midnight, 0)),
            CalendarDate { year: 2024, month: 9, day: 1 }
        );
        // West of UTC the correction runs the other way: UTC-5 at 20:00 local on the 1st is already the
        // 2nd in UTC, and the local date is the one that has to win.
        assert_eq!(
            date_of_millis(today_at(SEP_1 + 25 * HOUR, -5 * HOUR)),
            CalendarDate { year: 2024, month: 9, day: 1 },
            "at UTC-5 the local date is still the 1st while UTC has moved on"
        );
        // The stamped value is still a UTC day boundary, which is what every other reader here assumes,
        // and a real zone offset can only move the answer by the day it actually straddles.
        let instant = SEP_1 + 9 * HOUR; // 2024-09-01T09:00Z
        for offset in [-12 * HOUR, -5 * HOUR, 0, 5 * HOUR + 30 * 60_000, 14 * HOUR] {
            let today = today_at(instant, offset);
            assert_eq!(today % MILLIS_IN_24_HOURS, 0, "today {today} is not on a UTC day boundary");
            assert!(
                (today - canonical_millis(instant)).abs() <= MILLIS_IN_24_HOURS,
                "an offset of {offset} moved today {today} more than a day from the instant"
            );
        }
    }

    /// The live probe behind it: whatever zone this machine is in, the offset has to be one a real zone
    /// could carry. A garbage answer here is the one failure mode `today_at` cannot catch.
    ///
    /// This is a SMOKE CHECK, not a regression guard. A probe hard-wired to return 0 satisfies both
    /// assertions on every machine, and 0 is the correct answer on a UTC machine anyway — so this says
    /// "the zone lookup returned something legal", nothing more. The arithmetic is pinned separately by
    /// `today_is_the_local_date_stamped_at_utc_midnight` with synthetic offsets, and the wiring is
    /// pinned by `today_millis_reads_the_local_day_not_the_utc_one`.
    #[test]
    fn the_local_offset_is_a_whole_number_of_minutes_in_range() {
        let offset = local_utc_offset_millis();
        assert_eq!(offset % 60_000, 0, "offset {offset} is not a whole number of minutes");
        assert!(
            offset.abs() <= 14 * 3_600_000,
            "offset {offset} is outside the real range of UTC-12..UTC+14"
        );
    }

    /// The production call, not the helper: `CalendarModel::today_millis` must be the LOCAL day.
    ///
    /// ⚠ This can only bite while the machine is inside the window where its local date and its UTC date
    /// disagree — eight hours a day at UTC+8, zero on a UTC machine. That is a real limitation, not a
    /// formality: the clock is read inside the function and there is no seam to move it, so the one
    /// assertion that distinguishes "applies the offset" from "reads the UTC instant" is only available
    /// during that window. It is written to be skipped loudly rather than to pass quietly, so a suite run
    /// that never exercises it says so. The arithmetic is deterministic and covered by the test above;
    /// this covers the composition.
    #[test]
    fn today_millis_reads_the_local_day_not_the_utc_one() {
        let offset = local_utc_offset_millis();
        let model = CalendarModel::new(CalendarLocale::default());
        let answered = model.today_millis();

        if offset == 0 {
            eprintln!("today_millis_reads_the_local_day_not_the_utc_one: skipped — this machine is UTC, \
                       where the UTC day IS the local day and the two cannot be told apart");
            return;
        }
        // The two days, read the same way the function does, a moment either side of the call. A
        // microsecond of clock movement between the three reads is the only slack allowed.
        let utc_day = canonical_millis(now_millis());
        let local_day = today_at(now_millis(), offset);
        if utc_day == local_day {
            eprintln!(
                "today_millis_reads_the_local_day_not_the_utc_one: skipped — outside the window where \
                 this machine's local date and UTC date disagree; rerun within {} hours of local midnight",
                offset.abs() / 3_600_000
            );
            return;
        }
        assert_eq!(
            answered, local_day,
            "today_millis answered the UTC day ({answered}) while the local date is a different one"
        );
    }

    #[test]
    fn civil_days_round_trip_over_three_centuries() {
        for year in 1800..=2200 {
            for month in 1..=12u32 {
                for day in 1..=days_in_month(year, month) {
                    let days = days_from_civil(year, month, day);
                    assert_eq!(
                        civil_from_days(days),
                        (year, month, day),
                        "round trip {year}-{month}-{day}"
                    );
                    assert_eq!(date_of_millis(days * MILLIS_IN_24_HOURS).year, year);
                }
            }
        }
    }

    #[test]
    fn february_has_29_days_only_in_leap_years() {
        assert_eq!(days_in_month(1900, 2), 28, "1900 is not a leap year");
        assert_eq!(days_in_month(2000, 2), 29, "2000 is a leap year");
        assert_eq!(days_in_month(2024, 2), 29);
        assert_eq!(days_in_month(2023, 2), 28);
    }

    #[test]
    fn a_day_is_exactly_one_day_of_millis() {
        assert_eq!(
            CalendarDate::new(2024, 9, 2).unwrap().start_of_day_millis()
                - CalendarDate::new(2024, 9, 1).unwrap().start_of_day_millis(),
            MILLIS_IN_24_HOURS
        );
    }

    #[test]
    fn milliseconds_before_the_epoch_land_on_the_right_day() {
        // Truncating division would put -1ms on 1970-01-01; the day before the epoch is 1969-12-31.
        assert_eq!(date_of_millis(-1), CalendarDate::new(1969, 12, 31).unwrap());
        assert_eq!(canonical_millis(-1), -MILLIS_IN_24_HOURS);
    }

    #[test]
    fn the_month_offset_counts_from_the_locales_first_day() {
        // 2024-09-01 is a Sunday, so a Sunday-first week has no empty cells and a Monday-first week has six.
        let sunday_first = CalendarModel::new(CalendarLocale::default());
        assert_eq!(sunday_first.first_day_of_week(), 7);
        assert_eq!(
            sunday_first.month_of(2024, 9).days_from_start_of_week_to_first_of_month,
            0
        );

        let mut monday_first_locale = CalendarLocale::default();
        monday_first_locale.first_day_of_week = 1;
        let monday_first = CalendarModel::new(monday_first_locale);
        assert_eq!(
            monday_first.month_of(2024, 9).days_from_start_of_week_to_first_of_month,
            6
        );

        // 2021-01-01 is a Friday: five cells after a Sunday, four after a Monday.
        assert_eq!(
            sunday_first.month_of(2021, 1).days_from_start_of_week_to_first_of_month,
            5
        );
        assert_eq!(
            monday_first.month_of(2021, 1).days_from_start_of_week_to_first_of_month,
            4
        );
    }

    #[test]
    fn the_weekday_row_starts_at_the_locales_first_day() {
        let sunday_first = CalendarModel::new(CalendarLocale::default());
        assert_eq!(sunday_first.weekday_names()[0].0, "Sunday");
        assert_eq!(sunday_first.weekday_names()[6].0, "Saturday");

        let mut monday_first_locale = CalendarLocale::default();
        monday_first_locale.first_day_of_week = 1;
        let monday_first = CalendarModel::new(monday_first_locale);
        assert_eq!(monday_first.weekday_names()[0].0, "Monday");
        assert_eq!(monday_first.weekday_names()[6].0, "Sunday");
    }

    #[test]
    fn a_month_reports_its_own_length_and_start() {
        let model = CalendarModel::new(CalendarLocale::default());
        let february = model.month_of(2024, 2);
        assert_eq!(february.number_of_days, 29);
        assert_eq!(february.start_utc_time_millis % MILLIS_IN_24_HOURS, 0);
        assert_eq!(model.canonical_date(february.start_utc_time_millis).day, 1);
    }

    #[test]
    fn plus_months_crosses_year_boundaries_both_ways() {
        let model = CalendarModel::new(CalendarLocale::default());
        let november = model.month_of(2024, 11).start_utc_time_millis;
        let february = model.plus_months(november, 3);
        assert_eq!((february.year, february.month), (2025, 2));
        let august = model.plus_months(november, -3);
        assert_eq!((august.year, august.month), (2024, 8));
    }

    #[test]
    fn a_months_index_is_its_position_in_the_year_range() {
        let model = CalendarModel::new(CalendarLocale::default());
        assert_eq!(model.month_of(1900, 1).index_in(&DEFAULT_YEAR_RANGE), 0);
        assert_eq!(model.month_of(1900, 2).index_in(&DEFAULT_YEAR_RANGE), 1);
        assert_eq!(model.month_of(1901, 1).index_in(&DEFAULT_YEAR_RANGE), 12);
        assert_eq!(
            CalendarModel::number_of_months_in_range(&DEFAULT_YEAR_RANGE),
            (2100 - 1900 + 1) * 12
        );
    }

    #[test]
    fn the_year_range_holds_the_material3_default() {
        assert_eq!(*DEFAULT_YEAR_RANGE.start(), 1900);
        assert_eq!(*DEFAULT_YEAR_RANGE.end(), 2100);
    }

    #[test]
    fn dates_format_as_the_header_and_the_field_show_them() {
        let model = CalendarModel::new(CalendarLocale::default());
        let september = model.month_of(2024, 9).start_utc_time_millis;
        assert_eq!(model.format_month_year(september), "September 2024");
        assert_eq!(model.format_date(september, false), "Sep 1, 2024");
        assert_eq!(model.format_date(september, true), "Sunday, September 1, 2024");
    }

    #[test]
    fn an_invalid_date_has_no_calendar_date() {
        assert!(CalendarDate::new(2023, 2, 29).is_none());
        assert!(CalendarDate::new(2024, 2, 29).is_some());
        assert!(CalendarDate::new(2024, 13, 1).is_none());
        assert!(CalendarDate::new(2024, 0, 1).is_none());
        assert!(CalendarDate::new(2024, 4, 31).is_none());
    }

    /// The start of the UTC day `year-month-day`.
    fn millis(year: i32, month: u32, day: u32) -> i64 {
        CalendarDate::new(year, month, day).unwrap().start_of_day_millis()
    }

    /// A state initialised with the given values and a fixed "today", so the tests never read the clock.
    fn state(
        init: DatePickerStateInit,
        today: (i32, u32, u32),
    ) -> DatePickerState {
        DatePickerState::with(
            CalendarLocale::default(),
            DatePickerStateInit {
                today_millis: Some(millis(today.0, today.1, today.2)),
                ..init
            },
        )
    }

    #[test]
    fn the_arrows_step_from_the_month_the_state_is_on_now() {
        let state = state(
            DatePickerStateInit {
                initial_displayed_month_millis: Some(millis(2024, 9, 1)),
                ..Default::default()
            },
            (2024, 9, 5),
        );
        let text = |state: &DatePickerState| {
            state
                .calendar_model()
                .format_month_year(state.displayed_month_millis())
        };
        state.step_displayed_month(-1);
        assert_eq!(text(&state), "August 2024");
        // The second step reads what the first one left rather than the month the picker started on: a step
        // back and then forward returns to September, never to October.
        state.step_displayed_month(1);
        assert_eq!(text(&state), "September 2024");
        state.step_displayed_month(1);
        assert_eq!(text(&state), "October 2024");
    }

    #[test]
    fn the_arrows_stop_at_the_year_range() {
        let state = state(
            DatePickerStateInit {
                initial_displayed_month_millis: Some(millis(2024, 1, 1)),
                year_range: 2024..=2025,
                ..Default::default()
            },
            (2024, 1, 5),
        );
        state.step_displayed_month(-1);
        assert_eq!(
            state
                .calendar_model()
                .format_month_year(state.displayed_month_millis()),
            "January 2024",
            "a step below the range is dropped"
        );
        state.step_displayed_month(24);
        assert_eq!(
            state
                .calendar_model()
                .format_month_year(state.displayed_month_millis()),
            "January 2024",
            "and so is a step above it"
        );
    }

    #[test]
    fn a_selection_outside_the_year_range_is_dropped() {
        let state = state(
            DatePickerStateInit {
                initial_selected_date_millis: Some(millis(1899, 12, 31)),
                ..Default::default()
            },
            (2024, 9, 1),
        );
        assert_eq!(
            state.selected_date_millis(),
            None,
            "an initial selection before the year range is no selection"
        );

        state.set_selected_date_millis(Some(millis(2101, 1, 1)));
        assert_eq!(
            state.selected_date_millis(),
            None,
            "and a write outside the range clears the selection instead of throwing"
        );

        state.set_selected_date_millis(Some(millis(2100, 12, 31)));
        assert_eq!(
            state.selected_date_millis(),
            Some(millis(2100, 12, 31)),
            "the last day of the range is selectable"
        );
    }

    #[test]
    fn a_selection_is_canonicalised_to_the_start_of_its_utc_day() {
        let state = state(DatePickerStateInit::default(), (2024, 9, 1));
        state.set_selected_date_millis(Some(millis(2024, 9, 3) + 12 * 3_600_000 + 345));
        assert_eq!(state.selected_date_millis(), Some(millis(2024, 9, 3)));
    }

    #[test]
    fn the_displayed_month_follows_the_selection_and_otherwise_today() {
        let selected = state(
            DatePickerStateInit {
                initial_selected_date_millis: Some(millis(2024, 3, 15)),
                ..Default::default()
            },
            (2024, 9, 1),
        );
        assert_eq!(
            selected.displayed_month_millis(),
            millis(2024, 3, 1),
            "without a displayed month, the calendar opens on the selection's month"
        );

        let unselected = state(DatePickerStateInit::default(), (2024, 9, 1));
        assert_eq!(unselected.displayed_month_millis(), millis(2024, 9, 1));
    }

    #[test]
    fn an_initial_month_outside_the_year_range_falls_back_to_today() {
        let state = state(
            DatePickerStateInit {
                initial_displayed_month_millis: Some(millis(2200, 1, 1)),
                ..Default::default()
            },
            (2024, 9, 1),
        );
        assert_eq!(state.displayed_month_millis(), millis(2024, 9, 1));
    }

    #[test]
    fn showing_a_month_outside_the_year_range_is_ignored() {
        let state = state(DatePickerStateInit::default(), (2024, 9, 1));
        state.set_displayed_month_millis(millis(2101, 5, 20));
        assert_eq!(
            state.displayed_month_millis(),
            millis(2024, 9, 1),
            "a month past the year range does not move the calendar"
        );

        state.set_displayed_month_millis(millis(2001, 5, 20));
        assert_eq!(
            state.displayed_month_millis(),
            millis(2001, 5, 1),
            "an in-range month snaps to its first day"
        );
    }

    #[test]
    fn switching_mode_pulls_the_calendar_to_the_selected_month() {
        let state = state(
            DatePickerStateInit {
                initial_selected_date_millis: Some(millis(2024, 3, 15)),
                ..Default::default()
            },
            (2024, 9, 1),
        );
        state.set_displayed_month_millis(millis(2024, 12, 1));
        assert_eq!(state.displayed_month_millis(), millis(2024, 12, 1));

        state.set_display_mode(DisplayMode::Input);
        assert_eq!(state.display_mode(), DisplayMode::Input);
        assert_eq!(
            state.displayed_month_millis(),
            millis(2024, 3, 1),
            "the switch snaps the calendar back to the month the selection is in"
        );
    }

    #[test]
    fn a_state_starts_in_picker_mode_over_the_full_default_range() {
        let state = DatePickerState::new(CalendarLocale::default());
        assert_eq!(state.display_mode(), DisplayMode::Picker);
        assert_eq!(state.year_range(), DEFAULT_YEAR_RANGE);
        assert_eq!(state.selected_date_millis(), None);
        assert!(state.selectable_dates().is_selectable_date(0));
        assert!(state.selectable_dates().is_selectable_year(2000));
        assert_eq!(state.locale().first_day_of_week, 7);
    }

    #[test]
    fn the_state_keeps_the_selectable_dates_it_was_given() {
        struct NotSelectable;

        impl SelectableDates for NotSelectable {
            fn is_selectable_date(&self, _utc_time_millis: i64) -> bool {
                false
            }
        }

        let state = state(
            DatePickerStateInit {
                selectable_dates: Arc::new(NotSelectable),
                ..Default::default()
            },
            (2024, 9, 1),
        );
        assert!(!state.selectable_dates().is_selectable_date(millis(2024, 9, 1)));
    }

    #[test]
    fn a_month_grid_is_always_six_rows_of_seven() {
        let model = CalendarModel::new(CalendarLocale::default());
        // February 2021 fits in four rows and August 2020 needs six, but the grid is the same shape either
        // way, because the picker reserves the height (`DatePicker.kt:1859`).
        for (year, month) in [(2021, 2), (2020, 8), (2024, 9)] {
            let grid = MonthGrid::of(model.month_of(year, month), None, 0, &AllDates);
            assert_eq!(grid.cells().len(), 42, "{year}-{month} is six rows of seven");
            assert_eq!(grid.rows().count(), 6);
            assert!(grid.rows().all(|row| row.len() == 7));
        }
    }

    #[test]
    fn the_cells_outside_a_month_hold_the_neighbouring_month() {
        let model = CalendarModel::new(CalendarLocale::default());
        let mut monday_first_locale = CalendarLocale::default();
        monday_first_locale.first_day_of_week = 1;
        let monday_first = CalendarModel::new(monday_first_locale);

        // Sunday-first, 2024-09-01 is a Sunday: no leading cells, and four cells after the 30th carry
        // October's first days — 2024-09-30 is a Monday, so the month ends six cells in.
        let grid = MonthGrid::of(model.month_of(2024, 9), None, 0, &AllDates);
        assert_eq!(grid.cells()[0].day, 1, "no leading cells");
        assert_eq!(grid.cells()[29].day, 30);
        assert!(!grid.cells()[..30].iter().any(|cell| cell.is_outside_month));
        let tail: Vec<u32> = grid.cells()[30..].iter().map(|cell| cell.day).collect();
        assert_eq!(tail, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);
        assert!(grid.cells()[30..].iter().all(|cell| cell.is_outside_month));
        // October's cells carry October's dates, not September's day numbers shifted along.
        assert_eq!(date_of_millis(grid.cells()[30].utc_time_millis).month, 10);

        // Monday-first, the same month starts six cells in, and August's last days fill the gap.
        let grid = MonthGrid::of(monday_first.month_of(2024, 9), None, 0, &AllDates);
        assert!(grid.cells()[..6].iter().all(|cell| cell.is_outside_month));
        let head: Vec<u32> = grid.cells()[..6].iter().map(|cell| cell.day).collect();
        assert_eq!(head, vec![26, 27, 28, 29, 30, 31], "August's last six days");
        assert_eq!(date_of_millis(grid.cells()[0].utc_time_millis).month, 8);
        assert_eq!(grid.cells()[6].day, 1);
        assert_eq!(grid.cells()[35].day, 30);
    }

    #[test]
    fn an_outside_month_cell_is_never_selectable() {
        // `SelectableDates` allows everything, so a disabled outside cell is the grid's own rule and not
        // the caller's: tapping a context day must not move the selection into a month on the other side
        // of the displayed one.
        let model = CalendarModel::new(CalendarLocale::default());
        let grid = MonthGrid::of(model.month_of(2024, 9), None, 0, &AllDates);
        assert!(
            grid.cells()
                .iter()
                .filter(|cell| cell.is_outside_month)
                .all(|cell| !cell.is_enabled)
        );
        assert_eq!(
            grid.cells().iter().filter(|cell| cell.is_enabled).count(),
            30,
            "exactly the displayed month stays enabled"
        );
    }

    #[test]
    fn an_outside_month_cell_still_knows_its_real_date() {
        // The flags describe the date, not the cell's role in the grid: an outside cell can be today or the
        // selected date, and `day_cell` is what decides to draw it as context.
        let model = CalendarModel::new(CalendarLocale::default());
        let month = model.month_of(2024, 9);
        let october_first = month.start_utc_time_millis + 30 * MILLIS_IN_24_HOURS;
        let grid = MonthGrid::of(month, Some(october_first), october_first, &AllDates);
        let cell = grid.cells()[30];
        assert_eq!(cell.day, 1);
        assert!(cell.is_outside_month);
        assert!(cell.is_today, "today is reported wherever it lands");
        assert!(cell.is_selected);
        assert!(!cell.is_enabled);
    }

    #[test]
    fn an_outside_month_label_is_the_plain_role_at_disabled_alpha() {
        // The M3 specs tokens: "Date unselected outside month label text color" #1D1B20, "…text opacity"
        // 0.38. In the baseline light scheme #1D1B20 is onSurface, which is the plain day role.
        let colors = DatePickerColors::from_theme(&ThemeColors::default_light());
        assert_eq!(colors.outside_month_label().a, 97, "0.38 of 255 rounds to 97");
        assert_eq!(
            Color { a: 255, ..colors.outside_month_label() },
            Color { a: 255, ..colors.day_content },
            "same rgb as the plain day role, only the alpha differs"
        );
        assert_eq!(
            colors.outside_month_label(),
            colors.day_label(false, false, false),
            "the same expression as a disabled unselected day, which is what the two tokens add up to"
        );
        assert_eq!(
            colors.outside_month_label().a,
            (DatePickerDefaults::DISABLED_ALPHA * 255.0).round() as u8,
            "and that alpha is the theme's DisabledAlpha (0.38)"
        );
        assert_ne!(
            colors.outside_month_label(),
            colors.day_label(false, true, false),
            "a real day in the month is not dimmed"
        );
    }

    #[test]
    fn a_cell_carries_its_day_and_the_start_of_that_day() {
        let model = CalendarModel::new(CalendarLocale::default());
        let month = model.month_of(2024, 9);
        let grid = MonthGrid::of(month, None, 0, &AllDates);
        let fifteenth = &grid.cells()[14];
        assert_eq!(fifteenth.day, 15);
        assert_eq!(
            fifteenth.utc_time_millis,
            month.start_utc_time_millis + 14 * MILLIS_IN_24_HOURS
        );
        assert_eq!(date_of_millis(fifteenth.utc_time_millis).day, 15);
        assert_eq!(grid.cell(2, 0).unwrap().day, 15, "row two, column zero");
    }

    #[test]
    fn today_and_the_selection_are_flagged_on_their_own_cells() {
        let model = CalendarModel::new(CalendarLocale::default());
        let month = model.month_of(2024, 9);
        let today = month.start_utc_time_millis + 9 * MILLIS_IN_24_HOURS;
        let selected = month.start_utc_time_millis + 24 * MILLIS_IN_24_HOURS;
        let grid = MonthGrid::of(month, Some(selected), today, &AllDates);

        let flagged = |pick: fn(&DayCell) -> bool| {
            grid.cells()
                .iter()
                .filter(|day| pick(day))
                .map(|day| day.day)
                .collect::<Vec<u32>>()
        };
        assert_eq!(flagged(|cell| cell.is_today), vec![10]);
        assert_eq!(flagged(|cell| cell.is_selected), vec![25]);
        assert_eq!(flagged(|cell| cell.is_enabled).len(), 30);
    }

    #[test]
    fn an_unselectable_day_or_year_disables_cells() {
        struct FirstWeekOnly;

        impl SelectableDates for FirstWeekOnly {
            fn is_selectable_date(&self, utc_time_millis: i64) -> bool {
                date_of_millis(utc_time_millis).day <= 7
            }
        }

        struct No2025;

        impl SelectableDates for No2025 {
            fn is_selectable_year(&self, year: i32) -> bool {
                year != 2025
            }
        }

        let model = CalendarModel::new(CalendarLocale::default());
        let grid = MonthGrid::of(model.month_of(2024, 9), None, 0, &FirstWeekOnly);
        let enabled = grid
            .cells()
            .iter()
            .filter(|cell| cell.is_enabled)
            .map(|cell| cell.day)
            .collect::<Vec<u32>>();
        assert_eq!(enabled, (1..=7).collect::<Vec<u32>>());

        // material3: a year that cannot be selected makes every date in it unselectable
        // (`DatePicker.kt:296-297`).
        let grid = MonthGrid::of(model.month_of(2025, 3), None, 0, &No2025);
        assert!(grid.cells().iter().all(|cell| !cell.is_enabled));
    }

    #[test]
    fn a_day_description_names_today_and_the_date() {
        let model = CalendarModel::new(CalendarLocale::default());
        let month = model.month_of(2024, 9);
        let grid = MonthGrid::of(month, None, month.start_utc_time_millis, &AllDates);

        assert_eq!(
            day_content_description(&model, &grid.cells()[0]),
            "Today, Sunday, September 1, 2024"
        );
        assert_eq!(
            day_content_description(&model, &grid.cells()[1]),
            "Monday, September 2, 2024"
        );
        // An outside cell announces its own real date, which is what makes it context rather than noise.
        // Monday-first, September 2024 starts a week in (its 1st is a Sunday), so cell zero is six days
        // earlier — 2024-08-26, a Monday.
        let mut monday_first_locale = CalendarLocale::default();
        monday_first_locale.first_day_of_week = 1;
        let monday_first = CalendarModel::new(monday_first_locale);
        let grid = MonthGrid::of(monday_first.month_of(2024, 9), None, 0, &AllDates);
        assert_eq!(
            day_content_description(&model, &grid.cells()[0]),
            "Monday, August 26, 2024"
        );
    }

    #[test]
    fn a_day_container_is_primary_only_when_the_day_is_selected() {
        let colors = DatePickerColors::from_theme(&ThemeColors::default_light());
        assert_eq!(colors.day_container(true, true), colors.selected_container);
        assert_eq!(
            colors.day_container(true, false),
            Color {
                a: 97,
                ..colors.selected_container
            },
            "a disabled selected day is Primary at DisabledAlpha"
        );
        assert_eq!(colors.day_container(false, true), Color::TRANSPARENT);
        assert_eq!(colors.day_container(false, false), Color::TRANSPARENT);
    }

    #[test]
    fn a_day_label_follows_material3s_precedence() {
        let colors = DatePickerColors::from_theme(&ThemeColors::default_light());
        assert_eq!(colors.day_label(true, true, false), colors.selected_content);
        assert_eq!(colors.day_label(false, true, true), colors.today_content);
        assert_eq!(colors.day_label(false, true, false), colors.day_content);
        assert_eq!(
            colors.day_label(true, false, true),
            Color {
                a: 97,
                ..colors.selected_content
            }
        );
        // material3 falls through to the disabled branch for a day that is both disabled and today
        // (`DatePicker.kt:936-963`), so today's colour does not survive the disable.
        assert_eq!(
            colors.day_label(false, false, true),
            Color {
                a: 97,
                ..colors.day_content
            }
        );
    }

    #[test]
    fn a_year_label_follows_material3s_precedence() {
        let colors = DatePickerColors::from_theme(&ThemeColors::default_light());
        // The signature is `year_label(current_year, selected, enabled)`, the order material3's
        // `yearContentColor(currentYear, selected, enabled)` takes (`DatePicker.kt:1005-1027`).
        assert_eq!(
            colors.year_label(true, false, true),
            colors.today_content,
            "the current year reads as today's label"
        );
        assert_eq!(colors.year_label(false, false, true), colors.year_content);
        assert_eq!(colors.year_label(false, true, true), colors.selected_content);
        assert_eq!(
            colors.year_label(false, true, false),
            Color {
                a: 97,
                ..colors.selected_content
            },
            "a disabled selected year is OnPrimary at DisabledAlpha"
        );
        assert_eq!(
            colors.year_label(true, false, false),
            Color {
                a: 97,
                ..colors.year_content
            },
            "material3 falls through to the disabled branch, so the current year's colour does not survive the \
             disable"
        );
    }

    #[test]
    fn a_year_container_is_primary_only_when_the_year_is_displayed() {
        let colors = DatePickerColors::from_theme(&ThemeColors::default_light());
        assert_eq!(colors.year_container(true, true), colors.selected_container);
        assert_eq!(
            colors.year_container(true, false),
            Color {
                a: 97,
                ..colors.selected_container
            }
        );
        assert_eq!(colors.year_container(false, true), Color::TRANSPARENT);
        assert_eq!(colors.year_container(false, false), Color::TRANSPARENT);
    }

    #[test]
    fn the_year_panel_is_as_tall_as_the_calendar_it_stands_in_for() {
        // material3 takes the panel off `RecommendedSizeForAccessibility * (MaxCalendarRows + 1)` and draws the
        // divider below it inside that height (`DatePicker.kt:1634-1641`), so the panel plus its divider is the
        // weekday row (48) plus the month grid (288) exactly — opening it moves nothing above or below.
        assert_eq!(
            DatePickerDefaults::YEAR_PANEL_HEIGHT + DatePickerDefaults::DIVIDER_THICKNESS,
            DatePickerDefaults::ACCESSIBLE_SIZE + DatePickerDefaults::MONTH_HEIGHT
        );
        assert_eq!(
            DatePickerDefaults::DIVIDER_THICKNESS,
            crate::components::divider::DividerDefaults::thickness()
        );
    }

    #[test]
    fn a_year_keeps_the_displayed_month() {
        let state = state(
            DatePickerStateInit {
                initial_displayed_month_millis: Some(millis(2024, 9, 1)),
                ..Default::default()
            },
            (2024, 9, 5),
        );
        let text = |state: &DatePickerState| {
            state
                .calendar_model()
                .format_month_year(state.displayed_month_millis())
        };
        // material3 scrolls its month list to `(year - yearRange.first) * 12 + displayedMonth.month - 1`, which
        // keeps the month of year (`DatePicker.kt:1643-1652`).
        state.set_displayed_year(2025);
        assert_eq!(text(&state), "September 2025");
        state.set_displayed_year(2023);
        assert_eq!(text(&state), "September 2023");
    }

    #[test]
    fn a_year_outside_the_range_is_ignored() {
        let state = state(
            DatePickerStateInit {
                initial_displayed_month_millis: Some(millis(2024, 9, 1)),
                year_range: 2000..=2100,
                ..Default::default()
            },
            (2024, 9, 5),
        );
        let text = |state: &DatePickerState| {
            state
                .calendar_model()
                .format_month_year(state.displayed_month_millis())
        };
        state.set_displayed_year(1999);
        assert_eq!(text(&state), "September 2024");
        state.set_displayed_year(2101);
        assert_eq!(text(&state), "September 2024");
        state.set_displayed_year(2100);
        assert_eq!(text(&state), "September 2100");
    }

    #[test]
    fn the_year_panel_starts_one_row_above_the_displayed_year() {
        let displayed = state(
            DatePickerStateInit {
                initial_displayed_month_millis: Some(millis(2024, 9, 1)),
                ..Default::default()
            },
            (2024, 9, 5),
        );
        let model = displayed.calendar_model().clone();
        // 2024 is item 124 of the range that starts at 1900 — row 41 of three columns — and material3's
        // `max(0, displayedYear - yearRange.first - YearsInRow)` item index (`DatePicker.kt:2073-2080`) is that
        // row less one.
        assert_eq!(year_panel_first_row(&displayed, &model), 40);
        // A displayed year at the range's start clamps to the first row rather than underflowing.
        let at_range_start = state(
            DatePickerStateInit {
                initial_displayed_month_millis: Some(millis(1900, 1, 1)),
                ..Default::default()
            },
            (2024, 9, 5),
        );
        assert_eq!(year_panel_first_row(&at_range_start, &model), 0);
    }

    #[test]
    fn the_month_arrows_draw_mirrored_chevrons() {
        let left = render_glyph(CHEVRON_LEFT_PATH);
        let right = render_glyph(CHEVRON_RIGHT_PATH);
        let ink = |mask: &[bool]| mask.iter().filter(|on| **on).count();
        let left_ink = ink(&left);
        let right_ink = ink(&right);
        assert!(
            (20..=140).contains(&left_ink),
            "the chevron covers {left_ink} of the 576 pixels in its 24 dp box"
        );
        assert!(
            (left_ink as i32 - right_ink as i32).abs() <= 8,
            "the two chevrons are the same shape ({left_ink} and {right_ink} inked pixels)"
        );

        // The two glyphs are the same shape pointing opposite ways, so the left arrow's ink centre sits left of
        // the right arrow's.
        let centroid_x = |mask: &[bool]| {
            let (sum, count) = mask
                .iter()
                .enumerate()
                .filter(|(_, on)| **on)
                .fold((0usize, 0usize), |(sum, count), (index, _)| {
                    (sum + index % 24, count + 1)
                });
            sum as f32 / count.max(1) as f32
        };
        assert!(
            centroid_x(&left) < centroid_x(&right),
            "the left chevron leans left of the right one ({} vs {})",
            centroid_x(&left),
            centroid_x(&right)
        );

        // And the two are near mirror images of each other (measured: 17 of the 576 pixels differ, so the pair
        // is the same chevron drawn the other way rather than two unrelated glyphs).
        let mirrored_diff = (0..576)
            .filter(|index| {
                let (x, y) = (index % 24, index / 24);
                left[y * 24 + x] != right[y * 24 + 23 - x]
            })
            .count();
        assert!(
            mirrored_diff <= 24,
            "the two chevrons are mirrors ({mirrored_diff} pixels differ)"
        );
        let (_, left_top, _, left_bottom) = glyph_bounds(&left);
        let (_, right_top, _, right_bottom) = glyph_bounds(&right);
        assert!(
            left_top.abs_diff(right_top) <= 1 && left_bottom.abs_diff(right_bottom) <= 1,
            "both chevrons span the same rows ({left_top}..{left_bottom} vs {right_top}..{right_bottom})"
        );
    }

    /// The M3 specs page draws the two variants differently and this pins it: the DOCKED anatomy lists
    /// "Outside month date" among the grid's states, the MODAL anatomy has no such entry, and material3
    /// agrees with the modal one (it composes a `Spacer`, `DatePicker.kt:1870-1890`). So a docked page
    /// fills all 42 slots and a modal page draws only its own month.
    ///
    /// Counted by what is INERT rather than by how many labels there are. The grid is a paged `LazyRow`
    /// now (material3's `HorizontalMonthsList`), so a frame composes a couple of dozen pages of mixed
    /// lengths and "42 a page against 30" is not a number the tree can be asked for. What it can be
    /// asked is the difference that does not depend on the page count: an outside-month cell is never
    /// enabled, so its `Surface` carries no `click`, while every real day of the month does.
    #[test]
    fn the_docked_grid_draws_the_neighbouring_months_days_and_the_modal_one_does_not() {
        let model = CalendarModel::new(CalendarLocale::default());
        let month = model.month_of(2026, 9);

        /// `(choosable day cells, inert day cells)`. A day label is one numeric text in 1..=31; it is a
        /// leaf, so its `Surface` is two levels up — past the centring `Stack` — and that `Surface` is
        /// clickable exactly when the day can be chosen.
        fn day_cells(composer: &crate::runtime::composer::Composer) -> (usize, usize) {
            let nodes = composer.arena_nodes();
            // `LayoutNode` stores children but not a parent, so invert it to walk up from a leaf.
            let mut parent = vec![usize::MAX; nodes.len()];
            for (idx, node) in nodes.iter().enumerate() {
                for &child in &node.children {
                    if child < parent.len() {
                        parent[child] = idx;
                    }
                }
            }
            let clickable = |idx: usize| {
                nodes[idx].modifier.elements().iter().any(|e| {
                    matches!(e, crate::modifier::ModifierElement::Clickable { .. })
                })
            };
            let day_label = |idx: usize| {
                nodes[idx].modifier.elements().iter().any(|e| match e {
                    crate::modifier::ModifierElement::TextContent { content, .. } => {
                        content.parse::<u32>().is_ok_and(|v| (1..=31).contains(&v))
                    }
                    _ => false,
                })
            };
            let (mut choosable, mut inert) = (0, 0);
            for idx in 0..nodes.len() {
                if !day_label(idx) {
                    continue;
                }
                let stack = parent[idx];
                let surface = if stack == usize::MAX { usize::MAX } else { parent[stack] };
                if surface == usize::MAX {
                    continue;
                }
                if clickable(surface) {
                    choosable += 1;
                } else {
                    inert += 1;
                }
            }
            (choosable, inert)
        }

        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            let mut state = DatePickerState::new(CalendarLocale::default());
            state.set_displayed_month_millis(month.start_utc_time_millis);
            DockedDatePicker::new(state).build(ctx);
        });
        let (docked_choosable, docked_inert) = day_cells(&composer);

        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            let mut state = DatePickerState::new(CalendarLocale::default());
            state.set_displayed_month_millis(month.start_utc_time_millis);
            DatePicker::new(state).build(ctx);
        });
        let (modal_choosable, modal_inert) = day_cells(&composer);

        assert!(
            docked_inert > 0,
            "the docked grid composes inert outside-month cells, found none"
        );
        assert_eq!(
            modal_inert, 0,
            "the modal grid composes an inert day cell, so it is drawing an outside-month cell"
        );
        assert_eq!(
            docked_choosable, modal_choosable,
            "both compose the same pages, so both draw the same number of choosable days"
        );
    }

    /// The 24x24 ink mask of a glyph drawn through the real `Icon` pipeline (node, then render, then pixels),
    /// the measurement `the_published_arrow_data_draws_the_same_arrow` makes for the dropdown arrow
    /// (`winia/src/ui/overlay.rs:1970`).
    fn render_glyph(data: &str) -> Vec<bool> {
        render_glyph_in(data, crate::layout::LayoutDirection::Ltr)
    }

    /// [`render_glyph`] under an ambient `direction`, so the RTL run exercises the real path — the theme's
    /// direction reaching the node's `layout_direction` and `draw_icon`'s mirror test — rather than a
    /// modifier bolted on for the test. `auto_mirror` is on, which is what the arrow tests are about.
    fn render_glyph_in(data: &str, direction: crate::layout::LayoutDirection) -> Vec<bool> {
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            WiniaTheme::with_theme_and_direction(ThemeColors::default_light(), direction, ctx, |ctx| {
                Icon::svg_path(data)
                    .tint(Color::BLACK)
                    .size(24.0)
                    .auto_mirror(true)
                    .build(ctx);
            });
        });
        composer.layout(crate::layout::Constraints::new(0.0, 24.0, 0.0, 24.0));
        let mut surface = skia_safe::surfaces::raster_n32_premul((24, 24)).expect("surface");
        let canvas = surface.canvas();
        canvas.clear(skia_safe::Color::WHITE);
        let root = composer.layout_root_idx().expect("root");
        crate::render::render(composer.arena_nodes(), root, canvas);
        let pixels = surface.peek_pixels().expect("pixmap");
        let px: &[[u8; 4]] = pixels.pixels::<[u8; 4]>().expect("pixels");
        px.iter().map(|p| p[0] < 128).collect()
    }

    #[test]
    fn the_navigation_arrows_flip_their_artwork_under_rtl() {
        // `Row` already carries the previous arrow to the RIGHT edge in RTL, so the artwork has to flip
        // with it or both arrows point inward — the failure `step_arrow` guards with `auto_mirror`.
        // Inked pixels are compared against the OTHER glyph rather than through an ink centroid: the
        // chevron's centroid sits at ~11.5 either way round, so a centroid cannot see the flip at all
        // (measured: 11.516 under LTR, 11.484 under RTL — a genuine mirror that a centroid test misses).
        let previous_ltr = render_glyph(CHEVRON_LEFT_PATH);
        let previous_rtl = render_glyph_in(CHEVRON_LEFT_PATH, crate::layout::LayoutDirection::Rtl);
        let next_ltr = render_glyph(CHEVRON_RIGHT_PATH);
        let ink_diff = |a: &[bool], b: &[bool]| a.iter().zip(b.iter()).filter(|(x, y)| x != y).count();

        // Under RTL the previous arrow lands on the next chevron's pixels — 17 of 576 measured, the
        // same figure `the_month_arrows_draw_mirrored_chevrons` records for this pair of paths.
        assert!(
            ink_diff(&previous_rtl, &next_ltr) <= 24,
            "under RTL the previous chevron draws as the next one ({} pixels differ)",
            ink_diff(&previous_rtl, &next_ltr)
        );

        // And under LTR it does not, so the assertion above is the mirror doing work rather than the
        // two constants happening to rasterize alike (41 pixels measured).
        assert!(
            ink_diff(&previous_ltr, &next_ltr) > 24,
            "under LTR the previous chevron stays its own glyph (only {} pixels differ)",
            ink_diff(&previous_ltr, &next_ltr)
        );
    }

    /// The real guard on `step_arrow`: composing the picker has to mark its chevrons auto-mirrored. The
    /// pixel test above proves the pipeline flips an icon that ASKS to be flipped; this one proves the
    /// picker asks — drop the `auto_mirror(true)` from `step_arrow` and it goes red.
    #[test]
    fn the_picker_composes_its_chevrons_as_auto_mirrored() {
        use crate::modifier::ModifierElement;
        use crate::graphics::IconSource;

        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            let state = DatePickerState::new(CalendarLocale::default());
            DockedDatePicker::new(state).build(ctx);
        });

        let chevrons: Vec<(String, bool)> = composer
            .arena_nodes()
            .iter()
            .flat_map(|node| node.modifier.elements().to_vec())
            .filter_map(|element| match element {
                ModifierElement::DrawIcon { spec } => match &spec.source {
                    IconSource::SvgPath { data, .. }
                        if data.as_ref() == CHEVRON_LEFT_PATH || data.as_ref() == CHEVRON_RIGHT_PATH =>
                    {
                        Some((data.to_string(), spec.auto_mirror))
                    }
                    _ => None,
                },
                _ => None,
            })
            .collect();

        assert!(
            !chevrons.is_empty(),
            "the docked picker composes no chevron icons, so this test would pass vacuously"
        );
        for (data, auto_mirror) in &chevrons {
            assert!(
                *auto_mirror,
                "the chevron `{}` is not marked auto_mirror, so RTL leaves it pointing inward",
                &data[..data.len().min(24)]
            );
        }
    }

    /// Compose the modal picker with a runtime entered, which `LaunchedEffect` needs to spawn its
    /// task into. The task is never driven here — nothing in these assertions depends on the effect
    /// having run, and driving it would only race the assertions.
    fn compose_picker(state: &DatePickerState) -> crate::runtime::composer::Composer {
        compose_picker_with(|ctx| DatePicker::new(state.clone()).build(ctx))
    }

    /// Compose `content` inside a runtime, the way the app's own frame loop does.
    ///
    /// The entry field spawns a `LaunchedEffect` to ask for focus once it is showing, and spawning
    /// needs an entered runtime; a test that composes the picker directly has to provide one. The
    /// runtime is entered but never driven: what these helpers assert is what got composed, and
    /// letting the spawned task run would race that against the assertions.
    fn compose_picker_with(content: impl FnOnce(&mut ComposeCtx)) -> crate::runtime::composer::Composer {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let _guard = rt.enter();
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(content);
        composer
    }

    /// The composed text the modal picker puts on screen, in the order it composed it.
    fn composed_texts(state: &DatePickerState) -> Vec<String> {
        composed_texts_with_title(state, None)
    }

    /// The texts a picker composes when its title is set explicitly, the way a caller's own title
    /// reaches the header. `None` leaves the default in place.
    fn composed_texts_with_title(state: &DatePickerState, title: Option<&str>) -> Vec<String> {
        let state = state.clone();
        let title = title.map(str::to_string);
        let composer = compose_picker_with(|ctx| {
            let mut picker = DatePicker::new(state);
            if let Some(title) = title {
                picker = picker.title(Some(title));
            }
            picker.build(ctx);
        });
        let mut out = Vec::new();
        for node in composer.arena_nodes() {
            for element in node.modifier.elements() {
                if let crate::modifier::ModifierElement::TextContent { content, .. } = element {
                    out.push(content.clone());
                }
            }
        }
        out
    }

    /// The icon paths the modal picker composes.
    fn composed_icon_paths(state: &DatePickerState) -> Vec<String> {
        let composer = compose_picker(state);
        let mut out = Vec::new();
        for node in composer.arena_nodes() {
            for element in node.modifier.elements() {
                if let crate::modifier::ModifierElement::DrawIcon { spec, .. } = element {
                    if let crate::graphics::IconSource::SvgPath { data, .. } = &spec.source {
                        out.push(data.to_string());
                    }
                }
            }
        }
        out
    }

    /// `DisplayMode::Input` has to change what the picker composes. This is the guard on the
    /// original defect: `set_display_mode(DisplayMode::Input)` stored a value, and the whole crate
    /// had nothing that read it, so the picker stayed a calendar and the mode was a silent no-op.
    /// Both directions are asserted because either half alone would pass a picker that simply
    /// ignored the state and always drew the calendar.
    #[test]
    fn the_display_mode_decides_whether_the_picker_composes_a_calendar_or_a_field() {
        let state = picker();
        assert_eq!(state.display_mode(), DisplayMode::Picker);

        let calendar = composed_texts(&state);
        assert!(
            !calendar.is_empty(),
            "the picker composes no text at all, so this test would pass vacuously"
        );
        assert!(
            composed_icon_paths(&state).iter().any(|path| path == EDIT_PATH),
            "the calendar half composes no edit icon for the mode toggle, so the toggle is unreachable"
        );

        state.set_display_mode(DisplayMode::Input);
        let input = composed_texts(&state);
        assert!(
            input.iter().any(|text| text == DATE_INPUT_HEADLINE),
            "the input half has no {DATE_INPUT_HEADLINE:?} headline, got {input:?}"
        );
        assert!(
            input.iter().any(|text| text == DATE_INPUT_LABEL),
            "the input half has no {DATE_INPUT_LABEL:?} field label, got {input:?}"
        );
        assert!(
            composed_icon_paths(&state).iter().any(|path| path == DATE_RANGE_PATH),
            "the input half still offers the edit icon, so the toggle cannot go back"
        );

        state.set_display_mode(DisplayMode::Picker);
        assert_eq!(composed_texts(&state), calendar, "the mode did not go back to the calendar");
    }

    /// The headline names the mode's own wording when nothing is selected, and says the same thing
    /// either way once something is — material3 changes the word, not the fact
    /// (`DatePickerHeadline`, `DatePicker.kt:701-717`).
    #[test]
    fn the_headline_names_the_mode_when_nothing_is_selected() {
        let state = picker();
        assert!(composed_texts(&state).contains(&DatePickerDefaults::HEADLINE.to_string()));

        state.set_display_mode(DisplayMode::Input);
        assert!(composed_texts(&state).contains(&DATE_INPUT_HEADLINE.to_string()));

        let selected = CalendarDate::new(2024, 3, 1).expect("2024-03-01 is a date");
        state.set_selected_date_millis(Some(selected.start_of_day_millis()));
        let headline = CalendarModel::new(CalendarLocale::default())
            .format_date(selected.start_of_day_millis(), false);
        let texts = composed_texts(&state);
        assert!(
            texts.contains(&headline),
            "a selection should name the date in either mode, got {texts:?}"
        );
        assert!(
            !texts.contains(&DATE_INPUT_HEADLINE.to_string()),
            "the mode's own wording should be replaced once something is entered"
        );
    }

    /// The title names the mode as well as the headline does: a field that asks for a date is not
    /// headed "Select date" (`DatePicker.kt:654`). A title the caller supplied is left alone —
    /// only the default follows the mode.
    #[test]
    fn the_default_title_names_the_mode_and_a_supplied_one_does_not() {
        let state = picker();
        assert!(composed_texts(&state).contains(&DatePickerDefaults::TITLE.to_string()));
        assert!(!composed_texts(&state).contains(&DatePickerDefaults::INPUT_TITLE.to_string()));

        state.set_display_mode(DisplayMode::Input);
        let texts = composed_texts(&state);
        assert!(
            texts.contains(&DatePickerDefaults::INPUT_TITLE.to_string()),
            "the entry field should be headed by the input title, got {texts:?}"
        );
        assert!(
            !texts.contains(&DatePickerDefaults::TITLE.to_string()),
            "the calendar title should give way to the input title"
        );

        state.set_display_mode(DisplayMode::Picker);
        let texts = composed_texts(&state);
        assert!(texts.contains(&DatePickerDefaults::TITLE.to_string()));
        assert!(!texts.contains(&DatePickerDefaults::INPUT_TITLE.to_string()));

        // A caller's own title stands in both modes.
        let custom = "Pick a day off";
        let mut owned = state.clone();
        owned.set_display_mode(DisplayMode::Input);
        let texts = composed_texts_with_title(&owned, Some(custom));
        assert!(
            texts.contains(&custom.to_string()),
            "a supplied title should survive the mode switch, got {texts:?}"
        );
        assert!(!texts.contains(&DatePickerDefaults::INPUT_TITLE.to_string()));
    }

    /// The content descriptions the modal picker composes.
    fn composed_content_descriptions(state: &DatePickerState) -> Vec<String> {
        let composer = compose_picker(state);
        let mut out = Vec::new();
        for node in composer.arena_nodes() {
            for element in node.modifier.elements() {
                if let crate::modifier::ModifierElement::Semantics(config) = element {
                    if let Some(description) = config.content_description_value() {
                        out.push(description.to_string());
                    }
                }
            }
        }
        out
    }

    /// The field names its shape to whoever is not looking at it: the label carries the locale's
    /// pattern in its description, so a reader hears what to type before anything is typed
    /// (`DateInput.kt:93-98`).
    ///
    /// This reads the label's description rather than the placeholder because the placeholder is
    /// focus-gated — an unfocused field that has a label keeps the label sitting in the input slot
    /// instead (`text_field.rs:1094-1101`) — and a compose-only test has no focus to give it.
    #[test]
    fn the_input_field_names_the_locales_pattern_on_its_label() {
        let state = DatePickerState::new(locale_with_input_format("dd.MM.yyyy"));
        state.set_display_mode(DisplayMode::Input);
        let descriptions = composed_content_descriptions(&state);
        assert!(
            descriptions.iter().any(|d| d == "Date, DD.MM.YYYY"),
            "the field label does not name the locale's pattern, got {descriptions:?}"
        );
    }

    /// The half-open pixel box an ink mask covers, `(left, top, right, bottom)`, or `(0, 0, 0, 0)` when nothing
    /// was drawn.
    fn glyph_bounds(mask: &[bool]) -> (usize, usize, usize, usize) {
        let (mut left, mut top, mut right, mut bottom) = (usize::MAX, usize::MAX, 0usize, 0usize);
        for (index, on) in mask.iter().enumerate() {
            if !on {
                continue;
            }
            let (x, y) = (index % 24, index / 24);
            left = left.min(x);
            top = top.min(y);
            right = right.max(x + 1);
            bottom = bottom.max(y + 1);
        }
        if left == usize::MAX {
            return (0, 0, 0, 0);
        }
        (left, top, right, bottom)
    }
