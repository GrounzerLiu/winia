use super::*;

// ═══════════════════════════════════════════════════════
// LinearWavyProgressIndicator
// ═══════════════════════════════════════════════════════

/// 线性 wavy 进度指示器（对标 Compose `LinearWavyProgressIndicator`）。
///
/// determinate：`LinearWavyProgressIndicator::new(progress)`；
/// indeterminate：`LinearWavyProgressIndicator::indeterminate()`。
#[derive(Clone)]
pub struct LinearWavyProgressIndicator {
    pub(crate) progress: f32,
    pub(crate) indeterminate: bool,
    pub(crate) modifier: Modifier,
    pub(crate) color: Option<Color>,
    pub(crate) track_color: Option<Color>,
    pub(crate) stroke_width: f32,
    pub(crate) track_stroke_width: f32,
    pub(crate) gap_size: f32,
    pub(crate) stop_size: f32,
    pub(crate) amplitude: WavyAmplitude,
    pub(crate) wavelength: f32,
    pub(crate) wave_speed: f32,
}

impl LinearWavyProgressIndicator {
    /// determinate 构造（progress 0..1，越界 clamp）
    pub fn new(progress: f32) -> Self {
        Self {
            progress,
            indeterminate: false,
            modifier: Modifier::new(),
            color: None,
            track_color: None,
            stroke_width: WavyProgressIndicatorDefaults::linear_stroke_width(),
            track_stroke_width: WavyProgressIndicatorDefaults::linear_track_stroke_width(),
            gap_size: WAVY_GAP_SIZE,
            stop_size: WAVY_LINEAR_STOP_SIZE,
            amplitude: WavyAmplitude::Indicator,
            wavelength: WavyProgressIndicatorDefaults::linear_determinate_wavelength(),
            wave_speed: WavyProgressIndicatorDefaults::linear_determinate_wavelength(),
        }
    }

    /// indeterminate 构造：无限 wave + 4 条 head/tail 动画
    pub fn indeterminate() -> Self {
        Self::new(0.0).indeterminate_mode()
    }

