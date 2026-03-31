use crate::text::font_manager;
use lazy_static::lazy_static;
use parking_lot::Mutex;
use skia_safe::font_arguments::variation_position::Coordinate;
use skia_safe::font_arguments::VariationPosition;
use skia_safe::{Canvas, Font, FontArguments, FourByteTag, Paint, TextBlob, Typeface};
use std::collections::HashMap;
use std::sync::Arc;

#[cfg(feature = "material-symbols-outlined")]
mod outlined;
#[cfg(feature = "material-symbols-outlined")]
pub use outlined::*;

#[cfg(feature = "material-symbols-rounded")]
mod rounded;
#[cfg(feature = "material-symbols-rounded")]
pub use rounded::*;

#[cfg(feature = "material-symbols-sharp")]
mod sharp;
#[cfg(feature = "material-symbols-sharp")]
pub use sharp::*;

use crate::drawable::Drawable;
use crate::ui::{Color, SetColor};

#[cfg(any(feature = "material-symbols-outlined", feature = "material-symbols-rounded",
    feature = "material-symbols-sharp"))]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum MaterialSymbol {
    #[cfg(feature = "material-symbols-outlined")]
    Outlined(&'static str),
    #[cfg(feature = "material-symbols-rounded")]
    Rounded(&'static str),
    #[cfg(feature = "material-symbols-sharp")]
    Sharp(&'static str),
}
impl Into<&'static str> for MaterialSymbol {
    fn into(self) -> &'static str {
        match self {
            #[cfg(feature = "material-symbols-outlined")]
            MaterialSymbol::Outlined(code_point) => code_point,
            #[cfg(feature = "material-symbols-rounded")]
            MaterialSymbol::Rounded(code_point) => code_point,
            #[cfg(feature = "material-symbols-sharp")]
            MaterialSymbol::Sharp(code_point) => code_point,
        }
    }
}

#[cfg(any(feature = "material-symbols-outlined", feature = "material-symbols-rounded",
    feature = "material-symbols-sharp"))]
lazy_static! {
    static ref ICON_CHACHE: Mutex<HashMap<MaterialSymbol, IconDrawable>> = Mutex::new(HashMap::new());
}

#[cfg(feature = "material-symbols-outlined")]
static FONT_OUTLINED: &[u8] = include_bytes!("MaterialSymbolsOutlined[FILL,GRAD,opsz,wght].ttf");
#[cfg(feature = "material-symbols-outlined")]
lazy_static!(
    pub(crate) static ref TYPEFACE_OUTLINED: Typeface = font_manager().new_from_data(
        FONT_OUTLINED,
        None
    ).unwrap();
);

#[cfg(feature = "material-symbols-rounded")]
static FONT_ROUNDED: &[u8] = include_bytes!("MaterialSymbolsRounded[FILL,GRAD,opsz,wght].ttf");
#[cfg(feature = "material-symbols-rounded")]
lazy_static!(
    pub(crate) static ref TYPEFACE_ROUNDED: Typeface = font_manager().new_from_data(
        FONT_ROUNDED,
        None
    ).unwrap();
);

#[cfg(feature = "material-symbols-sharp")]
static FONT_SHARP: &[u8] = include_bytes!("MaterialSymbolsSharp[FILL,GRAD,opsz,wght].ttf");
#[cfg(feature = "material-symbols-sharp")]
lazy_static!(
    pub(crate) static ref TYPEFACE_SHARP: Typeface = font_manager().new_from_data(
        FONT_SHARP,
        None
    ).unwrap();
);

#[cfg(any(feature = "material-symbols-outlined", feature = "material-symbols-rounded",
          feature = "material-symbols-sharp"))]
struct InnerIcon {
    text_blob_changed: bool,
    symbol: MaterialSymbol,
    size: f32,
    color: Color,
    fill: f32,
    weight: f32,
    grade: f32,
    optical_size: f32,
    width: f32,
    height: f32,
    type_face: Typeface,
    paint: Paint,
    text_blob: TextBlob,
}

#[cfg(any(feature = "material-symbols-outlined", feature = "material-symbols-rounded",
    feature = "material-symbols-sharp"))]
