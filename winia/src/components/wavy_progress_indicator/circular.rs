use super::*;

// ═══════════════════════════════════════════════════════
// CircularWavyProgressIndicator
// ═══════════════════════════════════════════════════════

/// 圆形 wavy 进度指示器（对标 Compose `CircularWavyProgressIndicator`）。
///
/// determinate：`CircularWavyProgressIndicator::new(progress)`；
/// indeterminate：`CircularWavyProgressIndicator::indeterminate()`。
#[derive(Clone)]
pub struct CircularWavyProgressIndicator {
    pub(crate) progress: f32,
    pub(crate) indeterminate: bool,
    pub(crate) modifier: Modifier,
    pub(crate) color: Option<Color>,
    pub(crate) track_color: Option<Color>,
    pub(crate) stroke_width: f32,
    pub(crate) track_stroke_width: f32,
    pub(crate) gap_size: f32,
    pub(crate) amplitude: WavyAmplitude,
    pub(crate) wavelength: f32,
    pub(crate) wave_speed: f32,
}

impl CircularWavyProgressIndicator {
    /// determinate 构造（progress 0..1）
    pub fn new(progress: f32) -> Self {
        Self {
            progress,
            indeterminate: false,
            modifier: Modifier::new(),
            color: None,
            track_color: None,
            stroke_width: WavyProgressIndicatorDefaults::circular_stroke_width(),
            track_stroke_width: WavyProgressIndicatorDefaults::circular_track_stroke_width(),
            gap_size: WAVY_GAP_SIZE,
            amplitude: WavyAmplitude::Indicator,
            wavelength: WavyProgressIndicatorDefaults::circular_wavelength(),
            wave_speed: WavyProgressIndicatorDefaults::circular_wavelength(),
        }
    }

    /// indeterminate 构造：无限旋转 + 进度呼吸 + wave 滚动
    pub fn indeterminate() -> Self {
        Self::new(0.0).indeterminate_mode()
    }