    pub(crate) fn indeterminate_mode(mut self) -> Self {
        self.indeterminate = true;
        self.amplitude = WavyAmplitude::Fixed(1.0);
        self.wavelength = WavyProgressIndicatorDefaults::linear_indeterminate_wavelength();
        self.wave_speed = self.wavelength;
        self
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    /// 指示器颜色（默认 Primary）
    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    /// 轨道颜色（默认 SecondaryContainer）
    pub fn track_color(mut self, color: Color) -> Self {
        self.track_color = Some(color);
        self
    }

    /// active 描边宽度（dp，默认 4）
    pub fn stroke_width(mut self, width: f32) -> Self {
        self.stroke_width = width.max(0.0);
        self
    }

    /// track 描边宽度（dp，默认 4）
    pub fn track_stroke_width(mut self, width: f32) -> Self {
        self.track_stroke_width = width.max(0.0);
        self
    }

    /// 指示器与轨道间隙（dp，默认 4）
    pub fn gap_size(mut self, gap: f32) -> Self {
        self.gap_size = gap.max(0.0);
        self
    }

    /// stop indicator 尺寸（dp，默认 4）
    pub fn stop_size(mut self, size: f32) -> Self {
        self.stop_size = size.max(0.0);
        self
    }

    /// 自定义振幅函数（determinate；对标 Compose `amplitude` lambda）
    pub fn amplitude_fn(mut self, f: impl Fn(f32) -> f32 + Send + Sync + 'static) -> Self {
        self.amplitude = WavyAmplitude::Custom(Arc::new(f));
        self
    }

    /// 固定振幅（indeterminate；对标 Compose `amplitude: Float`）
    pub fn amplitude(mut self, value: f32) -> Self {
        self.amplitude = WavyAmplitude::Fixed(value);
        self
    }

    /// 波长（dp，默认 determinate 40 / indeterminate 20）
    pub fn wavelength(mut self, value: f32) -> Self {
        self.wavelength = value.max(0.0);
        self
    }

    /// 波速（dp/s，默认 = wavelength，即每秒移动一个波长）
    pub fn wave_speed(mut self, value: f32) -> Self {
        self.wave_speed = value.max(0.0);
        self
    }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        ctx.changed(&self.progress);
        ctx.changed(&self.indeterminate);
        ctx.changed(&self.color);
        ctx.changed(&self.track_color);
        ctx.changed(&self.stroke_width);
        ctx.changed(&self.track_stroke_width);
        ctx.changed(&self.gap_size);
        ctx.changed(&self.stop_size);
        ctx.changed(&self.wavelength);
        ctx.changed(&self.wave_speed);
        let amplitude_token = self.amplitude.change_token();
        ctx.changed(&amplitude_token);

        let key = ctx.next_key();
        let theme = WiniaTheme::colors();
        let color = self
            .color
            .unwrap_or_else(|| WavyProgressIndicatorDefaults::indicator_color(&theme));
        let track_color = self
            .track_color
            .unwrap_or_else(|| WavyProgressIndicatorDefaults::track_color(&theme));
        let stroke_width = self.stroke_width;
        let track_stroke_width = self.track_stroke_width;
        let gap_size = self.gap_size;
        let stop_size = self.stop_size;
        let wavelength = self.wavelength;
        let wave_speed = self.wave_speed;
        let cap = ProgressIndicatorStrokeCap::Round;
        let enable_motion = wave_speed > 0.0 && wavelength > 0.0;

        let shapes_cache = Arc::new(Mutex::new(LinearShapesCache::default()));

        let m = if self.indeterminate {
            let amplitude = self.amplitude.fixed();
            let mut inf = ctx.remember_infinite_transition();
            let wave_offset = inf.animate_float_preserving(
                ctx,
                0.0,
                1.0,
                wave_animation_spec(wavelength, wave_speed),
            );
            if !enable_motion {
                crate::animation::remove_animation_by_state(wave_offset.state_id());
            }
            let fh = inf.animate_float(
                ctx,
                0.0,
                1.0,
                progress_indicator::linear_first_line_head_spec(),
            );
            let ft = inf.animate_float(
                ctx,
                0.0,
                1.0,
                progress_indicator::linear_first_line_tail_spec(),
            );
            let sh = inf.animate_float(
                ctx,
                0.0,
                1.0,
                progress_indicator::linear_second_line_head_spec(),
            );
            let st = inf.animate_float(
                ctx,
                0.0,
                1.0,
                progress_indicator::linear_second_line_tail_spec(),
            );
            let cache = shapes_cache.clone();
            Modifier::new()
                .size(WAVY_LINEAR_WIDTH, WAVY_LINEAR_HEIGHT)
                .clip(Shape::Rectangle)
                .draw_node(LinearWavyIndeterminateNode {
                    color, track_color, stroke_width, track_stroke_width,
                    cap, gap_size, amplitude, wavelength, enable_motion,
                    fh: fh.clone(), ft: ft.clone(), sh: sh.clone(), st: st.clone(),
                    wave_offset: wave_offset.clone(),
                    cache,
                })
        } else {
            let progress = self.progress;
            let amplitude_fn = self.amplitude.clone();
            let amplitude_state: State<f32> = ctx.remember(|| amplitude_fn.resolve(progress));
            let target = amplitude_fn.resolve(progress);
            let current = amplitude_state.peek();
            let spec = if current < target {
                increasing_amplitude_spec()
            } else {
                decreasing_amplitude_spec()
            };
            let mut inf = ctx.remember_infinite_transition();
            let wave_offset = inf.animate_float_preserving(
                ctx,
                0.0,
                1.0,
                wave_animation_spec(wavelength, wave_speed),
            );
            // Compose 仅在真正画波时运行 wave offset 动画：motion 关闭或振幅为 0 时
            // 立即移除刚注册的无限动画（保留 State，绘制时读到 0）。
            let wave_active = enable_motion && (target > 0.0 || current > 0.0);
            if !wave_active {
                crate::animation::remove_animation_by_state(wave_offset.state_id());
            }

            let wave_for_draw = wave_offset.clone();
            if target == 0.0 && current != 0.0 {
                // 振幅从 >0 过渡到 0：动画结束后移除 wave offset 动画，避免持续空转。
                // 注：done 回调仅作兜底；振幅到达 0 后的重组分支也会显式移除。
                let wave_for_stop = wave_offset.clone();
                crate::animation::push_animatable_with_done(
                    amplitude_state.clone(),
                    target,
                    spec,
                    move || {
                        crate::animation::remove_animation_by_state(wave_for_stop.state_id());
                    },
                );
            } else {
                crate::animation::push_animatable(amplitude_state.clone(), target, spec);
            }

            let cache = shapes_cache.clone();
            Modifier::new()
                .size(WAVY_LINEAR_WIDTH, WAVY_LINEAR_HEIGHT)
                .clip(Shape::Rectangle)
                .draw_node(LinearWavyDeterminateNode {
                    color, track_color, stroke_width, track_stroke_width,
                    cap, gap_size, stop_size, progress, wavelength, enable_motion,
                    amplitude_token,
                    amplitude_state: amplitude_state.clone(),
                    wave_offset: wave_for_draw.clone(),
                    cache,
                })
        };


        // Same rule as the plain indicators: the value is what a progress bar IS to a screen reader,
        // and an indeterminate one has none to report.
        let m = m.semantics(match self.indeterminate {
            true => crate::semantics::SemanticsConfig::new()
                .role(crate::semantics::SemanticsRole::ProgressBar),
            false => crate::semantics::SemanticsConfig::new()
                .role(crate::semantics::SemanticsRole::ProgressBar)
                .state(crate::semantics::SemanticsState::new().progress(self.progress, 0.0, 1.0)),
        });

        let m = m.then(self.modifier);
        match ctx.start_restartable_group(key, m, BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {}
        }
        ctx.end_restartable_group();
    }
}