#[derive(Clone)]
pub struct IconDrawable {
    inner: Arc<Mutex<InnerIcon>>
}

fn generate_text_blob(
    type_face: &Typeface,
    symbol: MaterialSymbol,
    size: f32,
    fill: f32,
    weight: f32,
    grade: f32,
    optical_size: f32,
) -> TextBlob {
    let type_face = type_face.clone_with_arguments(
        &FontArguments::new().set_variation_design_position(
            VariationPosition {
                coordinates: &[
                    Coordinate {
                        axis: FourByteTag::from_chars('F', 'I', 'L', 'L'),
                        value: fill,
                    },
                    Coordinate {
                        axis: FourByteTag::from_chars('w', 'g', 'h', 't'),
                        value: weight,
                    },
                    Coordinate {
                        axis: FourByteTag::from_chars('G', 'R', 'A', 'D'),
                        value: grade,
                    },
                    Coordinate {
                        axis: FourByteTag::from_chars('o', 'p', 't', 's'),
                        value: optical_size,
                    },
                ],
            }
        )
    ).unwrap();
    let font = Font::new(&type_face, size);
    let symbol: &str = symbol.into();
    TextBlob::new(symbol, &font).unwrap()
}

impl IconDrawable {
/*    #[cfg(feature = "material-symbols-outlined")]
    pub fn outlined(symbol: MaterialSymbol, size: f32, color: Color) -> Self {
        let mut cache = ICON_CHACHE.lock();
        if let Some(icon) = cache.get(&symbol) {
            return icon.clone();
        }
        let icon = IconDrawable::new(symbol, TYPEFACE_OUTLINED.clone(), size, color);
        cache.insert(symbol, icon.clone());
        icon
    }

    #[cfg(feature = "material-symbols-rounded")]
    pub fn rounded(symbol: MaterialSymbol, size: f32, color: Color) -> Self {
        let mut cache = ICON_CHACHE.lock();
        if let Some(icon) = cache.get(&symbol) {
            return icon.clone();
        }
        let icon = IconDrawable::new(symbol, TYPEFACE_ROUNDED.clone(), size, color);
        cache.insert(symbol, icon.clone());
        icon
    }

    #[cfg(feature = "material-symbols-sharp")]
    pub fn sharp(symbol: MaterialSymbol, size: f32, color: Color) -> Self {
        let mut cache = ICON_CHACHE.lock();
        if let Some(icon) = cache.get(&symbol) {
            return icon.clone();
        }
        let icon = IconDrawable::new(symbol, TYPEFACE_SHARP.clone(), size, color);
        cache.insert(symbol, icon.clone());
        icon
    }*/

    pub fn new(symbol: MaterialSymbol) -> Self {
        let mut cache = ICON_CHACHE.lock();
        if let Some(icon) = cache.get(&symbol) {
            return icon.clone();
        }
        let type_face = match symbol {
            #[cfg(feature = "material-symbols-outlined")]
            MaterialSymbol::Outlined(_) => TYPEFACE_OUTLINED.clone(),
            #[cfg(feature = "material-symbols-rounded")]
            MaterialSymbol::Rounded(_) => TYPEFACE_ROUNDED.clone(),
            #[cfg(feature = "material-symbols-sharp")]
            MaterialSymbol::Sharp(_) => TYPEFACE_SHARP.clone(),
        };
        let icon = IconDrawable::inner_new(symbol, type_face, 24.0, Color::BLACK);
        cache.insert(symbol, icon.clone());
        icon
    }

    fn inner_new(symbol: MaterialSymbol, type_face: Typeface, size: f32, color: Color) -> Self {
        let mut paint = Paint::default();
        paint.set_any_color(color);
        paint.set_anti_alias(true);
        let fill = 0.0;
        let weight = 400.0;
        let grade = 0.0;
        let optical_size = 24.0;
        let text_blob = generate_text_blob(
            &type_face,
            symbol,
            size,
            fill,
            weight,
            grade,
            optical_size,
        );
        let inner = InnerIcon {
            text_blob_changed: true,
            symbol,
            size,
            color,
            fill,
            weight,
            grade,
            optical_size,
            width: size,
            height: size,
            type_face,
            paint,
            text_blob,
        };
        IconDrawable { inner: Arc::new(Mutex::new(inner)) }
    }

