use crate::shared::SharedDrawable;
use crate::text::paragraph::Paragraph;
use bimap::BiBTreeMap;
use skia_safe::textlayout::{FontCollection, ParagraphBuilder as SkParagraphBuilder, ParagraphStyle, PlaceholderAlignment, PlaceholderStyle, TextBaseline, TextStyle};
use std::collections::HashSet;
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

pub struct ParagraphBuilder {
    paragraph_builder: SkParagraphBuilder,
    placeholders: Vec<SharedDrawable>,
    last_byte_index: usize,
    last_real_index: usize,
    last_utf16_index: usize,
    last_glyph_index: usize,
    line_breaks: HashSet<Range<usize>>,
    paragraph_byte_to_real_index: BiBTreeMap<usize, usize>,
    byte_to_utf16_indices: BiBTreeMap<usize, usize>,
    byte_to_glyph_indices: BiBTreeMap<usize, usize>,
}

impl ParagraphBuilder {
    pub fn new(style: &ParagraphStyle, font_collection: impl Into<FontCollection>) -> Self {
        let paragraph_builder = SkParagraphBuilder::new(style, font_collection);
        let placeholders = Vec::new();
        let last_byte_index = 0;
        let last_real_index = 0;
        let last_utf16_index = 0;
        let last_glyph_index = 0;
        
        let line_breaks = HashSet::new();
        let paragraph_byte_to_real_index = BiBTreeMap::new();
        let byte_to_utf16_indices = BiBTreeMap::new();
        let byte_to_glyph_indices = BiBTreeMap::new();

        ParagraphBuilder {
            paragraph_builder,
            placeholders,
            last_byte_index,
            last_real_index,
            last_utf16_index,
            last_glyph_index,
            line_breaks,
            paragraph_byte_to_real_index,
            byte_to_utf16_indices,
            byte_to_glyph_indices,
        }
    }
    
    pub fn push_style(&mut self, style: &TextStyle) -> &mut Self {
        self.paragraph_builder.push_style(style);
        self
    }
    
    pub fn pop(&mut self) -> &mut Self {
        self.paragraph_builder.pop();
        self
    }
    
    pub fn peek_style(&mut self) -> TextStyle {
        self.paragraph_builder.peek_style()
    }
    
    pub fn add_text(&mut self, str: impl AsRef<str>) {
        let str = str.as_ref();
        if str.is_empty() {
            return;
        }

        let mut last_real_index = 0;
        let mut last_byte_index = 0;
        let mut last_utf16_index = 0;
        let mut last_glyph_index = 0;

        str.grapheme_indices(false)
            .enumerate()
            .for_each(|(glyph_index, (byte_index, str))| {
                let m_byte_index = self.last_byte_index + byte_index;
                self.byte_to_utf16_indices
                    .insert(
                        m_byte_index,
                        self.last_utf16_index + last_utf16_index
                    );
                self.byte_to_glyph_indices.insert(
                    m_byte_index,
                    self.last_glyph_index + glyph_index
                );
                self.paragraph_byte_to_real_index
                    .insert(m_byte_index, self.last_real_index + byte_index);

                let utf16_length = str.encode_utf16().count();
                last_utf16_index += utf16_length;

                if str == "\r\n" || str == "\n" || str == "\r" {
                    let index = self.last_real_index + byte_index;
                    self.line_breaks.insert(index..index + str.len());
                }
                last_glyph_index = glyph_index + 1;
                last_byte_index = m_byte_index + str.len();
                last_real_index = byte_index + str.len();
            });
        
        self.last_real_index += last_real_index;
        self.last_byte_index = last_byte_index;
        self.last_utf16_index += last_utf16_index;
        self.last_glyph_index += last_glyph_index;

        self.paragraph_builder.add_text(str);
    }