/// Wavy 绘制节点（exp/wavy-node 迁移：原 `.draw` 匿名闭包 ×4 + loading ×1）。
///
/// 照抄 `SliderTrackNode` 形状：具名 struct + `DrawNode`，`draw` 内复用原
/// 绘制函数。静态视觉参数全进 `node_key`；无限动画值（fh/ft/sh/st/wave_offset/
/// global/additional/progress_anim）与振幅过渡值（amplitude_state）渲染期
/// `peek` 直读，不进 key（重绘由动画引擎每帧 `request_redraw` 驱动）。
/// 路径缓存（`Arc<Mutex<ShapesCache>>`）为性能设施，不影响绘制语义，不进 key；
/// cache 含 `skia_safe::Path` 不宜 Debug，故手写 `impl Debug`（只打静态参数）。
/// `amplitude_token`（WavyAmplitude 身份，fn 指针/固定值）进 key——fn 换了即 Enter。
pub(crate) struct LinearWavyIndeterminateNode {
    pub(crate) color: Color,
    pub(crate) track_color: Color,
    pub(crate) stroke_width: f32,
    pub(crate) track_stroke_width: f32,
    pub(crate) cap: ProgressIndicatorStrokeCap,
    pub(crate) gap_size: f32,
    pub(crate) amplitude: f32,
    pub(crate) wavelength: f32,
    pub(crate) enable_motion: bool,
    pub(crate) fh: crate::runtime::state::Visual<f32>,
    pub(crate) ft: crate::runtime::state::Visual<f32>,
    pub(crate) sh: crate::runtime::state::Visual<f32>,
    pub(crate) st: crate::runtime::state::Visual<f32>,
    pub(crate) wave_offset: crate::runtime::state::Visual<f32>,
    pub(crate) cache: Arc<Mutex<LinearShapesCache>>,
}

impl std::fmt::Debug for LinearWavyIndeterminateNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LinearWavyIndeterminateNode")
            .field("color", &self.color)
            .field("track_color", &self.track_color)
            .field("stroke_width", &self.stroke_width)
            .field("track_stroke_width", &self.track_stroke_width)
            .field("cap", &self.cap)
            .field("gap_size", &self.gap_size)
            .field("amplitude", &self.amplitude)
            .field("wavelength", &self.wavelength)
            .field("enable_motion", &self.enable_motion)
            .finish_non_exhaustive()
    }
}

