use super::*;

/// The order the input field's three fields appear in, read off the pattern
/// (`DateInputFormat`'s pattern is always exactly one `yyyy`, one `MM` and one
/// `dd`, so the order is the only thing the locale actually decides).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DateInputFieldOrder {
    /// `dd/MM/yyyy` — the pattern en-GB and most of Europe use.
    DayMonthYear,
    /// `MM/dd/yyyy` — the pattern en-US uses.
    MonthDayYear,
    /// `yyyy/MM/dd` — the pattern ja-JP and zh-CN use.
    YearMonthDay,
}

/// How the input field writes and reads a date: a pattern over `d`, `M` and `y` plus the one
/// character separating them (`DateInputFormat`, `CalendarModel.kt:277`).
///
/// material3 derives this from the platform locale's best date-time pattern for the `yMd`
/// skeleton. winia has no locale database, so like [`CalendarLocale`]'s names this is data —
/// [`CalendarLocale::default`] is en-US (`MM/dd/yyyy`), and a caller wanting another ordering
/// builds one with [`DateInputFormat::from_pattern`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DateInputFormat {
    pattern_with_delimiters: String,
    delimiter: char,
}

impl Default for DateInputFormat {
    fn default() -> Self {
        // The en-US best pattern for the `yMd` skeleton: `MM/dd/yyyy`.
        Self::from_pattern("MM/dd/yyyy").expect("MM/dd/yyyy is a valid input pattern")
    }
}

impl DateInputFormat {
    /// Build a format from a locale pattern such as `M/d/yyyy`, `dd.MM.yyyy` or `yyyy/MM/dd`.
    ///
    /// Cleans the pattern the way material3 does (`datePatternAsInputFormat`,
    /// `CalendarModel.kt:296-315`): drop everything that is not a `d`, `M` or `y` field or a
    /// `/ - .` delimiter, widen each field to two digits for day and month and four for year, and
    /// take the delimiter from the first separator that survives.
    ///
    /// A run of the same letter is **one** field however wide the locale wrote it, so this takes a
    /// locale pattern (`M/d/yyyy`) and an already-normalized one (`MM/dd/yyyy`) alike — a
    /// deviation from Compose's regex, whose `d{1,2}` counts characters and would read the
    /// normalized form as two day fields. Returns `None` when what is left is not exactly one day,
    /// one month and one year, or when no delimiter is present: a pattern without a separator has
    /// nowhere to put one, and the visual transformation keys off the two delimiter offsets.
    ///
    /// Not ported: Compose's `.replace("My", "M/y")` for the Kako locale, whose pattern spells one
    /// combined year-month field. winia has no locale database, so no locale needs it yet.
    pub fn from_pattern(pattern: &str) -> Option<Self> {
        const DELIMITERS: [char; 3] = ['/', '-', '.'];
        let delimiter = pattern.chars().find(|c| DELIMITERS.contains(c))?;

        let mut pattern_with_delimiters = String::with_capacity(10);
        let mut fields: Vec<char> = Vec::with_capacity(3);
        let mut chars = pattern.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                'd' | 'M' | 'y' => {
                    while chars.peek() == Some(&c) {
                        chars.next();
                    }
                    fields.push(c);
                    pattern_with_delimiters.push_str(match c {
                        'd' => "dd",
                        'M' => "MM",
                        _ => "yyyy",
                    });
                }
                c if DELIMITERS.contains(&c) => pattern_with_delimiters.push(c),
                _ => {}
            }
        }
        let names_each_field_once = fields.len() == 3
            && fields.contains(&'d')
            && fields.contains(&'M')
            && fields.contains(&'y');
        if !names_each_field_once {
            return None;
        }
        Some(Self { pattern_with_delimiters, delimiter })
    }

    /// The pattern as the field displays it in its placeholder, `MM/dd/yyyy`.
    pub fn pattern_with_delimiters(&self) -> &str {
        &self.pattern_with_delimiters
    }

    /// The separator, `/` or `-` or `.`.
    pub fn delimiter(&self) -> char {
        self.delimiter
    }

    /// The same pattern with the separators dropped, `MMddyyyy`. This is what the field
    /// actually holds; the separators exist only in the transformed text
    /// (`DateInputFormat.patternWithoutDelimiters`, `CalendarModel.kt:279`).
    pub fn pattern_without_delimiters(&self) -> String {
        self.pattern_with_delimiters.replace(self.delimiter, "")
    }

    /// The digit count a full entry has, always 8.
    pub fn pattern_length(&self) -> usize {
        self.pattern_without_delimiters().len()
    }

    /// Which field comes first, taken from where each letter sits in the pattern.
    pub fn field_order(&self) -> DateInputFieldOrder {
        let pattern = self.pattern_without_delimiters();
        let day = pattern.find('d').unwrap_or(0);
        let month = pattern.find('M').unwrap_or(0);
        if day < month {
            DateInputFieldOrder::DayMonthYear
        } else if pattern.find('y').unwrap_or(0) < day {
            DateInputFieldOrder::YearMonthDay
        } else {
            DateInputFieldOrder::MonthDayYear
        }
    }

    /// The index of the first delimiter in [`DateInputFormat::pattern_with_delimiters`], which
    /// the visual transformation uses to decide where the first separator goes.
    pub fn first_delimiter_offset(&self) -> usize {
        self.pattern_with_delimiters
            .find(self.delimiter)
            .expect("a format is built from a pattern that had a delimiter")
    }

    /// The index of the last delimiter in [`DateInputFormat::pattern_with_delimiters`].
    pub fn last_delimiter_offset(&self) -> usize {
        self.pattern_with_delimiters
            .rfind(self.delimiter)
            .expect("a format is built from a pattern that had a delimiter")
    }
}