    pub fn add_placeholder(&mut self, str: impl AsRef<str>, placeholder: SharedDrawable) {
        let str = str.as_ref();
        if str.is_empty() {
            return;
        }
        
        self.paragraph_byte_to_real_index.insert(self.last_byte_index, self.last_real_index);
        self.byte_to_utf16_indices.insert(self.last_byte_index, self.last_utf16_index);
        self.byte_to_glyph_indices.insert(self.last_byte_index, self.last_glyph_index);

        let placeholder_str = String::from_utf16(&[0xFFFC]).unwrap();
        let placeholder_byte_len = placeholder_str.len();
        let placeholder_utf16_len = placeholder_str.encode_utf16().count();
        let placeholder_glyph_len = placeholder_str.graphemes(false).count();
        self.last_real_index += str.len();
        self.last_byte_index += placeholder_byte_len;
        self.last_utf16_index += placeholder_utf16_len;
        self.last_glyph_index += placeholder_glyph_len;
        
        
        let width = placeholder.lock().width();
        let height = placeholder.lock().height();
        self.placeholders.push(placeholder);
        self.paragraph_builder
            .add_placeholder(&PlaceholderStyle::new(
                width,
                height,
                PlaceholderAlignment::Middle,
                TextBaseline::Alphabetic,
                0.0,
            ));
    }

    pub fn build(&mut self) -> Paragraph {
        self.paragraph_byte_to_real_index.insert(self.last_byte_index, self.last_real_index);
        self.byte_to_utf16_indices.insert(self.last_byte_index, self.last_utf16_index);
        self.byte_to_glyph_indices.insert(self.last_byte_index, self.last_glyph_index);
        let paragraph = self.paragraph_builder.build();
        Paragraph::new(
            paragraph,
            &self.placeholders,
            &self.line_breaks,
            &self.paragraph_byte_to_real_index,
            &self.byte_to_utf16_indices,
            &self.byte_to_glyph_indices,
        )
    }
    
    pub fn get_paragraph_style(&self) -> ParagraphStyle {
        self.paragraph_builder.get_paragraph_style()
    }
    
    pub fn reset(&mut self) {
        self.paragraph_builder.reset();
        self.placeholders.clear();
        self.last_byte_index = 0;
        self.last_real_index = 0;
        self.last_utf16_index = 0;
        self.last_glyph_index = 0;
        self.line_breaks.clear();
        self.paragraph_byte_to_real_index.clear();
        self.byte_to_utf16_indices.clear();
        self.byte_to_glyph_indices.clear();
    }
}

#[cfg(test)]
mod paragraph_builder_test{
    use crate::shared::SharedDrawable;
    use skia_safe::textlayout::{FontCollection, ParagraphStyle};
    use unicode_segmentation::UnicodeSegmentation;

    #[test]
    fn test_paragraph_builder() {
        let font_collection = FontCollection::new();
        let paragraph_style = ParagraphStyle::default();
        let mut paragraph_builder = super::ParagraphBuilder::new(&paragraph_style, font_collection);
        paragraph_builder.add_text("abc");
        paragraph_builder.add_placeholder("h", SharedDrawable::empty());
        paragraph_builder.add_text("🤗");
        paragraph_builder.add_placeholder("hhhhhhh", SharedDrawable::empty());
        paragraph_builder.add_text("一二三");
        paragraph_builder.build();
        let text = paragraph_builder.paragraph_builder.get_text();
        let length = text.len();
        let utf16_length = text.encode_utf16().count();
        let glyph_length = text.graphemes(true).count();
        assert_eq!(paragraph_builder.byte_to_glyph_indices.get_by_left(&length), Some(&glyph_length));
        assert_eq!(paragraph_builder.byte_to_utf16_indices.get_by_left(&length), Some(&utf16_length));
        // println!("byte_to_glyph_indices: {:?}", paragraph_builder.byte_to_glyph_indices);
        // println!("byte_to_utf16_indices: {:?}", paragraph_builder.byte_to_utf16_indices);
        // println!("paragraph_byte_to_real_index: {:?}", paragraph_builder.paragraph_byte_to_real_index);
    }
}