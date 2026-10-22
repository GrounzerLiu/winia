use super::*;

// ─────────────────────────────────────────────────────────────────────────
// Variable-font axes
// ─────────────────────────────────────────────────────────────────────────

/// A variable-font axis value: a literal, or a value evaluated per paint (a state or a closure).
///
/// An alias of [`Dyn`] rather than its own enum, so the "static or per-frame" rules — `eval` at
/// paint time, `same_parameter` for the element comparison — exist once. `AxisValue::Static(..)`
/// keeps working through the alias.
pub type AxisValue = Dyn<f32>;

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
        self.fill.same_parameter(&other.fill)
            && self.grad.same_parameter(&other.grad)
            && self.opsz.same_parameter(&other.opsz)
            && self.wght.same_parameter(&other.wght)
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

    /// True when the source carries no colour of its own, so `Tint::None` would leave it painted in the
    /// glyph's own default — BLACK — and therefore invisible on a dark surface. Only `Symbol` qualifies: it
    /// is a monochrome variable-font glyph. An SVG document and an image file both bring fills of their own,
    /// which is exactly what `Tint::None` exists to preserve for them.
    pub(crate) fn carries_no_colour(&self) -> bool {
        #[cfg(any(
            feature = "material-symbols-outlined",
            feature = "material-symbols-rounded",
            feature = "material-symbols-sharp"
        ))]
        {
            matches!(self, IconSource::Symbol(_))
        }
        #[cfg(not(any(
            feature = "material-symbols-outlined",
            feature = "material-symbols-rounded",
            feature = "material-symbols-sharp"
        )))]
        {
            let _ = self;
            false
        }
    }

    /// 固有尺寸：SvgPath/Symbol 默认 24×24；SVG 文档解析 viewBox（失败回退
    /// 24×24——与 `decode_svg_str` 回退一致，避免 Image 测量 0 尺寸空白而
    /// Icon 正常显示的不一致）；位图取像素尺寸（解码失败 None——调用方按 0 处理）
    pub(crate) fn intrinsic_size(&self) -> Option<(f32, f32)> {
        match self {
            IconSource::SvgPath { .. } => Some((24.0, 24.0)),
            IconSource::Svg(data) => parse_view_box(data).or(Some((24.0, 24.0))),
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
// 渲染规格（进入 ModifierElement::DrawIcon）
// ─────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct IconSpec {
    pub source: IconSource,
    /// The resolved tint. A literal is stored inline; a state or closure is read at paint time, so a
    /// glyph can follow an animated colour without recomposing.
    pub tint: Option<Dyn<Color>>,
    pub auto_mirror: bool,
    pub axes: SymbolAxes,
}

impl PartialEq for IconSpec {
    fn eq(&self, other: &Self) -> bool {
        self.source == other.source
            && option_same_parameter(&self.tint, &other.tint)
            && self.auto_mirror == other.auto_mirror
            && self.axes == other.axes
    }
}

/// Option lifted Dyn::same_parameter — the comparison every element that stores an optional carrier
/// needs (a literal-vs-state difference is a CHANGE; two states are the same request).
pub(crate) fn option_same_parameter<T: PartialEq>(
    a: &Option<Dyn<T>>,
    b: &Option<Dyn<T>>,
) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => a.same_parameter(b),
        _ => false,
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
        // A symbol has no bytes to decode: it is rasterised from the embedded variable font by
        // `symbol_blob`, so this cache has nothing to hold for it — and nothing to hand back.
        // Gated on the same features as the variant itself: without a symbol feature `IconSource` has no
        // `Symbol`, and an ungated arm here failed to compile for every build that left them off.
        #[cfg(any(
            feature = "material-symbols-outlined",
            feature = "material-symbols-rounded",
            feature = "material-symbols-sharp"
        ))]
        IconSource::Symbol(_) => return None,
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
            // Unreachable — the match above returns early for a symbol — but written out because `match`
            // must be exhaustive, and a catch-all here would silently absorb a future variant instead.
            #[cfg(any(
                feature = "material-symbols-outlined",
                feature = "material-symbols-rounded",
                feature = "material-symbols-sharp"
            ))]
            IconSource::Symbol(_) => None,
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
        // 预生成 mipmap 链：FilterQuality::Medium/High 的 MipmapMode 需要
        //（Skia 对无 mipmap 的 image 会退化为普通线性过滤）；Low 不受影响。
        // 解码缓存保证只生成一次，不随绘制重复计算。
        let image = image.with_default_mipmaps().unwrap_or(image);
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
    // A bare number, optionally with the `px` unit the doc above advertises. This used to collect the
    // leading digit-ish characters and parse those, so `width="100%"` came out as 100 and `width="2em"` as
    // 2 — an intrinsic size nobody asked for — while `24px`, which IS supported, worked by accident. Now
    // `px` is stripped explicitly and every other suffix is refused, which is what the doc promises.
    let digits = value.strip_suffix("px").unwrap_or(value);
    if digits.is_empty()
        || !digits
            .chars()
            .all(|c| c.is_ascii_digit() || c == '.' || c == '-' || c == '+')
    {
        return None;
    }
    digits.parse::<f32>().ok()
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
/// An `f32` axis value as the integer the blob cache keys on: hundredths. Fine enough that two visibly
/// different sizes never share a blob, coarse enough that float noise from an animation does not miss the
/// cache on every frame.
fn quant_axis(v: f32) -> i32 {
    (v * 100.0).round() as i32
}

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
    // `parking_lot::Mutex`, like `interaction.rs`'s ripple cache: its `lock()` hands back the guard
    // directly, with no poisoning to unwrap, which is what this call site was written against.
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

