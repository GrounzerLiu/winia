use super::*;

// Variable-font axes

/// A variable-font axis value: a literal, or a value evaluated per paint (a state or a closure).
///
/// An alias of [`Dyn`] rather than its own enum, so the "static or per-frame" rules — `eval` at
/// paint time, `same_parameter` for the element comparison — exist once. `AxisValue::Static(..)`
/// keeps working through the alias.
pub type AxisValue = Dyn<f32>;

/// Material Symbols variable axes (official ranges):
/// FILL 0..=1, GRAD -50..=200, opsz 20..=48, wght 100..=700.
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
    /// Evaluated at render time and clamped to the official ranges.
    pub fn evaluated(&self) -> (f32, f32, f32, f32) {
        // NaN defence: clamp(NaN) returns NaN, which would poison the cache key and the font axes.
        let clamp = |v: f32, (lo, hi): (f32, f32)| if v.is_nan() { lo } else { v.clamp(lo, hi) };
        (
            clamp(self.fill.eval(), FILL_RANGE),
            clamp(self.grad.eval(), GRAD_RANGE),
            clamp(self.opsz.eval(), OPSZ_RANGE),
            clamp(self.wght.eval(), WGHT_RANGE),
        )
    }
}

// Icon source

/// Where an icon comes from — no icons are built in by default; an SVG path, an SVG document and
/// an image file are always available, while variable-font symbols need a `material-symbols-*` feature.
#[derive(Debug, Clone, PartialEq)]
pub enum IconSource {
    /// Raw SVG path data (copied from fonts.google.com/icons) — wrapped at render time in an
    /// `<svg viewBox="0 0 24 24">` document handed to Skia's `svg::Dom`; no hand-written parser.
    SvgPath { data: Arc<str>, fill_type: PathFillType },
    /// A complete SVG document string (`<svg viewBox=...>...</svg>`).
    Svg(Arc<str>),
    /// An image file path (png/jpg/jpeg/webp/bmp/svg, decoded by extension).
    File(Arc<str>),
    /// A Material Symbols variable-font symbol.
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

    /// Monochrome sources (SvgPath / Svg / Symbol) are tinted with the theme colour by default; File keeps its own colours.
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

    /// Intrinsic size: SvgPath/Symbol default to 24x24; an SVG document parses its viewBox (falling
    /// back to 24x24 — the same fallback as `decode_svg_str`, so Image does not measure a 0-sized
    /// blank while Icon displays normally); a bitmap takes its pixel size (None when decoding fails —
    /// the caller treats that as 0).
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

/// Wrap bare path data in a minimal SVG document (Skia's svg::Dom requires a complete document).
pub(crate) fn wrap_svg_path(data: &str, fill_type: PathFillType) -> String {
    let rule = match fill_type {
        PathFillType::NonZero => "nonzero",
        PathFillType::EvenOdd => "evenodd",
    };
    // The path data comes from a public API — escape XML special characters so it cannot break the document.
    let escaped = data
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('"', "&quot;");
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="{escaped}" fill-rule="{rule}"/></svg>"#
    )
}

// Render spec (goes into ModifierElement::DrawIcon)

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

// Global caches: path parsing / SVG and image decoding / glyph blobs

/// Decoded icon content (for the render layer to draw).
///
/// Invariant: after decoding it is **read-only during painting** — an SVG container size is fixed
/// at decode time, and painting re-sizes the container to the target size before `render`. `svg::Dom`
/// is not guaranteed thread-safe, so the decode cache is thread-local (created and used on the render
/// thread) and never shared across threads.
pub(crate) enum DecodedIcon {
    Bitmap { image: skia_safe::Image, width: f32, height: f32 },
    Svg { dom: RefCell<svg::Dom>, width: f32, height: f32 },
}

/// Decode cache key (SvgPath is distinguished by data + fill rule).
#[derive(Hash, PartialEq, Eq, Clone)]
enum DecodeKey {
    SvgPath(Arc<str>, PathFillType),
    Doc(Arc<str>),
    File(Arc<str>),
}

/// Decode cache TTL: refreshed every 5 seconds for success and failure alike — editing a file during
/// development takes effect, and a failure does not re-read the disk every frame.
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
        // Cache successes and failures alike (a failure carries the TTL — no re-read every frame, but it retries once it expires).
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
        // Pre-generate the mipmap chain: FilterQuality::Medium/High's MipmapMode needs one
        // (Skia falls back to plain linear filtering for an image without mipmaps); Low is unaffected.
        // The decode cache guarantees it is generated once, not recomputed per paint.
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

/// Extract the intrinsic size from an SVG string: prefer the `width`/`height` attributes (the SVG
/// spec makes them decide the layout size; viewBox is only the coordinate system), falling back to
/// the 3rd and 4th viewBox values when they are absent.
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

/// Take the `<svg ...>` open tag (attributes are parsed inside the open tag only, so nothing elsewhere in the document is matched by mistake).
fn svg_open_tag(svg: &str) -> Option<&str> {
    let lower = svg.to_ascii_lowercase();
    let start = lower.find("<svg")?;
    let end = lower[start..].find('>')? + start;
    Some(&svg[start..end])
}

/// Parse the length of an attribute inside the open tag (`24`, `24px`, ...; percentages are unsupported and return None).
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

/// Find `name=` inside the open tag, requiring whitespace/a quote/the start of the tag before the
/// attribute name — so `stroke-width=` cannot hit `width=` and path text cannot hit `viewbox`.
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
    // The combinations of animated axes could grow without bound — a simple cap; past it, clear and rebuild.
    if cache.len() > 512 {
        cache.clear();
    }
    blob
}

