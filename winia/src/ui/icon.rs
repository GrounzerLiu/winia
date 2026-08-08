//! Icon 组件与矢量图标基础设施。
//!
//! 与 Compose `Icon` 的差异（有意设计）：
//! - 默认不内置任何图标（可变字体图标需启用 `material-symbols-*` feature）；
//! - 支持四种来源：SVG path 字符串、完整 SVG 文档字符串、图片文件路径、
//!   可变字体符号（feature 启用时）；
//! - `auto_mirror` 是显式属性（默认关），开启后跟随布局方向（RTL）镜像；
//! - 可变字体轴 FILL / GRAD / opsz / wght 支持动画（渲染期求值，只重绘不重组）。
//!
//! 所有 SVG 渲染统一走 Skia 内置 `svg::Dom`（不自研路径解析）：
//! 裸 path 数据会被包成最小 `<svg viewBox="0 0 24 24">` 文档后交给 Dom。

use crate::core::composer::{ComposeCtx, GroupStatus};
use crate::core::state::State;
use crate::layout::BoxLayout;
use crate::modifier::{Color, Modifier};
use crate::ui::theme::{ThemeColors, WiniaTheme};
use skia_safe::svg;
use std::collections::HashMap;
use std::cell::RefCell;
use std::time::{Duration, Instant};
use std::fmt;
use std::sync::{Arc, LazyLock};
use parking_lot::Mutex;

/// 路径填充规则（写入包裹 SVG 的 fill-rule 属性——不自行解析路径）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PathFillType {
    NonZero,
    EvenOdd,
}

// ─────────────────────────────────────────────────────────────────────────
// 可变轴值（静态 / 动态——动画只重绘不重组，与 SizeValue 同一模式）
// ─────────────────────────────────────────────────────────────────────────

/// 可变字体轴取值：静态 f32 或动态（`&State<f32>` / 闭包，渲染期求值）
#[derive(Clone)]
pub enum AxisValue {
    Static(f32),
    Dynamic(Arc<dyn Fn() -> f32 + Send + Sync>),
}

impl AxisValue {
    pub fn eval(&self) -> f32 {
        match self {
            AxisValue::Static(v) => *v,
            AxisValue::Dynamic(f) => f(),
        }
    }

    fn static_eq(&self, other: &AxisValue) -> bool {
        match (self, other) {
            (AxisValue::Static(a), AxisValue::Static(b)) => a == b,
            // 动态值视为相等（渲染期求值——动画不触发重组/Skip）
            _ => true,
        }
    }
}

impl fmt::Debug for AxisValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AxisValue::Static(v) => f.debug_tuple("Static").field(v).finish(),
            AxisValue::Dynamic(_) => f.write_str("Dynamic(..)"),
        }
    }
}

impl From<f32> for AxisValue {
    fn from(v: f32) -> Self {
        AxisValue::Static(v)
    }
}

impl From<&State<f32>> for AxisValue {
    fn from(s: &State<f32>) -> Self {
        let s = s.clone();
        AxisValue::Dynamic(Arc::new(move || s.get()))
    }
}

impl From<State<f32>> for AxisValue {
    fn from(s: State<f32>) -> Self {
        AxisValue::Dynamic(Arc::new(move || s.get()))
    }
}

impl<F> From<F> for AxisValue
where
    F: Fn() -> f32 + Send + Sync + 'static,
{
    fn from(f: F) -> Self {
        AxisValue::Dynamic(Arc::new(f))
    }
}

/// Material Symbols 可变轴（官方范围）：
/// FILL 0..=1、GRAD -50..=200、opsz 20..=48、wght 100..=700
#[derive(Debug, Clone)]
pub struct SymbolAxes {
    pub fill: AxisValue,
    pub grad: AxisValue,
    pub opsz: AxisValue,
    pub wght: AxisValue,
}

pub const FILL_RANGE: (f32, f32) = (0.0, 1.0);
pub const GRAD_RANGE: (f32, f32) = (-50.0, 200.0);
pub const OPSZ_RANGE: (f32, f32) = (20.0, 48.0);
pub const WGHT_RANGE: (f32, f32) = (100.0, 700.0);

impl Default for SymbolAxes {
    fn default() -> Self {
        Self {
            fill: AxisValue::Static(0.0),
            grad: AxisValue::Static(0.0),
            opsz: AxisValue::Static(24.0),
            wght: AxisValue::Static(400.0),
        }
    }
}