impl crate::modifier::DrawNode for LinearWavyIndeterminateNode {
    fn draw(&self, canvas: &skia_safe::Canvas, rect: skia_safe::Rect) {
        draw_linear_wavy_indeterminate(
            canvas, rect,
            self.color, self.track_color,
            self.stroke_width, self.track_stroke_width,
            self.cap, self.gap_size,
            self.amplitude, self.wavelength, self.enable_motion,
            self.fh.peek(), self.ft.peek(), self.sh.peek(), self.st.peek(),
            self.wave_offset.peek(),
            &self.cache,
        );
    }
    fn node_key(&self) -> String {
        format!(
            "wavy-linear-indet:{:?}:{:?}:{}:{}:{}:{}:{}:{}:{}",
            self.color,
            self.track_color,
            self.stroke_width.to_bits(),
            self.track_stroke_width.to_bits(),
            self.cap as u8,
            self.gap_size.to_bits(),
            self.amplitude.to_bits(),
            self.wavelength.to_bits(),
            self.enable_motion,
        )
    }
}

/// Linear determinate wavy 节点（振幅过渡值 peek，不进 key；fn 身份 token 进 key）。
pub(crate) struct LinearWavyDeterminateNode {
    pub(crate) color: Color,
    pub(crate) track_color: Color,
    pub(crate) stroke_width: f32,
    pub(crate) track_stroke_width: f32,
    pub(crate) cap: ProgressIndicatorStrokeCap,
    pub(crate) gap_size: f32,
    pub(crate) stop_size: f32,
    pub(crate) progress: f32,
    pub(crate) wavelength: f32,
    pub(crate) enable_motion: bool,
    pub(crate) amplitude_token: (u8, u64, usize),
    pub(crate) amplitude_state: State<f32>,
    pub(crate) wave_offset: crate::runtime::state::Visual<f32>,
    pub(crate) cache: Arc<Mutex<LinearShapesCache>>,
}

impl std::fmt::Debug for LinearWavyDeterminateNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LinearWavyDeterminateNode")
            .field("color", &self.color)
            .field("track_color", &self.track_color)
            .field("stroke_width", &self.stroke_width)
            .field("track_stroke_width", &self.track_stroke_width)
            .field("cap", &self.cap)
            .field("gap_size", &self.gap_size)
            .field("stop_size", &self.stop_size)
            .field("progress", &self.progress)
            .field("wavelength", &self.wavelength)
            .field("enable_motion", &self.enable_motion)
            .field("amplitude_token", &self.amplitude_token)
            .finish_non_exhaustive()
    }
}

impl crate::modifier::DrawNode for LinearWavyDeterminateNode {
    fn draw(&self, canvas: &skia_safe::Canvas, rect: skia_safe::Rect) {
        draw_linear_wavy_determinate(
            canvas, rect,
            self.color, self.track_color,
            self.stroke_width, self.track_stroke_width,
            self.cap, self.gap_size, self.stop_size,
            self.progress,
            self.amplitude_state.peek(),
            self.wavelength,
            self.wave_offset.peek(),
            true,
            self.enable_motion,
            &self.cache,
        );
    }
    fn node_key(&self) -> String {
        format!(
            "wavy-linear-det:{:?}:{:?}:{}:{}:{}:{}:{}:{}:{}:{}:{:?}",
            self.color,
            self.track_color,
            self.stroke_width.to_bits(),
            self.track_stroke_width.to_bits(),
            self.cap as u8,
            self.gap_size.to_bits(),
            self.stop_size.to_bits(),
            self.progress.to_bits(),
            self.wavelength.to_bits(),
            self.enable_motion,
            self.amplitude_token,
        )
    }
}

