//! Effect API — 类似 Jetpack Compose 的 LaunchedEffect / DisposableEffect。
//!
//! - LaunchedEffect: key 变化时启动异步任务，离开组合时自动取消
//! - DisposableEffect: key 变化时执行同步副作用，离开组合时清理
//! - remember_coroutine_scope: 获取组合生命周期绑定的协程作用域
//! - observe_watch: 将 watch channel 转为组合生命周期绑定的 State
//! - remember_updated_state: follow the latest value from a long-lived effect
//! - on_forget: register a cleanup for one composition point (what the effects build on)
//!
//! 依赖 tokio 运行时（已作为项目依赖）。
//!
//! # Timing: these run DURING composition, not after it
//!
//! Compose runs `DisposableEffect`'s setup/cleanup and starts `LaunchedEffect`'s coroutine after the
//! composition is applied. Here both happen inline in the compose pass: `DisposableEffect`'s setup and
//! cleanup are plain synchronous calls from `build`, and `LaunchedEffect` spawns its task from the same
//! place. Two consequences a caller has to know about:
//!
//! - A setup/cleanup that reads layout, window size or a `State` another branch of the same
//!   composition is still writing sees a HALF-BUILT tree, not the frame that is about to be shown.
//! - Writing a `State` from a setup marks its readers dirty while they may still be composing in this
//!   same pass.
//!
//! Moving effects after the apply phase needs a commit hook the composer does not have yet, so this is
//! documented rather than changed: treat an effect body as "runs somewhere inside the frame", and keep
//! it to work that does not depend on the frame being finished.
//!
//! # Lock discipline
//!
//! User code (the effect body and the cleanup a caller returns) is NEVER invoked while one of this
//! module's `Mutex` guards is alive. A panic in it would poison the guard, and every later
//! `lock().unwrap()` in that effect would panic — the instance would be dead for the rest of the run.
//! A caller that re-enters the same state from its cleanup would deadlock outright, `std::sync::Mutex`
//! being non-reentrant. Every site below therefore takes what it needs under the lock and calls out
//! after releasing it.

mod disposable;
mod frame_clock;
mod hooks;
mod launched;
mod scope;
mod watch;

pub use disposable::DisposableEffect;
pub use frame_clock::{frame_tick, with_frame_nanos};
pub use hooks::{on_forget, remember_updated_state, side_effect};
pub use launched::LaunchedEffect;
pub use scope::{remember_coroutine_scope, CoroutineScope};
pub use watch::observe_watch;

#[cfg(test)]
mod tests;
