//! The frame clock `with_frame_nanos` waits on, ticked by the render loop.

use std::sync::LazyLock;
use tokio::sync::broadcast;

/// 帧时钟发送端——渲染循环（RedrawRequested 处理器）每帧调用 [`frame_tick`]。
/// broadcast 队列化每个帧时间戳：订阅者收到**下一帧**（无 watch 的跳过语义）。
/// static 持有 receiver 防 send 因无订阅者而失败。
static FRAME_CLOCK: LazyLock<(broadcast::Sender<u64>, broadcast::Receiver<u64>)> = LazyLock::new(|| {
    let (tx, rx) = broadcast::channel(16);
    (tx, rx)
});

static FRAME_EPOCH: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();

/// 单调时钟纳秒（Instant 基准——墙钟 SystemTime 可能回拨，导致 dt 为负/巨大）
fn monotonic_nanos() -> u64 {
    let epoch = *FRAME_EPOCH.get_or_init(std::time::Instant::now);
    // max(1)：首次初始化与调用同纳秒时 elapsed=0（测试断言 > 0 会失败）
    (epoch.elapsed().as_nanos() as u64).max(1)
}

/// 帧循环每帧调用（app.rs RedrawRequested 内注入）——向所有等待者广播帧时间戳。
/// 无副作用失败：无订阅者时 send 返回 Err，忽略（帧时钟只是辅助驱动）。
pub fn frame_tick() {
    let _ = FRAME_CLOCK.0.send(monotonic_nanos());
}

/// 等待下一帧并返回帧时间戳（纳秒，UNIX epoch 基准）——
/// 自定义动画的帧驱动入口（对标 Compose `withFrameNanos`）。
///
/// 用法（LaunchedEffect / CoroutineScope 内）：
/// ```ignore
/// scope.spawn(async move {
///     let mut last = 0u64;
///     loop {
///         let now = winia::effect::with_frame_nanos().await;
///         let dt = (now - last) as f32 / 1e9;
///         last = now;
///         // 按 dt 推进自定义状态……
///     }
/// });
/// ```
///
/// 注意：帧时钟由渲染循环驱动——窗口未渲染（最小化/无 request_redraw）时
/// 不会返回。需要"时间驱动"而非"帧驱动"请用 tokio sleep。
pub fn with_frame_nanos() -> impl std::future::Future<Output = u64> + Send {
    // 非 async fn：subscribe 在**调用时**执行（async fn 在首次 poll 才执行——
    // 会错过调用与 await 之间 frame_tick 发出的帧）
    let mut rx = FRAME_CLOCK.0.subscribe();
    // 同步丢弃订阅时刻已存在的旧帧：broadcast 新 receiver 的游标从当前
    // 最新开始——不丢弃的话 recv 会立即返回"已经发生"的帧而非"下一帧"
    //（并行测试 poll 时机不定——flaky 根因）。
    // ⚠ 必须在这里同步执行：放进 async body 会在首次 poll 才运行——若
    // frame_tick 发生在 subscribe 与 poll 之间，会把"下一帧"误当旧帧丢弃
    // → 死等（曾引入此 bug）。
    // Drain EVERY tick that predates the subscription. One `try_recv` was not enough: with several
    // frames buffered the future returned the oldest of them instead of the next frame.
    while rx.try_recv().is_ok() {}
    async move {
        loop {
            match rx.recv().await {
                Ok(v) => return v,
                // 订阅晚于缓冲区淘汰（长时间无渲染后恢复）——跳过丢失帧继续等
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => return 0, // 发送端消亡——兜底
            }
        }
    }
}