/// Linear determinate wavy 绘制。
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_linear_wavy_determinate(
    canvas: &skia_safe::Canvas,
    rect: skia_safe::Rect,
    color: Color,
    track_color: Color,
    stroke_width: f32,
    track_stroke_width: f32,
    cap: ProgressIndicatorStrokeCap,
    gap_size: f32,
    stop_size: f32,
    progress: f32,
    amplitude: f32,
    wavelength: f32,
    wave_offset: f32,
    draw_stop: bool,
    enable_motion: bool,
    cache: &Mutex<LinearShapesCache>,
) {
    let fractions = [0.0f32, progress.clamp(0.0, 1.0)];
    let (track_path, progress_paths) = {
        let mut cache = cache.lock().unwrap_or_else(|e| e.into_inner());
        build_linear_wavy_paths(
            &mut cache,
            rect,
            wavelength,
            &fractions,
            amplitude,
            wave_offset,
            gap_size,
            stroke_width,
            track_stroke_width,
            cap,
            enable_motion,
        )
    };

    canvas.save();
    canvas.translate((rect.left, rect.top));
    draw_linear_wavy_paths(
        canvas,
        rect,
        &track_path,
        &progress_paths,
        color,
        track_color,
        stroke_width,
        track_stroke_width,
        cap,
    );

    if draw_stop {
        draw_linear_stop_indicator(
            canvas,
            rect,
            fractions[1],
            stop_size,
            cap,
            stroke_width,
            track_stroke_width,
            color,
        );
    }
    canvas.restore();
}

/// Linear indeterminate wavy 绘制（4 条 head/tail 段）。
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_linear_wavy_indeterminate(
    canvas: &skia_safe::Canvas,
    rect: skia_safe::Rect,
    color: Color,
    track_color: Color,
    stroke_width: f32,
    track_stroke_width: f32,
    cap: ProgressIndicatorStrokeCap,
    gap_size: f32,
    amplitude: f32,
    wavelength: f32,
    enable_motion: bool,
    first_head: f32,
    first_tail: f32,
    second_head: f32,
    second_tail: f32,
    wave_offset: f32,
    cache: &Mutex<LinearShapesCache>,
) {
    let fractions = [first_tail, first_head, second_tail, second_head];
    let (track_path, progress_paths) = {
        let mut cache = cache.lock().unwrap_or_else(|e| e.into_inner());
        build_linear_wavy_paths(
            &mut cache,
            rect,
            wavelength,
            &fractions,
            amplitude,
            wave_offset,
            gap_size,
            stroke_width,
            track_stroke_width,
            cap,
            enable_motion,
        )
    };

    canvas.save();
    canvas.translate((rect.left, rect.top));
    draw_linear_wavy_paths(
        canvas,
        rect,
        &track_path,
        &progress_paths,
        color,
        track_color,
        stroke_width,
        track_stroke_width,
        cap,
    );
    canvas.restore();
}

/// Linear 波路径缓存：按 size/wavelength/stroke/cap/振幅是否为零 重建满幅波路径。
/// 避免每帧重新构造二次贝塞尔波和 PathMeasure（对标 Compose `LinearProgressDrawingCache`）。
#[derive(Default)]
pub(crate) struct LinearShapesCache {
    pub(crate) key: Option<(f32, f32, f32, f32, f32, ProgressIndicatorStrokeCap, bool)>,
    pub(crate) full_path: skia_safe::Path,
    pub(crate) full_path_length: f32,
    pub(crate) full_bounds_width: f32,
}