    pub fn get_color(&self) -> Color {
        self.inner.lock().color
    }

    pub fn set_color(&self, color: Color) {
        let mut inner = self.inner.lock();
        inner.color = color;
        inner.text_blob_changed = true;
    }

    pub fn color(self, color: Color) -> Self {
        self.set_color(color);
        self
    }

    pub fn get_size(self) -> f32 {
        self.inner.lock().size
    }

    pub fn set_size(&self, size: f32) {
        let mut inner = self.inner.lock();
        inner.size = size;
        inner.text_blob_changed = true;
    }

    pub fn size(self, size: f32) -> Self {
        self.set_size(size);
        self
    }

    pub fn get_fill(&self) -> f32 {
        self.inner.lock().fill
    }
    pub fn set_fill(&self, fill: f32) {
        let mut inner = self.inner.lock();
        inner.fill = fill;
        inner.text_blob_changed = true;
    }
    pub fn fill(self, fill: f32) -> Self {
        self.set_fill(fill);
        self
    }

    pub fn get_weight(&self) -> f32 {
        self.inner.lock().weight
    }
    pub fn set_weight(&self, weight: f32) {
        let mut inner = self.inner.lock();
        inner.weight = weight;
        inner.text_blob_changed = true;
    }
    pub fn weight(self, weight: f32) -> Self {
        self.set_weight(weight);
        self
    }
    pub fn get_grade(&self) -> f32 {
        self.inner.lock().grade
    }
    pub fn set_grade(&self, grade: f32) {
        let mut inner = self.inner.lock();
        inner.grade = grade;
        inner.text_blob_changed = true;
    }
    pub fn grade(self, grade: f32) -> Self {
        self.set_grade(grade);
        self
    }
    pub fn get_optical_size(&self) -> f32 {
        self.inner.lock().optical_size
    }
    pub fn set_optical_size(&self, optical_size: f32) {
        let mut inner = self.inner.lock();
        inner.optical_size = optical_size;
        inner.text_blob_changed = true;
    }
    pub fn optical_size(self, optical_size: f32) -> Self {
        self.set_optical_size(optical_size);
        self
    }
}

impl Drawable for IconDrawable {
    fn draw(&self, canvas: &Canvas, x: f32, y: f32) {
        let mut inner = self.inner.lock();
        let color = inner.color;
        inner.paint.set_any_color(color);
        let size = inner.width.min(inner.height);
        if inner.text_blob_changed {
            inner.text_blob = generate_text_blob(
                &inner.type_face,
                inner.symbol,
                size,
                inner.fill,
                inner.weight,
                inner.grade,
                inner.optical_size,
            );
            inner.text_blob_changed = false;
        }
        let x = x + (inner.width - size) / 2.0;
        let y = y + (inner.height - size) / 2.0 + size;
        canvas.draw_text_blob(&inner.text_blob, (x, y), &inner.paint);
    }

    fn get_intrinsic_width(&self) -> f32 {
        self.inner.lock().size
    }

    fn get_intrinsic_height(&self) -> f32 {
        self.inner.lock().size
    }

    fn set_width(&mut self, width: f32) {
        let mut inner = self.inner.lock();
        inner.width = width;
        inner.text_blob_changed = true;
    }

    fn set_height(&mut self, height: f32) {
        self.inner.lock().height = height;
    }

    fn width(&self) -> f32 {
        self.inner.lock().width
    }

    fn height(&self) -> f32 {
        self.inner.lock().height
    }

    fn set_color(&mut self, color: Option<Color>) {
        if let Some(c) = color {
            self.inner.lock().color = c;
        }
    }

    fn get_color(&self) -> Option<Color> {
        self.inner.lock().color.into()
    }

    fn add_redraw_requester(&mut self, id: u32, redraw_requester: Box<dyn Fn() + Send>) {
        // todo!()
    }

    fn remove_redraw_requester(&mut self, id: u32) {
        // todo!()
    }

    fn request_redraw(&self) {
        // todo!()
    }

    fn clone_drawable(&self) -> Box<dyn Drawable> {
        let clone = self.clone();
        Box::new(clone)
    }
}