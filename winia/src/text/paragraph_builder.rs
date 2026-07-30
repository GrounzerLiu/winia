use crate::text::index_bimap::IndexBiMap;
use crate::text::paragraph::Paragraph;
use skia_safe::textlayout::{FontCollection, ParagraphBuilder as SkParagraphBuilder, ParagraphStyle, PlaceholderAlignment, PlaceholderStyle, TextBaseline, TextStyle};
use std::collections::HashSet;
use std::ops::Range;
use std::sync::Arc;
use unicode_segmentation::UnicodeSegmentation;

pub struct ParagraphBuilder {
    paragraph_builder: SkParagraphBuilder,
    placeholders: Vec<std::sync::Arc<dyn crate::text::InlineDrawable>>,
    last_byte_index: usize,
    last_real_index: usize,
    last_utf16_index: usize,
    last_grapheme_cluster_index: usize,
    line_breaks: HashSet<Range<usize>>,
    paragraph_byte_to_real_indices: IndexBiMap,
    byte_to_utf16_indices: IndexBiMap,
    byte_to_grapheme_cluster_indices: IndexBiMap,
}

impl ParagraphBuilder {
    pub fn new(style: &ParagraphStyle, font_collection: impl Into<FontCollection>) -> Self {
        let paragraph_builder = SkParagraphBuilder::new(style, font_collection);
        let placeholders = Vec::new();
        let last_byte_index = 0;
        let last_real_index = 0;
        let last_utf16_index = 0;
        let last_grapheme_cluster_index = 0;
        
        let line_breaks = HashSet::new();
        let paragraph_byte_to_real_indices = IndexBiMap::new();
        let byte_to_utf16_indices = IndexBiMap::new();
        let byte_to_grapheme_cluster_indices = IndexBiMap::new();

        ParagraphBuilder {
            paragraph_builder,
            placeholders,
            last_byte_index,
            last_real_index,
            last_utf16_index,
            last_grapheme_cluster_index,
            line_breaks,
            paragraph_byte_to_real_indices,
            byte_to_utf16_indices,
            byte_to_grapheme_cluster_indices,
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
        let mut last_grapheme_cluster_index = 0;

        str.grapheme_indices(true)
            .enumerate()
            .for_each(|(grapheme_cluster_index, (byte_index, str))| {
                if str == "\r\n" || str == "\n" || str == "\r" {
                    let index = self.last_real_index + byte_index;
                    self.line_breaks.insert(index..index + str.len());
                }
                let m_byte_index = self.last_byte_index + byte_index;
                self.byte_to_grapheme_cluster_indices.insert(
                    m_byte_index,
                    self.last_grapheme_cluster_index + grapheme_cluster_index
                );

                str.char_indices().for_each(|(index, char)|{
                    let m_byte_index = m_byte_index + index;
                    self.byte_to_utf16_indices
                        .insert(
                            m_byte_index,
                            self.last_utf16_index + last_utf16_index
                        );
                    self.paragraph_byte_to_real_indices
                        .insert(m_byte_index, self.last_real_index + byte_index + index);


                    let utf16_length = char.len_utf16();
                    let uft8_length = char.len_utf8();
                    last_utf16_index += utf16_length;
                    last_byte_index = m_byte_index + uft8_length;
                    last_real_index = byte_index + index + uft8_length;
                    // println!("last_byte_index: {}, last_real_index: {}, last_utf16_index: {}", last_byte_index, last_real_index, last_utf16_index);
                });
                last_grapheme_cluster_index = grapheme_cluster_index + 1;
            });
        
        self.last_real_index += last_real_index;
        self.last_byte_index = last_byte_index;
        self.last_utf16_index += last_utf16_index;
        // println!("self.last_byte_index: {}, self.last_real_index: {}, self.last_utf16_index: {}", self.last_byte_index, self.last_real_index, self.last_utf16_index);
        self.last_grapheme_cluster_index += last_grapheme_cluster_index;

        self.paragraph_builder.add_text(str);
    }

    pub fn add_placeholder(
        &mut self, 
        str: impl AsRef<str>, 
        placeholder: std::sync::Arc<dyn crate::text::InlineDrawable>, 
        placeholder_style: &Option<(PlaceholderAlignment, TextBaseline, f32)>,
    ) {
        let str = str.as_ref();
        if str.is_empty() {
            return;
        }
        
        self.paragraph_byte_to_real_indices.insert(self.last_byte_index, self.last_real_index);
        self.byte_to_utf16_indices.insert(self.last_byte_index, self.last_utf16_index);
        self.byte_to_grapheme_cluster_indices.insert(self.last_byte_index, self.last_grapheme_cluster_index);

        let placeholder_str = String::from_utf16(&[0xFFFC]).unwrap();
        let placeholder_byte_len = placeholder_str.len();
        let placeholder_utf16_len = placeholder_str.encode_utf16().count();
        let placeholder_grapheme_cluster_len = placeholder_str.graphemes(true).count();
        self.last_real_index += str.len();
        self.last_byte_index += placeholder_byte_len;
        self.last_utf16_index += placeholder_utf16_len;
        self.last_grapheme_cluster_index += placeholder_grapheme_cluster_len;
        
        
        let (width, height) = placeholder.size();
        self.placeholders.push(placeholder);
        let placeholder_style = if let Some((alignment, baseline, offset)) = placeholder_style {
            PlaceholderStyle::new(
                width,
                height,
                *alignment,
                *baseline,
                *offset,
            )
        } else {
            PlaceholderStyle::new(
                width,
                height,
                PlaceholderAlignment::Bottom,
                TextBaseline::Alphabetic,
                0.0,
            )
        };
        self.paragraph_builder
            .add_placeholder(&placeholder_style);
            // .add_placeholder(&PlaceholderStyle::new(
            //     width,
            //     height,
            //     PlaceholderAlignment::Bottom,
            //     TextBaseline::Alphabetic,
            //     0.0,
            // ));
    }

    pub fn build(&mut self) -> Paragraph {
        self.paragraph_byte_to_real_indices.insert(self.last_byte_index, self.last_real_index);
        self.byte_to_utf16_indices.insert(self.last_byte_index, self.last_utf16_index);
        self.byte_to_grapheme_cluster_indices.insert(self.last_byte_index, self.last_grapheme_cluster_index);
        // println!("paragraph_byte_to_real_index: {:?}", self.paragraph_byte_to_real_index);
        // println!("byte_to_utf16_indices: {:?}", self.byte_to_utf16_indices);
        // println!("byte_to_grapheme_cluster_indices: {:?}", self.byte_to_grapheme_cluster_indices);
        let paragraph = self.paragraph_builder.build();
        Paragraph::new(
            paragraph,
            &self.placeholders,
            &self.line_breaks,
            &self.paragraph_byte_to_real_indices,
            &self.byte_to_utf16_indices,
            &self.byte_to_grapheme_cluster_indices,
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
        self.last_grapheme_cluster_index = 0;
        self.line_breaks.clear();
        self.paragraph_byte_to_real_indices.clear();
        self.byte_to_utf16_indices.clear();
        self.byte_to_grapheme_cluster_indices.clear();
    }
}

#[cfg(test)]
mod paragraph_builder_test{
    use crate::text::index_bimap::IndexBiMap;
    use crate::text::paragraph::Paragraph;
    use skia_safe::textlayout::{FontCollection, ParagraphStyle};
    use unicode_segmentation::UnicodeSegmentation;

    #[test]
    fn test_paragraph_builder() {
        // let font_collection = FontCollection::new();
        // let paragraph_style = ParagraphStyle::default();
        // let mut paragraph_builder = super::ParagraphBuilder::new(&paragraph_style, font_collection);
        // paragraph_builder.add_text("abc");
        // paragraph_builder.add_placeholder("h", SharedDrawable::empty());
        // paragraph_builder.add_text("🤗");
        // paragraph_builder.add_placeholder("hhhhhhh", SharedDrawable::empty());
        // paragraph_builder.add_text("一二三");
        // paragraph_builder.build();
        // let text = paragraph_builder.paragraph_builder.get_text();
        // let length = text.len();
        // let utf16_length = text.encode_utf16().count();
        // let glyph_length = text.graphemes(true).count();
        // assert_eq!(paragraph_builder.byte_to_glyph_indices.get_by_left(&length), Some(&glyph_length));
        // assert_eq!(paragraph_builder.byte_to_utf16_indices.get_by_left(&length), Some(&utf16_length));
        // println!("byte_to_glyph_indices: {:?}", paragraph_builder.byte_to_glyph_indices);
        // println!("byte_to_utf16_indices: {:?}", paragraph_builder.byte_to_utf16_indices);
        // println!("paragraph_byte_to_real_index: {:?}", paragraph_builder.paragraph_byte_to_real_index);
    }
}