impl LinearShapesCache {
    pub(crate) fn update(
        &mut self,
        width: f32,
        height: f32,
        wavelength: f32,
        stroke_width: f32,
        track_stroke_width: f32,
        cap: ProgressIndicatorStrokeCap,
        amplitude_zero: bool,
    ) {
        let key = (
            width,
            height,
            wavelength,
            stroke_width,
            track_stroke_width,
            cap,
            amplitude_zero,
        );
        if self.key == Some(key) {
            return;
        }

        let mut full_builder = skia_safe::PathBuilder::new();
        full_builder.move_to((0.0, 0.0));
        if amplitude_zero || wavelength <= 0.0 {
            full_builder.line_to((width, 0.0));
        } else {
            let half_wavelength = wavelength / 2.0;
            let mut anchor_x = half_wavelength;
            let mut control_x = half_wavelength / 2.0;
            let mut control_y = height - stroke_width;
            let width_with_extra = width + wavelength * 2.0;
            while anchor_x <= width_with_extra {
                full_builder.quad_to((control_x, control_y), (anchor_x, 0.0));
                anchor_x += half_wavelength;
                control_x += half_wavelength;
                control_y *= -1.0;
            }
        }
        full_builder.offset((0.0, height / 2.0));
        let full_path = full_builder.detach();

        let mut measure = skia_safe::PathMeasure::new(&full_path, false, None);
        let full_path_length = measure.length();
        let full_bounds_width = full_path.bounds().width().max(0.00000001);

        self.key = Some(key);
        self.full_path = full_path;
        self.full_path_length = full_path_length;
        self.full_bounds_width = full_bounds_width;
    }
}

/// 构造 Linear wavy 的 track 路径与各 progress 段路径。
pub(crate) fn build_linear_wavy_paths(
    cache: &mut LinearShapesCache,
    rect: skia_safe::Rect,
    wavelength: f32,
    progress_fractions: &[f32],
    amplitude: f32,
    wave_offset: f32,
    gap_size: f32,
    stroke_width: f32,
    track_stroke_width: f32,
    cap: ProgressIndicatorStrokeCap,
    enable_motion: bool,
) -> (skia_safe::Path, Vec<skia_safe::Path>) {
    let width = rect.width();
    let height = rect.height();
    if width <= 0.0 || height <= 0.0 {
        return (skia_safe::Path::new(), Vec::new());
    }

    let current_stroke_cap_width = if cap == ProgressIndicatorStrokeCap::Butt || height > width {
        0.0
    } else {
        stroke_width.max(track_stroke_width) / 2.0
    };

    // 满幅波路径（按 size/wavelength/stroke/振幅是否为零 缓存，避免每帧重建）
    cache.update(
        width,
        height,
        wavelength,
        stroke_width,
        track_stroke_width,
        cap,
        amplitude == 0.0,
    );
    let mut measure = skia_safe::PathMeasure::new(&cache.full_path, false, None);
    let full_path_length = cache.full_path_length;
    let progress_path_scale = full_path_length / cache.full_bounds_width;

    let half_height = height / 2.0;
    let mut track_builder = skia_safe::PathBuilder::new();
    let mut next_end_track_offset = width - current_stroke_cap_width;
    track_builder.move_to((next_end_track_offset, half_height));

    let mut adjusted_track_gap = gap_size;
    let mut active_indicator_visible = false;
    let mut progress_paths = Vec::with_capacity(progress_fractions.len() / 2);

    for (i, pair) in progress_fractions.chunks_exact(2).enumerate() {
        let start_fraction = pair[0];
        let end_fraction = pair[1];
        let bar_tail = start_fraction * width;
        let bar_head = end_fraction * width;

        if i == 0 {
            adjusted_track_gap = if bar_head < current_stroke_cap_width {
                0.0
            } else {
                (bar_head - current_stroke_cap_width).min(gap_size)
            };
            active_indicator_visible = bar_head >= current_stroke_cap_width;
        }

        let adjusted_bar_head =
            bar_head.clamp(current_stroke_cap_width, width - current_stroke_cap_width);
        let adjusted_bar_tail =
            bar_tail.clamp(current_stroke_cap_width, width - current_stroke_cap_width);

        if (end_fraction - start_fraction).abs() > 0.0 {
            let wave_shift = if amplitude != 0.0 && enable_motion {
                wave_offset.rem_euclid(1.0) * wavelength
            } else {
                0.0
            };
            let mut seg_builder = skia_safe::PathBuilder::new();
            measure.get_segment(
                (adjusted_bar_tail + wave_shift) * progress_path_scale,
                (adjusted_bar_head + wave_shift) * progress_path_scale,
                &mut seg_builder,
                true,
            );
            let mut seg_path = seg_builder.detach();
            let dx = if wave_shift > 0.0 { -wave_shift } else { 0.0 };
            let matrix = skia_safe::Matrix::scale_translate(
                (1.0, amplitude),
                (dx, (1.0 - amplitude) * half_height),
            );
            seg_path = seg_path.make_transform(&matrix);
            progress_paths.push(seg_path);
        }

        let adaptive_track_spacing = if active_indicator_visible {
            adjusted_track_gap + current_stroke_cap_width * 2.0
        } else {
            adjusted_track_gap
        };
        if next_end_track_offset > adjusted_bar_head + adaptive_track_spacing {
            track_builder.line_to((
                (adjusted_bar_head + adaptive_track_spacing).max(current_stroke_cap_width),
                half_height,
            ));
        }
        if bar_head > bar_tail {
            next_end_track_offset =
                (adjusted_bar_tail - adaptive_track_spacing).max(current_stroke_cap_width);
            track_builder.move_to((next_end_track_offset, half_height));
        }
    }

    if next_end_track_offset > current_stroke_cap_width {
        track_builder.line_to((current_stroke_cap_width, half_height));
    }
    let track_path = track_builder.detach();

    (track_path, progress_paths)
}