/// Writes the format's delimiters into a date entry as it is typed, so the field shows
/// `03/01/2024` while what it holds is the eight digits `03012024`
/// (`DateVisualTransformation`, `DateInput.kt:392-439`).
///
/// The delimiters are inserted as the user types and are never part of the stored text, which is
/// why the offsets have to be translated both ways: a caret sitting after the fifth displayed
/// character is after the fourth typed one.
#[derive(Clone, Debug)]
pub struct DateVisualTransformation {
    /// Where the first delimiter goes in the delimited pattern, `MM/dd/yyyy` → 2.
    first_delimiter_offset: usize,
    /// Where the second goes, `MM/dd/yyyy` → 5.
    second_delimiter_offset: usize,
    /// The digit count, always 8.
    date_format_length: usize,
    delimiter: char,
}

impl DateVisualTransformation {
    /// A transformation for `format`.
    pub fn new(format: &DateInputFormat) -> Self {
        Self {
            first_delimiter_offset: format.first_delimiter_offset(),
            second_delimiter_offset: format.last_delimiter_offset(),
            date_format_length: format.pattern_length(),
            delimiter: format.delimiter(),
        }
    }
}

impl VisualTransformation for DateVisualTransformation {
    fn filter(&self, text: &str) -> TransformedText {
        // A longer entry is cut at a full field's width, which is the same width the field's
        // `onValueChange` accepts; the two never disagree, so this is belt and braces.
        let trimmed = if text.len() > self.date_format_length {
            &text[..self.date_format_length]
        } else {
            text
        };
        let mut transformed = String::with_capacity(self.date_format_length + 2);
        for (index, c) in trimmed.chars().enumerate() {
            transformed.push(c);
            if index + 1 == self.first_delimiter_offset
                || index + 2 == self.second_delimiter_offset
            {
                transformed.push(self.delimiter);
            }
        }
        TransformedText {
            text: transformed,
            offset_mapping: Arc::new(DateOffsetMapping {
                first_delimiter_offset: self.first_delimiter_offset,
                second_delimiter_offset: self.second_delimiter_offset,
                date_format_length: self.date_format_length,
            }),
        }
    }
}

/// Moves a caret between the digits a date entry holds and the digits it shows
/// (`DateVisualTransformation`'s offset translator, `DateInput.kt:401-421`).
#[derive(Clone, Copy, Debug)]
struct DateOffsetMapping {
    first_delimiter_offset: usize,
    second_delimiter_offset: usize,
    date_format_length: usize,
}

impl OffsetMapping for DateOffsetMapping {
    fn original_to_transformed(&self, offset: usize) -> usize {
        if offset < self.first_delimiter_offset {
            offset
        } else if offset < self.second_delimiter_offset {
            offset + 1
        } else if offset <= self.date_format_length {
            offset + 2
        } else {
            // Past a full entry there is nothing more to show; clamp rather than run off the end.
            self.date_format_length + 2
        }
    }

    /// Deliberate deviation from `DateInput.kt:413-420`, and the whole point of the round-trip
    /// test. Compose's branches are `<= firstDelimiterOffset - 1` and `<= secondDelimiterOffset - 1`,
    /// which is one too tight: for `MM/dd/yyyy` it sends displayed offset 5 — the caret just after
    /// `01` — back to 3, while its own forward map puts typed offset 4 at displayed 5. Typing the
    /// day therefore moves the caret one character back as soon as the caret crosses the second
    /// delimiter. Using `<= firstDelimiterOffset` and `<= secondDelimiterOffset` makes each side
    /// the inverse of the other for every position, and puts both sides of a delimiter — which is
    /// zero-width in the stored text — on the same offset, which is where a caret there belongs.
    ///
    /// Forward is unchanged; it already inverts correctly.
    fn transformed_to_original(&self, offset: usize) -> usize {
        if offset <= self.first_delimiter_offset {
            offset
        } else if offset <= self.second_delimiter_offset {
            offset - 1
        } else if offset <= self.date_format_length + 1 {
            offset - 2
        } else {
            self.date_format_length
        }
    }
}

