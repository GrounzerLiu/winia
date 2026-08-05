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
}

#[cfg(test)]
mod paragraph_builder_test {
    use crate::text::paragraph_builder::ParagraphBuilder;
    use skia_safe::textlayout::{FontCollection, ParagraphStyle};
    use crate::text::index_bimap::IndexBiMap;

    // ── IndexBiMap 基础 ──

    #[test]
    fn test_index_bimap_basic() {
        let mut m = IndexBiMap::new();
        m.insert(0, 10);
        m.insert(1, 11);
        m.insert(3, 15);
        assert_eq!(m.get_by_left(&0), Some(&10));
        assert_eq!(m.get_by_left(&1), Some(&11));
        assert_eq!(m.get_by_left(&3), Some(&15));
        assert_eq!(m.get_by_left(&2), None);
        assert_eq!(m.get_by_right(&10), Some(&0));
        assert_eq!(m.get_by_right(&11), Some(&1));
        assert_eq!(m.get_by_right(&15), Some(&3));
        assert_eq!(m.get_by_right(&99), None);
        assert_eq!(m.len(), 3);
    }

    #[test]
    fn test_index_bimap_contains() {
        let mut m = IndexBiMap::new();
        m.insert(5, 50);
        assert!(m.contains_left(&5));
        assert!(!m.contains_left(&0));
        assert!(m.contains_right(&50));
        assert!(!m.contains_right(&0));
    }

    #[test]
    fn test_index_bimap_clear() {
        let mut m = IndexBiMap::new();
        m.insert(0, 0);
        m.clear();
        assert!(m.is_empty());
        assert_eq!(m.len(), 0);
    }

    // ── ParagraphBuilder::add_text 纯 ASCII ──

    #[test]
    fn test_builder_ascii() {
        let fc = FontCollection::new();
        let style = ParagraphStyle::default();
        let mut b = ParagraphBuilder::new(&style, fc);
        b.add_text("Hello");
        // 每个 ASCII 字符: byte=1, utf16=1, real=1
        assert_eq!(b.last_byte_index, 5);
        assert_eq!(b.last_real_index, 5);
        assert_eq!(b.last_utf16_index, 5);
        // byte 0 -> utf16 0
        assert_eq!(b.byte_to_utf16_indices.get_by_left(&0), Some(&0));
        // byte 4 -> utf16 4
        assert_eq!(b.byte_to_utf16_indices.get_by_left(&4), Some(&4));
    }

    // ── ParagraphBuilder::add_text 含 emoji ──

    #[test]
    fn test_builder_emoji() {
        let fc = FontCollection::new();
        let style = ParagraphStyle::default();
        let mut b = ParagraphBuilder::new(&style, fc);
        b.add_text("Hi 😊"); // H(0),i(1),space(2),😊(3-6) -> 7 bytes, 4 chars, 5 utf16 units
        // real (byte index) for '😊' is 3
        assert_eq!(b.paragraph_byte_to_real_indices.get_by_left(&3).copied(), Some(3));
        // utf16 for '😊' at byte 3 should be 3 (H=0,i=1,space=2,😊=3-4)
        assert_eq!(b.byte_to_utf16_indices.get_by_left(&3).copied(), Some(3));
        // utf16 for byte 4 (part of 😊 surrogate) — should not exist
        assert!(b.byte_to_utf16_indices.get_by_left(&4).is_none());
    }

    // ── ParagraphBuilder::add_text 混合 CJK ──

    #[test]
    fn test_builder_cjk() {
        let fc = FontCollection::new();
        let style = ParagraphStyle::default();
        let mut b = ParagraphBuilder::new(&style, fc);
        b.add_text("A中文B");
        // A(0),中(1-3),文(4-6),B(7) -> 8 bytes, 4 chars
        // utf16: A=0, 中=1, 文=2, B=3
        assert_eq!(b.byte_to_utf16_indices.get_by_left(&1).copied(), Some(1)); // 中 byte 1 -> utf16 1
        assert_eq!(b.byte_to_utf16_indices.get_by_left(&4).copied(), Some(2)); // 文 byte 4 -> utf16 2
    }

    // ── 多段 add_text 拼接 ──

    #[test]
    fn test_builder_multiple_calls() {
        let fc = FontCollection::new();
        let style = ParagraphStyle::default();
        let mut b = ParagraphBuilder::new(&style, fc);
        b.add_text("ab");
        b.add_text("cd");
        // total 4 bytes
        assert_eq!(b.last_byte_index, 4);
        // 'c' at byte 2 -> utf16 2
        assert_eq!(b.byte_to_utf16_indices.get_by_left(&2).copied(), Some(2));
    }
}