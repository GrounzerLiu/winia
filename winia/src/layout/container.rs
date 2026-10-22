//! The shared build boilerplate for the user-facing containers (Column/Row/Stack/Flow/Spacer).
//!
//! The container builders live next to their measure policies; this module keeps the one piece of
//! composition glue they all use. Parameters are declared by each builder (their sets differ); this
//! only pairs `start_restartable_group` with `end_restartable_group` and handles Skip/Enter.

use super::node::MeasurePolicy;
use crate::modifier::Modifier;
use crate::runtime::composer::{ComposeCtx, GroupStatus};

pub(crate) fn build_container(
    ctx: &mut ComposeCtx,
    modifier: Modifier,
    policy: impl MeasurePolicy + 'static,
    content: impl FnOnce(&mut ComposeCtx),
) {
    let key = ctx.next_key();
    match ctx.start_restartable_group(key, modifier, policy) {
        GroupStatus::Skip => {}
        GroupStatus::Enter => {
            content(ctx);
        }
    }
    ctx.end_restartable_group();
}