/// Material Icons `edit` (24 dp) — the mode toggle while the calendar is showing
/// (`DisplayModeToggleButton`, `DatePicker.kt:1413`).
pub const EDIT_PATH: &str = "M3 17.25V21h3.75L17.81 9.94l-3.75-3.75L3 17.25zM20.71 7.04a.996.996 0 0 0 0-1.41l-2.34-2.34a.996.996 0 0 0-1.41 0l-1.83 1.83 3.75 3.75 1.83-1.83z";

/// Material Icons `date_range` (24 dp) — the mode toggle while the text field is showing
/// (`DatePicker.kt:1420`).
pub const DATE_RANGE_PATH: &str = "M9 11H7v2h2v-2zm4 0h-2v2h2v-2zm4 0h-2v2h2v-2zm2-7h-1V2h-2v2H8V2H6v2H5c-1.11 0-1.99.9-1.99 2L3 20a2 2 0 0 0 2 2h14c1.1 0 2-.9 2-2V6c0-1.1-.9-2-2-2zm0 16H5V9h14v11z";

/// The input field's padding, 24 dp at each end (`InputTextFieldPadding`, `DateInput.kt:441`).
pub const INPUT_TEXT_FIELD_PADDING: f32 = 24.0;

/// How long the entry field waits before taking focus for itself, material3's
/// `MotionTokens.DurationMedium2` (`DateInput.kt:259-266`). Long enough for the picker's own
/// entrance motion to have finished, so the caret does not arrive mid-animation.
pub const MODAL_MODE_SWITCH_FOCUS_DELAY_MS: u64 = 300;

/// The test tag on the date entry field, so a UI test can click and type into it without having to
/// find the field by its geometry.
pub const INPUT_FIELD_TEST_TAG: &str = "date-picker-input-field";

/// The test tag on the mode toggle in the header.
pub const MODE_TOGGLE_TEST_TAG: &str = "date-picker-mode-toggle";

/// The bottom padding the field carries only while no error is showing, so an error appearing as
/// supporting text does not make the container jump (`InputTextNonErroneousBottomPadding`,
/// `DateInput.kt:445`).
pub const INPUT_TEXT_NON_ERROROUS_BOTTOM_PADDING: f32 = 16.0;