impl PartialEq for SymbolAxes {
    fn eq(&self, other: &Self) -> bool {
        self.fill.static_eq(&other.fill)
            && self.grad.static_eq(&other.grad)
            && self.opsz.static_eq(&other.opsz)
            && self.wght.static_eq(&other.wght)
    }
}

impl SymbolAxes {
    /// 渲染期求值并按官方范围钳制
    pub fn evaluated(&self) -> (f32, f32, f32, f32) {
        // NaN 防御：clamp(NaN) 返回 NaN，会污染缓存键与字体轴
        let clamp = |v: f32, (lo, hi): (f32, f32)| if v.is_nan() { lo } else { v.clamp(lo, hi) };
        (
            clamp(self.fill.eval(), FILL_RANGE),
            clamp(self.grad.eval(), GRAD_RANGE),
            clamp(self.opsz.eval(), OPSZ_RANGE),
            clamp(self.wght.eval(), WGHT_RANGE),
        )
    }
}

// ─────────────────────────────────────────────────────────────────────────
// 图标来源
// ─────────────────────────────────────────────────────────────────────────

/// 图标来源——默认不内置任何图标；SVG path / SVG 文档 / 图片文件始终可用，
/// 可变字体符号需启用 `material-symbols-*` feature。
#[derive(Debug, Clone, PartialEq)]
pub enum IconSource {
    /// 原始 SVG path 数据（fonts.google.com/icons 复制）——渲染时包成
    /// `<svg viewBox="0 0 24 24">` 文档交给 Skia `svg::Dom`，不自研解析
    SvgPath { data: Arc<str>, fill_type: PathFillType },
    /// 完整 SVG 文档字符串（`<svg viewBox=...>...</svg>`）
    Svg(Arc<str>),
    /// 图片文件路径（png/jpg/jpeg/webp/bmp/svg，按扩展名解码）
    File(Arc<str>),
    /// Material Symbols 可变字体符号
    #[cfg(any(
        feature = "material-symbols-outlined",
        feature = "material-symbols-rounded",
        feature = "material-symbols-sharp"
    ))]
    Symbol(crate::icon::MaterialSymbol),
}

impl IconSource {
    pub fn svg_path(data: impl Into<Arc<str>>) -> Self {
        IconSource::SvgPath { data: data.into(), fill_type: PathFillType::NonZero }
    }

    pub fn svg(data: impl Into<Arc<str>>) -> Self {
        IconSource::Svg(data.into())
    }

    pub fn file(path: impl Into<Arc<str>>) -> Self {
        IconSource::File(path.into())
    }

    #[cfg(any(
        feature = "material-symbols-outlined",
        feature = "material-symbols-rounded",
        feature = "material-symbols-sharp"
    ))]
    pub fn symbol(symbol: crate::icon::MaterialSymbol) -> Self {
        IconSource::Symbol(symbol)
    }

    /// 单色源（SvgPath / Svg / Symbol）默认染主题色；File 默认保留原色
    pub(crate) fn default_tinted(&self) -> bool {
        !matches!(self, IconSource::File(_))
    }

    /// 固有尺寸（无则回退 24×24）：SVG 文档/文件解析 viewBox，位图取像素尺寸
    pub(crate) fn intrinsic_size(&self) -> Option<(f32, f32)> {
        match self {
            IconSource::SvgPath { .. } => Some((24.0, 24.0)),
            IconSource::Svg(data) => parse_view_box(data),
            IconSource::File(path) => file_intrinsic_size(path),
            #[cfg(any(
                feature = "material-symbols-outlined",
                feature = "material-symbols-rounded",
                feature = "material-symbols-sharp"
            ))]
            IconSource::Symbol(_) => Some((24.0, 24.0)),
        }
    }
}

/// 把裸 path 数据包成最小 SVG 文档（Skia svg::Dom 要求完整文档）
pub(crate) fn wrap_svg_path(data: &str, fill_type: PathFillType) -> String {
    let rule = match fill_type {
        PathFillType::NonZero => "nonzero",
        PathFillType::EvenOdd => "evenodd",
    };
    // path 数据来自公开 API——转义 XML 特殊字符，防止破坏文档
    let escaped = data
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('"', "&quot;");
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="{escaped}" fill-rule="{rule}"/></svg>"#
    )
}

