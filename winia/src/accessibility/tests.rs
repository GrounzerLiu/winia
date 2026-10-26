    use super::*;
    use crate::semantics::SemanticsState;

    fn element(
        role: Option<SemanticsRole>,
        clickable: bool,
        state: SemanticsState,
    ) -> ElementData {
        ElementData {
            node_id: 1,
            role,
            name: Some("x".to_string()),
            state,
            clickable,
            focused: false,
            live_region: crate::semantics::LiveRegionMode::Off,
            collection: false,
            collection_index: None,
            scroll_owner: None,
        }
    }

    /// The lazy list's shape: a collection that offers ScrollToIndex.
    fn collection() -> ElementData {
        ElementData {
            collection: true,
            scroll_owner: None,
            ..element(None, false, SemanticsState::new())
        }
    }

    /// A row of that list: a position in the collection, whose ancestor offers the jump.
    fn item(index: usize, owner: u64) -> ElementData {
        ElementData {
            collection_index: Some(index),
            scroll_owner: Some(owner),
            ..element(None, false, SemanticsState::new())
        }
    }

    #[test]
    fn every_role_maps_to_a_control_type() {
        // The mapping is the contract a client reads: getting a role wrong makes a screen reader
        // announce the wrong kind of control, which is worse than announcing nothing.
        let cases = [
            (SemanticsRole::Button, UIA_ButtonControlTypeId),
            (SemanticsRole::Checkbox, UIA_CheckBoxControlTypeId),
            // UIA has no switch type; a switch is a toggleable control, so CheckBox it is.
            (SemanticsRole::Switch, UIA_CheckBoxControlTypeId),
            (SemanticsRole::RadioButton, UIA_RadioButtonControlTypeId),
            (SemanticsRole::Tab, UIA_TabItemControlTypeId),
            (SemanticsRole::Image, UIA_ImageControlTypeId),
            (SemanticsRole::ProgressBar, UIA_ProgressBarControlTypeId),
            (SemanticsRole::Dialog, UIA_PaneControlTypeId),
            (SemanticsRole::DropdownList, UIA_ListControlTypeId),
        ];
        for (role, expected) in cases {
            let element = element(Some(role), false, SemanticsState::new());
            assert_eq!(control_type_for(&element).0, expected.0, "{role:?}");
        }
        // A named element with no role is text — `Text` and `RichText` compose exactly that.
        let plain = element(None, false, SemanticsState::new());
        assert_eq!(control_type_for(&plain).0, UIA_TextControlTypeId.0);
        // A collection is a LIST and its rows are LIST ITEMS, whatever role they carry: a reader that
        // hears "text" for every row never gets to say "row 3 of 200".
        assert_eq!(control_type_for(&collection()).0, UIA_ListControlTypeId.0);
        assert_eq!(control_type_for(&item(3, 7)).0, UIA_ListItemControlTypeId.0);
    }

    #[test]
    fn a_click_target_offers_invoke() {
        let button = element(Some(SemanticsRole::Button), true, SemanticsState::new());
        assert!(supports_pattern(&button, UIA_InvokePatternId));
        // A plain button has no toggle or selection state, so it offers neither.
        assert!(!supports_pattern(&button, UIA_TogglePatternId));
        assert!(!supports_pattern(&button, UIA_SelectionItemPatternId));
    }

    #[test]
    fn a_clickable_checkbox_offers_invoke_and_toggle() {
        // The case that made "one pattern per element" wrong: an ENABLED checkbox is clickable AND
        // checked, and a client may address it with either pattern.
        let checkbox = element(
            Some(SemanticsRole::Checkbox),
            true,
            SemanticsState::new().checked_bool(true),
        );
        assert!(supports_pattern(&checkbox, UIA_InvokePatternId));
        assert!(supports_pattern(&checkbox, UIA_TogglePatternId));
        assert!(!supports_pattern(&checkbox, UIA_SelectionItemPatternId));
    }

    #[test]
    fn a_disabled_checkbox_still_offers_toggle() {
        // Its state is readable and a screen reader must be able to announce it; `IsEnabled` is what
        // tells the client not to act on it.
        let disabled = element(
            Some(SemanticsRole::Checkbox),
            false,
            SemanticsState::new().checked(crate::selection::ToggleableState::Indeterminate),
        );
        assert!(!supports_pattern(&disabled, UIA_InvokePatternId));
        assert!(supports_pattern(&disabled, UIA_TogglePatternId));
    }

    #[test]
    fn a_selected_control_offers_selection_item() {
        let radio = element(
            Some(SemanticsRole::RadioButton),
            true,
            SemanticsState::new().selected(true),
        );
        assert!(supports_pattern(&radio, UIA_SelectionItemPatternId));
        assert!(supports_pattern(&radio, UIA_InvokePatternId));
        assert!(!supports_pattern(&radio, UIA_TogglePatternId));
    }

    #[test]
    fn a_progress_bar_offers_range_value_and_its_number() {
        let bar = element(
            Some(SemanticsRole::ProgressBar),
            false,
            SemanticsState::new().progress(30.0, 0.0, 100.0),
        );
        assert!(supports_pattern(&bar, UIA_RangeValuePatternId), "a value to report means the pattern");
        assert!(!supports_pattern(&bar, UIA_InvokePatternId), "a progress bar is not clickable");

        // And the number itself reads back through the interface a client uses.
        let provider = RangeValuePattern { value: 30.0, min: 0.0, max: 100.0 };
        let provider: IRangeValueProvider = provider.into();
        // SAFETY: COM calls into the implementation above; the provider outlives them.
        unsafe {
            assert_eq!(provider.Value().expect("a value"), 30.0);
            assert_eq!(provider.Minimum().expect("a min"), 0.0);
            assert_eq!(provider.Maximum().expect("a max"), 100.0);
            assert!(provider.IsReadOnly().expect("a flag").as_bool(), "progress is not settable");
            assert!(provider.SetValue(50.0).is_err(), "and refuses rather than pretending");
        }
    }

    #[test]
    fn an_indeterminate_progress_bar_reports_no_value() {
        // A spinner has no value; a reader is told it is a progress bar and nothing more. Reporting
        // zero would be a lie — the difference between "0 percent" and "in progress".
        let spinner = element(Some(SemanticsRole::ProgressBar), false, SemanticsState::new());
        assert!(!supports_pattern(&spinner, UIA_RangeValuePatternId));
        assert_eq!(spinner.state.progress_value(), None);
    }

    #[test]
    fn an_element_with_no_action_offers_no_pattern() {
        // A label: name and role, neither clickable nor toggleable. Advertising a pattern here is what
        // made clients ask for one and get nothing back.
        let label = element(None, false, SemanticsState::new());
        assert!(!supports_pattern(&label, UIA_InvokePatternId));
        assert!(!supports_pattern(&label, UIA_TogglePatternId));
        assert!(!supports_pattern(&label, UIA_SelectionItemPatternId));
    }

    #[test]
    fn toggle_state_carries_all_three_values() {
        // The tri-state checkbox depends on this: "indeterminate" must not collapse into "off".
        let state_of = |state| {
            let pattern = TogglePattern {
                window_id: 0,
                node_id: 0,
                state,
            };
            let provider: IToggleProvider = pattern.into();
            // SAFETY: the call goes through COM into the implementation above; the provider is alive for
            // the call and owns nothing the caller has to free.
            unsafe { provider.ToggleState().expect("a state").0 }
        };
        assert_eq!(state_of(ToggleableState::On), ToggleState_On.0);
        assert_eq!(state_of(ToggleableState::Off), ToggleState_Off.0);
        assert_eq!(
            state_of(ToggleableState::Indeterminate),
            ToggleState_Indeterminate.0
        );
    }

    #[test]
    fn selection_item_reports_its_selection_and_refuses_multi_select() {
        let selected = SelectionItemPattern {
            window_id: 0,
            node_id: 0,
            selected: true,
        };
        let provider: ISelectionItemProvider = selected.into();
        // SAFETY: COM calls into the implementation above; the provider outlives each call.
        unsafe {
            assert_eq!(provider.IsSelected().expect("a value").as_bool(), true);
            // Single-choice controls: "add to the selection" is refused rather than faked.
            assert!(provider.AddToSelection().is_err());
            assert!(provider.RemoveFromSelection().is_err());
        }
    }

    #[test]
    fn an_item_offers_scroll_item_and_scrolls_into_view() {
        let row = item(42, 7);
        assert!(
            supports_pattern(&row, UIA_ScrollItemPatternId),
            "an item of a collection that offers the jump offers ScrollItem"
        );
        // Half a declaration is not enough: no position, or no collection to move, means a pattern
        // that would have nowhere to send the request.
        let no_index = ElementData { collection_index: None, ..row.clone() };
        assert!(!supports_pattern(&no_index, UIA_ScrollItemPatternId));
        let no_owner = ElementData { scroll_owner: None, ..row.clone() };
        assert!(!supports_pattern(&no_owner, UIA_ScrollItemPatternId));
        // The collection itself is not an item.
        assert!(!supports_pattern(&collection(), UIA_ScrollItemPatternId));

        // Invoking the pattern queues the jump for the frame loop — it must not run application code.
        crate::semantics::take_actions(7777);
        let pattern: IScrollItemProvider = ScrollItemPattern {
            window_id: 7777,
            owner: 7,
            index: 42,
        }
        .into();
        // SAFETY: COM calls into the implementation above; the provider outlives the call.
        unsafe { pattern.ScrollIntoView().expect("queued") };
        assert_eq!(
            crate::semantics::take_actions(7777),
            vec![crate::semantics::UiAction::ScrollToIndex { node: Some(7), index: 42 }],
            "the collection is named, so a window with several lists moves the right one"
        );
    }

    #[test]
    fn an_action_from_a_bridge_is_queued_for_this_window_only() {
        // The provider must not run application code, so what it does is queue — and the queue has to
        // keep windows apart, or a dialog's action would land on the page behind it.
        crate::semantics::request_action(4242, crate::semantics::UiAction::Invoke(7));
        crate::semantics::request_action(4243, crate::semantics::UiAction::Focus(9));
        assert!(crate::semantics::has_actions());
        assert_eq!(
            crate::semantics::take_actions(4242),
            vec![crate::semantics::UiAction::Invoke(7)]
        );
        assert_eq!(
            crate::semantics::take_actions(4243),
            vec![crate::semantics::UiAction::Focus(9)]
        );
        assert!(!crate::semantics::has_actions(), "the queue is drained");
        assert!(crate::semantics::take_actions(4242).is_empty());
    }