/// The modal date picker's text entry half: one outlined field that takes the date as digits and
/// offers the locale's pattern as its placeholder (`DateInputContent`, `DateInput.kt:59-113`).
///
/// The field holds the eight digits with no delimiters — [`DateVisualTransformation`] is what puts
/// them on screen — and [`DatePickerState::validate_date_input`] is what decides whether an entry
/// may become the selection. An entry that is not complete, or that fails a check, leaves the
/// selection empty rather than committing something the field itself calls wrong.
#[composable]
pub fn date_input_content(ctx: &mut ComposeCtx, state: &DatePickerState) {
    let model = state.calendar_model().clone();
    let format = model.date_input_format().clone();
    let pattern = format.pattern_with_delimiters().to_uppercase();
    let selected = state.selected_date_millis();

    let text: State<TextFieldValue> = ctx.remember(|| TextFieldValue::new(""));
    let error: State<String> = ctx.remember(|| String::new());

    // A selection made outside the field — the calendar, the initial value — rewrites the digits,
    // exactly as `LaunchedEffect(initialDateMillis)` does (`DateInput.kt:238-258`).
    let effect_selected = selected;
    let effect_model = model.clone();
    let effect_format = format.clone();
    let effect_text = text.clone();
    let effect_error = error.clone();
    LaunchedEffect::new(selected).build(ctx, move |_scope| {
        async move {
            let Some(millis) = effect_selected else { return };
            // `TextFieldValue::new` puts the caret at the end, which is the right place for a value
            // this field just filled in wholesale.
            effect_text.set(TextFieldValue::new(effect_model.format_with_pattern(millis, &effect_format)));
            effect_error.set(String::new());
        }
    });

    // The field asks for focus once it is showing, but not straight away: the picker has just
    // animated in, and a caret that arrives mid-motion is a caret the user never chose. Compose
    // waits `MotionTokens.DurationMedium2` for the same reason
    // (`LaunchedEffect(Unit) { delay(...); focusRequester?.requestFocus() }`, `DateInput.kt:259-266`).
    let focus = ctx.remember(|| crate::modifier::FocusRequester::new()).get();
    let effect_focus = focus.clone();
    LaunchedEffect::new(()).build(ctx, move |_scope| {
        async move {
            tokio::time::sleep(std::time::Duration::from_millis(
                MODAL_MODE_SWITCH_FOCUS_DELAY_MS,
            ))
            .await;
            effect_focus.request_focus();
        }
    });

    // Anything that is not a digit, or that runs past a full entry, is refused outright: the field
    // keeps what it had (`DateInput.kt:166-169`). A shorter entry clears the error and empties the
    // selection without being judged; a full one is parsed and judged, and only commits if it
    // passes (`DateInput.kt:171-200`).
    let on_value_change_model = model.clone();
    let on_value_change_format = format.clone();
    let on_value_change_text = text.clone();
    let on_value_change_error = error.clone();
    let on_value_change_state = state.clone();

    let message = error.get();
    let is_error = !message.trim().is_empty();

    let transformation: Arc<dyn VisualTransformation> =
        Arc::new(DateVisualTransformation::new(&format));

    // The message is the field's error semantics as well as its supporting text, so a reader is
    // told what is wrong rather than left to find it on the screen (`DateInput.kt:211-214`).
    let field_semantics = if is_error {
        crate::semantics::SemanticsConfig::new()
            .state(crate::semantics::SemanticsState::new().error(message.clone()))
    } else {
        crate::semantics::SemanticsConfig::new()
    };

    let label_pattern = pattern.clone();
        let mut field = TextField::new(text.clone())
        .outlined()
        .single_line(true)
        .visual_transformation(transformation)
        .label(move |ctx| {
            Text::new(DATE_INPUT_LABEL)
                .modifier(Modifier::new().semantics(
                    crate::semantics::SemanticsConfig::new()
                        // The label names the field and the shape it wants, so a reader says what to
                        // type before the user types it (`DateInput.kt:93-98`).
                        .content_description(format!("{DATE_INPUT_LABEL}, {label_pattern}")),
                ))
                .build(ctx);
        })
        .placeholder(move |ctx| {
            Text::new(pattern.clone()).build(ctx);
        })
        .on_value_change(move |value| {
            let digits = value.text.trim().to_string();
            let width = on_value_change_format.pattern_length();
            if digits.len() > width || !digits.bytes().all(|b| b.is_ascii_digit()) {
                return;
            }
            on_value_change_text.set(value);
            if digits.is_empty() || digits.len() < width {
                on_value_change_error.set(String::new());
                on_value_change_state.set_selected_date_millis(None);
                return;
            }
            let parsed = on_value_change_model.parse(&digits, &on_value_change_format);
            let message = on_value_change_state.validate_date_input(parsed);
            on_value_change_error.set(message.clone());
            // Commit only what the validator passed, so the calendar never shows a date the field
            // is currently calling wrong.
            let millis = if message.is_empty() { parsed.map(|d| d.start_of_day_millis()) } else { None };
            on_value_change_state.set_selected_date_millis(millis);
        })
        .is_error(is_error);
    if is_error {
        field = field.supporting_text(message);
    }
    // The error's own line of supporting text brings its own padding, so the bottom padding is only
    // there to keep the container the same height either way (`DateInput.kt:152-162`).
    let bottom = if is_error { 0.0 } else { INPUT_TEXT_NON_ERROROUS_BOTTOM_PADDING };
    field
        .modifier(
            Modifier::new()
                .test_tag(INPUT_FIELD_TEST_TAG)
                .semantics(field_semantics)
                .focus_requester(focus.clone())
                .padding_start(INPUT_TEXT_FIELD_PADDING)
                .padding_end(INPUT_TEXT_FIELD_PADDING)
                .padding_bottom(bottom),
        )
        .build(ctx);
}

/// The button that moves between the calendar and the text field
/// (`DisplayModeToggleButton`, `DatePicker.kt:1401-1425`).
///
/// Neither icon is auto-mirrored: material3 gives `Icon` no `autoMirrored`, and the pair means
/// "edit" and "calendar" rather than a direction.
pub(super) fn display_mode_toggle(
    ctx: &mut ComposeCtx,
    display_mode: DisplayMode,
    on_toggle: impl Fn() + Send + Sync + 'static,
    color: Color,
) {
    let (path, description) = match display_mode {
        DisplayMode::Picker => (EDIT_PATH, SWITCH_TO_INPUT_MODE_DESCRIPTION),
        DisplayMode::Input => (DATE_RANGE_PATH, SWITCH_TO_CALENDAR_MODE_DESCRIPTION),
    };
    IconButton::new()
        .on_click(on_toggle)
        .modifier(Modifier::new().test_tag(MODE_TOGGLE_TEST_TAG))
        .build(ctx, |ctx| {
            Icon::svg_path(path)
                .tint(color)
                .modifier(Modifier::new().semantics(
                    crate::semantics::SemanticsConfig::new().content_description(description),
                ))
                .build(ctx);
        });
}