    pub(crate) fn indeterminate_mode(mut self) -> Self {
        self.indeterminate = true;
        self.amplitude = WavyAmplitude::Fixed(1.0);
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

    /// 轨道颜色（默认 SecondaryContainer；indeterminate 也会绘制 track，与 Compose 一致）
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

    /// 自定义振幅函数（determinate）
    pub fn amplitude_fn(mut self, f: impl Fn(f32) -> f32 + Send + Sync + 'static) -> Self {
        self.amplitude = WavyAmplitude::Custom(Arc::new(f));
        self
    }

    /// 固定振幅（indeterminate）
    pub fn amplitude(mut self, value: f32) -> Self {
        self.amplitude = WavyAmplitude::Fixed(value);
        self
    }

    /// 波长（dp，默认 15）
    pub fn wavelength(mut self, value: f32) -> Self {
        self.wavelength = value.max(0.0);
        self
    }

    /// 波速（dp/s，默认 = wavelength）
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
        let wavelength = self.wavelength;
        let wave_speed = self.wave_speed;
        let cap = ProgressIndicatorStrokeCap::Round;
        let enable_motion = wave_speed > 0.0 && wavelength > 0.0;

        let shapes_cache = Arc::new(Mutex::new(CircularShapesCache::default()));

        let m = if self.indeterminate {
            let amplitude = self.amplitude.fixed();
            let duration_ms =
                circular_wave_duration_ms(wavelength, wave_speed, WAVY_CIRCULAR_SIZE, stroke_width);
            let mut inf = ctx.remember_infinite_transition();
            let wave_offset = inf.animate_float_preserving(
                ctx,
                0.0,
                1.0,
                wave_animation_spec_duration(duration_ms),
            );
            if !enable_motion {
                crate::animation::remove_animation_by_state(wave_offset.state_id());
            }
            let global = inf.animate_float(
                ctx,
                0.0,
                1080.0,
                progress_indicator::circular_global_rotation_spec(),
            );
            let additional = inf.animate_float(
                ctx,
                0.0,
                360.0,
                progress_indicator::circular_additional_rotation_spec(),
            );
            let progress_anim =
                inf.animate_float(ctx, 0.1, 0.87, progress_indicator::circular_progress_spec());
            Modifier::new()
                .size(WAVY_CIRCULAR_SIZE, WAVY_CIRCULAR_SIZE)
                .draw_node(CircularWavyIndeterminateNode {
                    color, track_color, stroke_width, track_stroke_width,
                    cap, gap_size, amplitude, wavelength, enable_motion,
                    wave_offset: wave_offset.clone(),
                    global: global.clone(),
                    additional: additional.clone(),
                    progress_anim: progress_anim.clone(),
                    cache: shapes_cache.clone(),
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
            let duration_ms =
                circular_wave_duration_ms(wavelength, wave_speed, WAVY_CIRCULAR_SIZE, stroke_width);
            let mut inf = ctx.remember_infinite_transition();
            let wave_offset = inf.animate_float_preserving(
                ctx,
                0.0,
                1.0,
                wave_animation_spec_duration(duration_ms),
            );
            // Compose 仅在真正画波时运行 wave offset 动画（同 Linear）。
            let wave_active = enable_motion && (target > 0.0 || current > 0.0);
            if !wave_active {
                crate::animation::remove_animation_by_state(wave_offset.state_id());
            }

            let wave_for_draw = wave_offset.clone();
            if target == 0.0 && current != 0.0 {
                // 同 Linear：done 回调仅作兜底，重组分支也会移除 wave 动画。
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

            Modifier::new()
                .size(WAVY_CIRCULAR_SIZE, WAVY_CIRCULAR_SIZE)
                .draw_node(CircularWavyDeterminateNode {
                    color, track_color, stroke_width, track_stroke_width,
                    cap, gap_size, progress, wavelength, enable_motion,
                    amplitude_token,
                    amplitude_state: amplitude_state.clone(),
                    wave_offset: wave_for_draw.clone(),
                    cache: shapes_cache.clone(),
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

/// Circular wave offset 动画时长（Compose：`(wavelength/waveSpeed)*1000*vertexCount`）。
pub(crate) fn circular_wave_duration_ms(
    wavelength: f32,
    wave_speed: f32,
    size: f32,
    stroke_width: f32,
) -> u64 {
    if wave_speed <= 0.0 || wavelength <= 0.0 {
        return MIN_WAVE_ANIMATION_MS;
    }
    let r = size / 2.0 - stroke_width / 2.0;
    let num_vertices =
        ((2.0 * PI * r / wavelength).round() as usize).max(MIN_CIRCULAR_VERTEX_COUNT);
    (((wavelength / wave_speed) * 1000.0) * num_vertices as f32)
        .round()
        .max(MIN_WAVE_ANIMATION_MS as f32) as u64
}

pub(crate) fn wave_animation_spec_duration(duration_ms: u64) -> InfiniteRepeatableSpec {
    InfiniteRepeatableSpec::restart_tween(
        Duration::from_millis(duration_ms),
        TweenSpec::new(
            Duration::from_millis(duration_ms),
            interpolator::Linear::new(),
        ),
    )
}

/// Circular determinate wavy 绘制。
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_circular_wavy_determinate(
    canvas: &skia_safe::Canvas,
    rect: skia_safe::Rect,
    color: Color,
    track_color: Color,
    stroke_width: f32,
    track_stroke_width: f32,
    cap: ProgressIndicatorStrokeCap,
    gap_size: f32,
    progress: f32,
    amplitude: f32,
    wavelength: f32,
    enable_motion: bool,
    wave_offset: f32,
    cache: &Mutex<CircularShapesCache>,
) {
    let progress = progress.clamp(0.0, 1.0);
    let mut shapes = cache.lock().unwrap_or_else(|e| e.into_inner());
    let (track_path, progress_path) = build_circular_wavy_paths(
        &mut shapes,
        rect,
        wavelength,
        stroke_width,
        track_stroke_width,
        cap,
        gap_size,
        0.0,
        progress,
        amplitude,
        wave_offset,
        enable_motion,
    );
    canvas.save();
    canvas.translate((rect.left, rect.top));
    draw_circular_wavy_paths(
        canvas,
        rect,
        &track_path,
        &progress_path,
        color,
        track_color,
        stroke_width,
        track_stroke_width,
        cap,
    );
    canvas.restore();
}

/// Circular indeterminate wavy 绘制。
#[allow(clippy::too_many_arguments)]
/// Circular indeterminate/determinate wavy 节点。
pub(crate) struct CircularWavyIndeterminateNode {
    pub(crate) color: Color,
    pub(crate) track_color: Color,
    pub(crate) stroke_width: f32,
    pub(crate) track_stroke_width: f32,
    pub(crate) cap: ProgressIndicatorStrokeCap,
    pub(crate) gap_size: f32,
    pub(crate) amplitude: f32,
    pub(crate) wavelength: f32,
    pub(crate) enable_motion: bool,
    pub(crate) wave_offset: crate::runtime::state::Visual<f32>,
    pub(crate) global: crate::runtime::state::Visual<f32>,
    pub(crate) additional: crate::runtime::state::Visual<f32>,
    pub(crate) progress_anim: crate::runtime::state::Visual<f32>,
    pub(crate) cache: Arc<Mutex<CircularShapesCache>>,
}

impl std::fmt::Debug for CircularWavyIndeterminateNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CircularWavyIndeterminateNode")
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

impl crate::modifier::DrawNode for CircularWavyIndeterminateNode {
    fn draw(&self, canvas: &skia_safe::Canvas, rect: skia_safe::Rect) {
        draw_circular_wavy_indeterminate(
            canvas, rect,
            self.color, self.track_color,
            self.stroke_width, self.track_stroke_width,
            self.cap, self.gap_size,
            self.amplitude, self.wavelength, self.enable_motion,
            self.wave_offset.peek(),
            self.global.peek(), self.additional.peek(), self.progress_anim.peek(),
            &self.cache,
        );
    }
    fn node_key(&self) -> String {
        format!(
            "wavy-circular-indet:{:?}:{:?}:{}:{}:{}:{}:{}:{}:{}",
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

pub(crate) struct CircularWavyDeterminateNode {
    pub(crate) color: Color,
    pub(crate) track_color: Color,
    pub(crate) stroke_width: f32,
    pub(crate) track_stroke_width: f32,
    pub(crate) cap: ProgressIndicatorStrokeCap,
    pub(crate) gap_size: f32,
    pub(crate) progress: f32,
    pub(crate) wavelength: f32,
    pub(crate) enable_motion: bool,
    pub(crate) amplitude_token: (u8, u64, usize),
    pub(crate) amplitude_state: State<f32>,
    pub(crate) wave_offset: crate::runtime::state::Visual<f32>,
    pub(crate) cache: Arc<Mutex<CircularShapesCache>>,
}

impl std::fmt::Debug for CircularWavyDeterminateNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CircularWavyDeterminateNode")
            .field("color", &self.color)
            .field("track_color", &self.track_color)
            .field("stroke_width", &self.stroke_width)
            .field("track_stroke_width", &self.track_stroke_width)
            .field("cap", &self.cap)
            .field("gap_size", &self.gap_size)
            .field("progress", &self.progress)
            .field("wavelength", &self.wavelength)
            .field("enable_motion", &self.enable_motion)
            .field("amplitude_token", &self.amplitude_token)
            .finish_non_exhaustive()
    }
}

impl crate::modifier::DrawNode for CircularWavyDeterminateNode {
    fn draw(&self, canvas: &skia_safe::Canvas, rect: skia_safe::Rect) {
        draw_circular_wavy_determinate(
            canvas, rect,
            self.color, self.track_color,
            self.stroke_width, self.track_stroke_width,
            self.cap, self.gap_size,
            self.progress,
            self.amplitude_state.peek(),
            self.wavelength, self.enable_motion,
            self.wave_offset.peek(),
            &self.cache,
        );
    }
    fn node_key(&self) -> String {
        format!(
            "wavy-circular-det:{:?}:{:?}:{}:{}:{}:{}:{}:{}:{}:{:?}",
            self.color,
            self.track_color,
            self.stroke_width.to_bits(),
            self.track_stroke_width.to_bits(),
            self.cap as u8,
            self.gap_size.to_bits(),
            self.progress.to_bits(),
            self.wavelength.to_bits(),
            self.enable_motion,
            self.amplitude_token,
        )
    }
}

pub(crate) fn draw_circular_wavy_indeterminate(
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
    wave_offset: f32,
    global_rotation: f32,
    additional_rotation: f32,
    progress: f32,
    cache: &Mutex<CircularShapesCache>,
) {
    let mut shapes = cache.lock().unwrap_or_else(|e| e.into_inner());
    let (track_path, progress_path) = build_circular_wavy_paths(
        &mut shapes,
        rect,
        wavelength,
        stroke_width,
        track_stroke_width,
        cap,
        gap_size,
        0.0,
        progress,
        amplitude,
        wave_offset,
        enable_motion,
    );
    let cx = rect.width() / 2.0;
    let cy = rect.height() / 2.0;
    canvas.save();
    canvas.translate((rect.left, rect.top));
    // 保持与 Compose 参考一致：indeterminate 在 start_angle=270°（12 点）基础上
    // 再叠加 +90°，因此初始基准方向为右侧（3 点）；determinate 不叠加，起点在 12 点。
    canvas.rotate(
        global_rotation + additional_rotation + 90.0,
        Some(skia_safe::Point::new(cx, cy)),
    );
    draw_circular_wavy_paths(
        canvas,
        rect,
        &track_path,
        &progress_path,
        color,
        track_color,
        stroke_width,
        track_stroke_width,
        cap,
    );
    canvas.restore();
}

/// 构造 Circular wavy 的 track 与 progress 路径。
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_circular_wavy_paths(
    shapes: &mut CircularShapesCache,
    rect: skia_safe::Rect,
    wavelength: f32,
    stroke_width: f32,
    track_stroke_width: f32,
    cap: ProgressIndicatorStrokeCap,
    gap_size: f32,
    start_progress: f32,
    end_progress: f32,
    amplitude: f32,
    wave_offset: f32,
    enable_motion: bool,
) -> (skia_safe::Path, skia_safe::Path) {
    let width = rect.width();
    let height = rect.height();
    if width <= 0.0 || height <= 0.0 || wavelength <= 0.0 {
        return (skia_safe::Path::new(), skia_safe::Path::new());
    }

    let cap_width = stroke_cap_width(cap, stroke_width, track_stroke_width, width, height);

    // 形状缓存（RoundedPolygon）
    shapes.update(width, height, wavelength, stroke_width);

    let mut full_progress_path = shapes.get_progress_path(amplitude, enable_motion);
    process_circular_path(&mut full_progress_path, width, height, stroke_width);

    let mut progress_measure = skia_safe::PathMeasure::new(&full_progress_path, true, None);
    let progress_path_length = if enable_motion {
        progress_measure.length() / 2.0
    } else {
        progress_measure.length()
    };

    let mut full_track_path = shapes.get_track_path();
    process_circular_path(&mut full_track_path, width, height, stroke_width);
    let mut track_measure = skia_safe::PathMeasure::new(&full_track_path, true, None);
    let track_path_length = track_measure.length();

    let p_start = start_progress * progress_path_length;
    let p_stop = end_progress * progress_path_length;

    let track_gap_size = p_stop.min(gap_size);
    let horizontal_insets = p_stop.min(cap_width);
    let track_spacing = horizontal_insets * 2.0 + track_gap_size;

    let mut progress_builder = skia_safe::PathBuilder::new();
    if enable_motion {
        let coerced_wave_offset = wave_offset.rem_euclid(1.0);
        let shift = coerced_wave_offset * progress_path_length;
        progress_measure.get_segment(p_start + shift, p_stop + shift, &mut progress_builder, true);
        let mut progress_path = progress_builder.detach();
        let offset_angle = (coerced_wave_offset * 360.0) % 360.0;
        if offset_angle != 0.0 {
            let bounds = *full_progress_path.bounds();
            let center = bounds.center();
            progress_path = progress_path.make_offset((-center.x, -center.y));
            let matrix = skia_safe::Matrix::rotate_deg(-offset_angle);
            progress_path = progress_path.make_transform(&matrix);
            progress_path = progress_path.make_offset((center.x, center.y));
        }
        progress_builder = skia_safe::PathBuilder::new();
        progress_builder.add_path(&progress_path, None);
    } else {
        progress_measure.get_segment(p_start, p_stop, &mut progress_builder, true);
    }
    let progress_path = progress_builder.detach();

    let mut track_builder = skia_safe::PathBuilder::new();
    if track_path_length > 0.0 {
        let t_start = end_progress * track_path_length + track_spacing;
        let t_stop = track_path_length - track_spacing;
        track_measure.get_segment(t_start, t_stop, &mut track_builder, true);
    }
    let track_path = track_builder.detach();

    (track_path, progress_path)
}

pub(crate) fn process_circular_path(path: &mut skia_safe::Path, width: f32, height: f32, stroke_width: f32) {
    let scale_matrix = skia_safe::Matrix::scale((width - stroke_width, height - stroke_width));
    *path = path.make_transform(&scale_matrix);
    let bounds = *path.bounds();
    let dx = width / 2.0 - bounds.center_x();
    let dy = height / 2.0 - bounds.center_y();
    *path = path.make_offset((dx, dy));
    // `to_path(start_angle=270)` 已由 material-shapes 修正为真正旋转到 270°
    // （12 点方向），这里不再做额外旋转，避免 Morph 中间帧因动态旋转导致相位跳变。
}

pub(crate) fn draw_circular_wavy_paths(
    canvas: &skia_safe::Canvas,
    _rect: skia_safe::Rect,
    track_path: &skia_safe::Path,
    progress_path: &skia_safe::Path,
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
        canvas.draw_path(progress_path, &progress_paint);
    }
}

/// 缓存 Circular 使用的 RoundedPolygon（按 size/wavelength 重建）。
///
/// 额外缓存 `Morph::morph_match`：`Morph::new` 的 feature-mapping 是昂贵步骤，
/// 这里只做一次，之后每帧用 `Morph::from_morph_match` 廉价插值。
#[derive(Default)]
pub(crate) struct CircularShapesCache {
    pub(crate) size: (f32, f32),
    pub(crate) wavelength: f32,
    pub(crate) stroke_width: f32,
    pub(crate) vertex_count: usize,
    pub(crate) track_polygon: Option<RoundedPolygon>,
    pub(crate) active_polygon: Option<RoundedPolygon>,
    pub(crate) morph_match: Option<Vec<(Cubic, Cubic)>>,
}

impl CircularShapesCache {
    pub(crate) fn update(&mut self, width: f32, height: f32, wavelength: f32, stroke_width: f32) {
        if self.size == (width, height)
            && self.wavelength == wavelength
            && self.stroke_width == stroke_width
            && self.track_polygon.is_some()
            && self.active_polygon.is_some()
            && self.morph_match.is_some()
        {
            return;
        }
        let r = width.min(height) / 2.0 - stroke_width / 2.0;
        let num_vertices =
            ((2.0 * PI * r / wavelength).round() as usize).max(MIN_CIRCULAR_VERTEX_COUNT);

        let track = RoundedPolygon::circle(num_vertices, None, None, None).normalized();
        let active = RoundedPolygon::star(
            num_vertices,
            None,
            Some(0.75),
            Some(CornerRounding::new(0.35, Some(0.4))),
            Some(CornerRounding::new(0.5, None)),
            None,
            None,
            None,
        )
        .normalized();
        // 只做一次昂贵的 feature-mapping，缓存 morph_match 供每帧插值复用。
        let morph_match = Morph::new(&track, &active).morph_match().clone();

        self.size = (width, height);
        self.wavelength = wavelength;
        self.stroke_width = stroke_width;
        self.vertex_count = num_vertices;
        self.track_polygon = Some(track);
        self.active_polygon = Some(active);
        self.morph_match = Some(morph_match);
    }

    pub(crate) fn get_track_path(&self) -> skia_safe::Path {
        match &self.track_polygon {
            Some(p) => p.to_path(Some(270), Some(false), None),
            None => skia_safe::Path::new(),
        }
    }

    pub(crate) fn get_progress_path(&self, amplitude: f32, repeat_path: bool) -> skia_safe::Path {
        match (&self.track_polygon, &self.active_polygon, &self.morph_match) {
            (Some(track), Some(active), Some(morph_match)) => {
                if amplitude == 0.0 {
                    track.to_path(Some(270), Some(repeat_path), None)
                } else {
                    // 统一走 Morph（含 amplitude==1.0）：Compose 在创建 Morph 后也是
                    // 全部用 Morph.toPath，避免纯 star path 与 Morph 极限相位不一致
                    // 导致振幅接近 1 时波峰/波谷角度跳变。
                    let morph = Morph::from_morph_match(track, active, morph_match.clone());
                    morph.to_path(
                        amplitude,
                        Some(270),
                        Some(repeat_path),
                        None,
                        Some(0.5),
                        Some(0.5),
                    )
                }
            }
            _ => skia_safe::Path::new(),
        }
    }
}