// ─────────────────────────────────────────────────────────────────────────
// Tint（三态：Auto 单色源染主题色 / 多色源不染；可显式覆盖）
// ─────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tint {
    /// 单色源（SvgPath/Svg/Symbol）染主题 on_surface；File 保留原色
    Auto,
    Color(Color),
    None,
}

impl From<Color> for Tint {
    fn from(c: Color) -> Self {
        Tint::Color(c)
    }
}

impl From<Option<Color>> for Tint {
    fn from(o: Option<Color>) -> Self {
        match o {
            Some(c) => Tint::Color(c),
            None => Tint::None,
        }
    }
}

impl Tint {
    /// Auto：单色源染当前内容色（默认主题 on_surface），File 保留原色
    fn resolve(self, source: &IconSource, default_content: Color) -> Option<Color> {
        match self {
            Tint::Auto => source.default_tinted().then_some(default_content),
            Tint::Color(c) => Some(c),
            Tint::None => None,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────
// 渲染规格（进入 ModifierElement::DrawIcon）
// ─────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct IconSpec {
    pub source: IconSource,
    pub tint: Option<Color>,
    pub auto_mirror: bool,
    pub axes: SymbolAxes,
}

impl PartialEq for IconSpec {
    fn eq(&self, other: &Self) -> bool {
        self.source == other.source
            && self.tint == other.tint
            && self.auto_mirror == other.auto_mirror
            && self.axes == other.axes
    }
}

// ─────────────────────────────────────────────────────────────────────────
// 全局缓存：path 解析 / SVG 与图片解码 / 字形 blob
// ─────────────────────────────────────────────────────────────────────────

/// 解码后的图标内容（供渲染层绘制）。
///
/// 不变式：解码后**绘制期只读**——SVG 容器尺寸在解码期固定，
/// 绘制按目标尺寸重设容器后 `render`。`svg::Dom` 未保证线程安全，
/// 因此解码缓存是线程本地的（渲染线程内创建与使用），不跨线程共享。
pub(crate) enum DecodedIcon {
    Bitmap { image: skia_safe::Image, width: f32, height: f32 },
    Svg { dom: RefCell<svg::Dom>, width: f32, height: f32 },
}

/// 解码缓存键（SvgPath 按数据+填充规则区分）
#[derive(Hash, PartialEq, Eq, Clone)]
enum DecodeKey {
    SvgPath(Arc<str>, PathFillType),
    Doc(Arc<str>),
    File(Arc<str>),
}

/// 解码缓存 TTL：成功/失败都 5 秒刷新——开发期改文件能生效，
/// 失败也不会每帧重读磁盘
const CACHE_TTL: Duration = Duration::from_secs(5);

thread_local! {
    static DECODED_CACHE: RefCell<HashMap<DecodeKey, (Instant, Option<Arc<DecodedIcon>>)>> =
        RefCell::new(HashMap::new());
}

pub(crate) fn decoded_icon(source: &IconSource) -> Option<Arc<DecodedIcon>> {
    let key = match source {
        IconSource::SvgPath { data, fill_type } => DecodeKey::SvgPath(data.clone(), *fill_type),
        IconSource::Svg(data) => DecodeKey::Doc(data.clone()),
        IconSource::File(path) => DecodeKey::File(path.clone()),
        _ => return None,
    };
    DECODED_CACHE.with(|cell| {
        let mut cache = cell.borrow_mut();
        if let Some((at, v)) = cache.get(&key) {
            if at.elapsed() < CACHE_TTL {
                return v.clone();
            }
        }
        let decoded = match source {
            IconSource::SvgPath { data, fill_type } => {
                decode_svg_str(&wrap_svg_path(data, *fill_type))
            }
            IconSource::Svg(data) => decode_svg_str(data),
            IconSource::File(path) => decode_file(path),
            _ => None,
        };
        // 成功与失败都缓存（失败带 TTL——避免每帧重读，但会过期重试）
        let entry = (Instant::now(), decoded.map(Arc::new));
        let result = entry.1.clone();
        cache.insert(key, entry);
        result
    })
}

fn decode_svg_str(data: &str) -> Option<DecodedIcon> {
    let (w, h) = parse_view_box(data).unwrap_or((24.0, 24.0));
    let mut dom = svg::Dom::from_str(data, skia_safe::FontMgr::default()).ok()?;
    dom.set_container_size((w, h));
    Some(DecodedIcon::Svg { dom: RefCell::new(dom), width: w, height: h })
}

fn decode_file(path: &str) -> Option<DecodedIcon> {
    let data = std::fs::read(path).ok()?;
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    if ext == "svg" {
        let text = String::from_utf8(data).ok()?;
        return decode_svg_str(&text);
    }
    if matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "webp" | "bmp") {
        let image = skia_safe::Image::from_encoded(skia_safe::Data::new_copy(&data))?;
        let (w, h) = (image.width() as f32, image.height() as f32);
        return Some(DecodedIcon::Bitmap { image, width: w, height: h });
    }
    None
}

fn file_intrinsic_size(path: &str) -> Option<(f32, f32)> {
    static FILE_SIZE_CACHE: LazyLock<parking_lot::Mutex<HashMap<String, (Instant, Option<(f32, f32)>)>>> =
        LazyLock::new(|| parking_lot::Mutex::new(HashMap::new()));
    let mut cache = FILE_SIZE_CACHE.lock();
    if let Some((at, v)) = cache.get(path) {
        if at.elapsed() < CACHE_TTL {
            return *v;
        }
    }
    let size = match decoded_icon(&IconSource::File(Arc::from(path))) {
        Some(d) => match d.as_ref() {
            DecodedIcon::Bitmap { width, height, .. } => Some((*width, *height)),
            DecodedIcon::Svg { width, height, .. } => Some((*width, *height)),
        },
        None => None,
    };
    cache.insert(path.to_string(), (Instant::now(), size));
    size
}

/// 从 SVG 字符串提取固有尺寸：优先 `width`/`height` 属性（SVG 规范中它们
/// 决定布局尺寸，viewBox 只是坐标系统）；没有则回退 viewBox 的第 3、4 个值。
pub fn parse_view_box(svg: &str) -> Option<(f32, f32)> {
    let tag = svg_open_tag(svg)?;
    if let (Some(w), Some(h)) = (parse_svg_attr_len(tag, "width"), parse_svg_attr_len(tag, "height")) {
        return Some((w, h));
    }
    let lower = tag.to_ascii_lowercase();
    let idx = find_attr(&lower, "viewbox")?;
    let rest = &tag[idx + "viewbox".len()..];
    let open = rest.find(['"', '\''])?;
    let quote = rest[open..].chars().next()?;
    let value = rest[open + 1..].split(quote).next()?;
    let nums: Vec<f32> = value
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter_map(|s| s.parse::<f32>().ok())
        .collect();
    if nums.len() >= 4 {
        Some((nums[2], nums[3]))
    } else {
        None
    }
}

/// 截取 `<svg ...>` 开标签（属性只在开标签内解析，避免误匹配文档其它位置）
fn svg_open_tag(svg: &str) -> Option<&str> {
    let lower = svg.to_ascii_lowercase();
    let start = lower.find("<svg")?;
    let end = lower[start..].find('>')? + start;
    Some(&svg[start..end])
}

/// 解析开标签内属性的长度（`24`、`24px` 等；百分比不支持，返回 None）
fn parse_svg_attr_len(tag: &str, name: &str) -> Option<f32> {
    let lower = tag.to_ascii_lowercase();
    let idx = find_attr(&lower, name)?;
    let rest = &tag[idx + name.len()..];
    let eq = rest.find('=')?;
    let after = &rest[eq + 1..].trim_start();
    let quote = after.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let value = after[quote.len_utf8()..].split(quote).next()?;
    let num: String = value.chars().take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-').collect();
    num.parse::<f32>().ok()
}

/// 在开标签内查找 `name=`，要求属性名前是空白/引号/标签开头——
/// 避免 `stroke-width=` 误命中 `width=`、路径文本误命中 `viewbox`
fn find_attr(lower: &str, name: &str) -> Option<usize> {
    let pat = format!("{name}=");
    let mut search = 0usize;
    while let Some(rel) = lower[search..].find(&pat) {
        let idx = search + rel;
        let boundary_ok = idx == 0
            || matches!(
                lower.as_bytes()[idx - 1],
                b' ' | b'\t' | b'\n' | b'\r' | b'"' | b'\''
            );
        if boundary_ok {
            return Some(idx);
        }
        search = idx + pat.len();
    }
    None
}

#[cfg(any(
    feature = "material-symbols-outlined",
    feature = "material-symbols-rounded",
    feature = "material-symbols-sharp"
))]
static BLOB_CACHE: LazyLock<Mutex<HashMap<BlobKey, Option<Arc<skia_safe::TextBlob>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[cfg(any(
    feature = "material-symbols-outlined",
    feature = "material-symbols-rounded",
    feature = "material-symbols-sharp"
))]
#[derive(Hash, PartialEq, Eq)]
struct BlobKey {
    symbol: crate::icon::MaterialSymbol,
    size: i32,
    fill: i32,
    grad: i32,
    opsz: i32,
    wght: i32,
}

