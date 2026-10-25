//! Date pickers, starting with the calendar the picker is built on.
//!
//! material3's date pickers are a calendar model plus three surfaces over it: the docked date picker, the
//! modal date picker and the modal date input (see `docs/date-picker.md` for the spec tables and the source
//! anchors). This module starts at the bottom of that stack — the proleptic Gregorian date arithmetic and
//! the month descriptions the grid, the header and the range pickers all read.
//!
//! The rules come from `androidx/compose/material3/internal/CalendarModel.kt` and its Android
//! implementation:

//! everything is UTC (material3's millis values are "the start of the day in UTC"), a week starts on the
//! locale's `first_day_of_week`, and the offset of a month's first day inside that week is
//! `day_of_week - first_day_of_week`, wrapped into `0..7` (`CalendarModelImpl.android.kt:201-222`).
//!
//! winia has no locale database, so [`CalendarLocale`] carries the weekday and month names and the first day
//! of the week explicitly, and [`CalendarLocale::default`] is an English, Sunday-first locale. A caller that
//! needs another language supplies its own; the picker never reads a global.

use crate::animation::visibility::VisibilityTransition;
use crate::animation::{AnimationSpec, TweenSpec};
use crate::components::animated_visibility::AnimatedVisibility;
use crate::composable;
use crate::runtime::composer::ComposeCtx;
use crate::runtime::state::State;
use crate::layout::{Alignment, Arrangement};
use crate::modifier::{Modifier};
use crate::graphics::{Color, Shape};
use crate::components::alert_dialog::{AlertDialogDefaults, BasicAlertDialog};
use crate::components::button::Button;
use crate::components::divider::Divider;
use crate::components::icon::Icon;
use crate::components::icon_button::{IconButton, IconButtonSize};
use crate::layout::lazy::{LazyColumn, LazyListState, LazyRow};
use crate::layout::components::{Column, Row, Spacer, Stack};
use crate::components::dropdown_menu::ExposedDropdownMenuDefaults;
use crate::components::scrollbar::LazyScrollbar;
use crate::components::surface::{Surface, SurfaceBorder};
use crate::effect::LaunchedEffect;
use crate::components::text::{ProvideTextStyle, Text};
use crate::components::text_field::{TextField, TextFieldValue};
use crate::text::transformation::{OffsetMapping, TransformedText, VisualTransformation};
use crate::theme::{ThemeColors, WiniaTheme};
use std::ops::RangeInclusive;
use std::sync::Arc;

/// The alpha material3's date picker brings its year panel in from (`DatePicker.kt:1617`:
/// `fadeIn(animationSpec = fadeInAnimationSpec, initialAlpha = 0.6f)`) — the panel is already 60%
/// opaque when the expand starts, so it reads as a reveal rather than as a second surface arriving.
const YEAR_PANEL_INITIAL_ALPHA: f32 = 0.6;

/// Milliseconds in a day, the unit material3's pickers count in (`CalendarModel.kt:316`).
pub const MILLIS_IN_24_HOURS: i64 = 86_400_000;

/// Days in a week (`CalendarModel.kt:315`).
pub const DAYS_IN_WEEK: u32 = 7;

/// The default year range of the date picker dialogs, `DatePickerDefaults.YearRange`
/// (`DatePicker.kt:764`).
pub const DEFAULT_YEAR_RANGE: std::ops::RangeInclusive<i32> = 1900..=2100;

/// A day in the proleptic Gregorian calendar, with no time and no zone.
mod calendar;
mod defaults;
mod input;
mod picker;
mod state;

pub use calendar::*;
pub use defaults::*;
pub use input::*;
pub use picker::*;
pub use state::*;

#[cfg(test)]
mod tests;