pub(crate) fn draw_linear_wavy_paths(
    canvas: &skia_safe::Canvas,
    _rect: skia_safe::Rect,
    track_path: &skia_safe::Path,
    progress_paths: &[skia_safe::Path],
    color: Color,
    track_color: Color,
    stroke_width: f32,
    track_stroke_width: f32,
    cap: ProgressIndicatorStrokeCap,
) {
    if track_color.a != 0 {
        let track_paint = paint_stroke(track_stroke_width, cap, track_color);
        canvas.draw_path(track_path, &track_paint);
    }
    if color.a != 0 {
        let progress_paint = paint_stroke(stroke_width, cap, color);
        for path in progress_paths {
            canvas.draw_path(path, &progress_paint);
        }
    }
}

/// Linear stop indicator（对标 Compose `drawStopIndicator`）。
pub(crate) fn draw_linear_stop_indicator(
    canvas: &skia_safe::Canvas,
    rect: skia_safe::Rect,
    progress_end: f32,
    max_stop_size: f32,
    cap: ProgressIndicatorStrokeCap,
    stroke_width: f32,
    track_stroke_width: f32,
    color: Color,
) {
    let width = rect.width();
    let height = rect.height();
    if width <= 0.0 || height <= 0.0 {
        return;
    }
    let mut stop_size = track_stroke_width.min(max_stop_size);
    let indicator_x_offset = if stop_size == track_stroke_width {
        0.0
    } else {
        track_stroke_width / 4.0
    };
    let mut indicator_x = width - stop_size - indicator_x_offset;
    let cap_width = stroke_cap_width(cap, stroke_width, track_stroke_width, width, height);
    let progress_x = width * progress_end + cap_width;
    if indicator_x <= progress_x {
        stop_size = (stop_size - (progress_x - indicator_x)).max(0.0);
        indicator_x = progress_x;
    }
    if stop_size > 0.0 {
        let mut paint = skia_safe::Paint::default();
        paint.set_anti_alias(true);
        paint.set_color(skia_color(color));
        let cy = height / 2.0;
        if cap == ProgressIndicatorStrokeCap::Round {
            canvas.draw_circle(
                skia_safe::Point::new(indicator_x + stop_size / 2.0, cy),
                stop_size / 2.0,
                &paint,
            );
        } else {
            let r =
                skia_safe::Rect::from_xywh(indicator_x, cy - stop_size / 2.0, stop_size, stop_size);
            canvas.draw_rect(r, &paint);
        }
    }
}

pub(crate) fn stroke_cap_width(
    cap: ProgressIndicatorStrokeCap,
    stroke_width: f32,
    track_stroke_width: f32,
    width: f32,
    height: f32,
) -> f32 {
    if cap == ProgressIndicatorStrokeCap::Butt || height > width {
        0.0
    } else {
        stroke_width.max(track_stroke_width) / 2.0
    }
}