/// 量化到 0.01：动画连续值命中同一 key（可变字体轴实际离散），
/// 避免每帧 to_bits 精确键 miss 重建 TextBlob
fn quant_axis(v: f32) -> i32 {
    (v * 100.0).round() as i32
}

#[cfg(any(
    feature = "material-symbols-outlined",
    feature = "material-symbols-rounded",
    feature = "material-symbols-sharp"
))]
pub(crate) fn symbol_blob(
    symbol: crate::icon::MaterialSymbol,
    size: f32,
    axes: &SymbolAxes,
) -> Option<Arc<skia_safe::TextBlob>> {
    let (fill, grad, opsz, wght) = axes.evaluated();
    let key = BlobKey {
        symbol,
        size: quant_axis(size),
        fill: quant_axis(fill),
        grad: quant_axis(grad),
        opsz: quant_axis(opsz),
        wght: quant_axis(wght),
    };
    let mut cache = BLOB_CACHE.lock();
    if let Some(b) = cache.get(&key) {
        return b.clone();
    }
    let blob = crate::icon::symbol_text_blob(symbol, size, fill, grad, opsz, wght).map(Arc::new);
    cache.insert(key, blob.clone());
    // 动画轴组合可能无限增长——简单上限，超限清空重建
    if cache.len() > 512 {
        cache.clear();
    }
    blob
}

// ─────────────────────────────────────────────────────────────────────────
// Icon 组件
// ─────────────────────────────────────────────────────────────────────────

/// Icon 组件——多来源、tint、autoMirror、可变轴（feature 启用时）。
///
/// ```ignore
/// Icon::svg_path("M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6v2z")
///     .tint(theme.primary)
///     .build(ctx);
/// ```
pub struct Icon {
    source: IconSource,
    tint: Tint,
    auto_mirror: bool,
    size: Option<f32>,
    content_description: Option<String>,
    axes: SymbolAxes,
    modifier: Modifier,
}

impl Icon {
    pub fn new(source: impl Into<IconSource>) -> Self {
        Self {
            source: source.into(),
            tint: Tint::Auto,
            auto_mirror: false,
            size: None,
            content_description: None,
            axes: SymbolAxes::default(),
            modifier: Modifier::new(),
        }
    }

    pub fn svg_path(data: impl Into<Arc<str>>) -> Self {
        Self::new(IconSource::svg_path(data))
    }

    pub fn svg(data: impl Into<Arc<str>>) -> Self {
        Self::new(IconSource::svg(data))
    }

    pub fn file(path: impl Into<Arc<str>>) -> Self {
        Self::new(IconSource::file(path))
    }

    #[cfg(any(
        feature = "material-symbols-outlined",
        feature = "material-symbols-rounded",
        feature = "material-symbols-sharp"
    ))]
    pub fn symbol(symbol: crate::icon::MaterialSymbol) -> Self {
        Self::new(IconSource::symbol(symbol))
    }

    pub fn tint(mut self, tint: impl Into<Tint>) -> Self {
        self.tint = tint.into();
        self
    }

    /// 开启后图标跟随布局方向（RTL 时水平镜像）。默认 false。
    pub fn auto_mirror(mut self, enabled: bool) -> Self {
        self.auto_mirror = enabled;
        self
    }

    /// 覆盖尺寸（默认按源固有尺寸，失败回退 24×24）
    pub fn size(mut self, size: f32) -> Self {
        self.size = Some(size);
        self
    }

    /// 无障碍描述（当前存储；semantics 树实现后接入）
    pub fn content_description(mut self, desc: impl Into<String>) -> Self {
        self.content_description = Some(desc.into());
        self
    }

    pub fn content_description_value(&self) -> Option<&str> {
        self.content_description.as_deref()
    }

    #[cfg(any(
        feature = "material-symbols-outlined",
        feature = "material-symbols-rounded",
        feature = "material-symbols-sharp"
    ))]
    pub fn fill(mut self, v: impl Into<AxisValue>) -> Self {
        self.axes.fill = v.into();
        self
    }

    #[cfg(any(
        feature = "material-symbols-outlined",
        feature = "material-symbols-rounded",
        feature = "material-symbols-sharp"
    ))]
    pub fn grad(mut self, v: impl Into<AxisValue>) -> Self {
        self.axes.grad = v.into();
        self
    }

    #[cfg(any(
        feature = "material-symbols-outlined",
        feature = "material-symbols-rounded",
        feature = "material-symbols-sharp"
    ))]
    pub fn opsz(mut self, v: impl Into<AxisValue>) -> Self {
        self.axes.opsz = v.into();
        self
    }

    #[cfg(any(
        feature = "material-symbols-outlined",
        feature = "material-symbols-rounded",
        feature = "material-symbols-sharp"
    ))]
    pub fn wght(mut self, v: impl Into<AxisValue>) -> Self {
        self.axes.wght = v.into();
        self
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    pub fn build(self, ctx: &mut ComposeCtx) {
        ctx.changed(&self.source);
        ctx.changed(&self.tint);
        ctx.changed(&self.auto_mirror);
        ctx.changed(&self.axes);
        let tint = self.tint.resolve(&self.source, WiniaTheme::content_color());
        let (w, h) = self
            .size
            .map(|s| (s, s))
            .or_else(|| self.source.intrinsic_size())
            .unwrap_or((24.0, 24.0));
        let spec = IconSpec {
            source: self.source,
            tint,
            auto_mirror: self.auto_mirror,
            axes: self.axes,
        };
        let modifier = Modifier::new().size(w, h).draw_icon(spec).then(self.modifier);
        let key = ctx.next_key();
        match ctx.start_restartable_group(key, modifier, BoxLayout::new().alignment(crate::layout::Alignment::Center)) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {}
        }
        ctx.end_restartable_group();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_svg_path_is_valid_document() {
        let wrapped = wrap_svg_path("M19 13h-6v6z", PathFillType::NonZero);
        assert!(wrapped.starts_with("<svg"));
        assert!(wrapped.contains("M19 13h-6v6z"));
        assert!(wrapped.contains("fill-rule=\"nonzero\""));
        assert_eq!(parse_view_box(&wrapped), Some((24.0, 24.0)));
    }

    #[test]
    fn axis_clamps_to_official_ranges() {
        let axes = SymbolAxes {
            fill: AxisValue::Static(5.0),
            grad: AxisValue::Static(-999.0),
            opsz: AxisValue::Static(2.0),
            wght: AxisValue::Static(9999.0),
        };
        assert_eq!(axes.evaluated(), (1.0, -50.0, 20.0, 700.0));
    }

    #[test]
    fn tint_auto_resolves_by_source() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let path_src = IconSource::svg_path("M0 0h1z");
        assert_eq!(Tint::Auto.resolve(&path_src, theme.on_surface), Some(theme.on_surface));
        let file_src = IconSource::file("logo.png");
        assert_eq!(Tint::Auto.resolve(&file_src, theme.on_surface), None, "File 默认保留原色");
        assert_eq!(Tint::None.resolve(&path_src, theme.on_surface), None);
        assert_eq!(Tint::Color(Color::RED).resolve(&file_src, theme.on_surface), Some(Color::RED));
    }

    #[test]
    fn view_box_parsing() {
        assert_eq!(
            parse_view_box("<svg viewBox='0 0 24 24'><path d='M0 0'/></svg>"),
            Some((24.0, 24.0))
        );
        // width/height 属性优先于 viewBox（布局尺寸语义）
        assert_eq!(
            parse_view_box("<svg width='32' height='32' viewBox='0 0 960 960'></svg>"),
            Some((32.0, 32.0))
        );
        assert_eq!(parse_view_box("<svg width='24px' height='24px'></svg>"), Some((24.0, 24.0)));
        assert_eq!(parse_view_box("<svg></svg>"), None);
    }
}
