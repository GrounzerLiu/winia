use std::collections::HashMap;
use std::fmt::Display;
use std::ops::{Add, Index, Range};
use std::str::FromStr;
use std::sync::{Arc, Mutex};

// use icu::segmenter::GraphemeClusterSegmenter;
use crate::core::generate_id;
use crate::property::{Observable, Removal};
use crate::text::EdgeBehavior;
use crate::text::style::Style;

pub struct StyledText {
    string: String,
    styles: Vec<(Style, Range<usize>, EdgeBehavior)>,
    simple_observers: Arc<Mutex<Vec<(usize, Box<dyn FnMut()>)>>>,
}

impl StyledText {
    fn new(string: String) -> Self {
        StyledText {
            string,
            styles: Vec::new(),
            simple_observers: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn as_str(&self) -> &str {
        &self.string
    }

    pub fn substring(&self, range: Range<usize>) -> StyledText {
        self.assert_in_range(&range);
        let string = self.string[range.clone()].to_string();
        let mut styles: Vec<(Style, Range<usize>, EdgeBehavior)> = Vec::new();
        for (style, style_range, edge_behavior) in self.styles.iter() {
            if style_range.start < range.start{
                if style_range.end >range.start{
                    styles.push(
                        (
                            style.clone(),
                            style_range.start-range.start..style_range.end-range.start,
                            edge_behavior.clone()
                        )
                    );
                }
            }else if style_range.start >= range.start && style_range.end < range.end {
                styles.push(
                    (
                        style.clone(),
                        style_range.start-range.start..style_range.end-range.start,
                        edge_behavior.clone()
                    )
                );
            }
        }
        StyledText {
            string,
            styles,
            simple_observers: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn len(&self) -> usize {
        self.string.len()
    }

    pub fn is_empty(&self) -> bool {
        self.string.is_empty()
    }

    fn notify(&mut self){
        for (_, observer) in self.simple_observers.lock().unwrap().iter_mut() {
            observer();
        }
    }


    pub fn insert(&mut self, index: usize, string: &str) {
        self.string.insert_str(index, string);
        self.styles.iter_mut().for_each(|(_, range, _)| {
            if range.start >= index {
                range.start += string.len();
            }
            if range.end >= index {
                range.end += string.len();
            }
        });
        self.notify();
    }

    pub fn remove(&mut self, range: Range<usize>) {
        self.string.drain(range.clone());
        self.styles.iter_mut().for_each(|(_, style_range, _)| {
            if style_range.start >= range.end {
                style_range.start -= range.end - range.start;
            } else if style_range.start >= range.start {
                style_range.start = range.start;
            }
            if style_range.end >= range.end {
                style_range.end -= range.end - range.start;
            } else if style_range.end >= range.start {
                style_range.end = range.start;
            }
        });
        self.notify();
    }

    pub fn append(&mut self, string: &str) {
        self.string.push_str(string);
        self.notify();
    }

    pub fn push(&mut self, c: char) {
        self.string.push(c);
        self.notify();
    }

    pub fn clear(&mut self) {
        self.string.clear();
        self.styles.clear();
        self.notify();
    }

    fn assert_in_range(&self, range: &Range<usize>) {
        if range.start > self.string.len() || range.end > self.string.len() {
            panic!("Range out of bounds");
        }
    }

    pub fn set_style(&mut self, style: Style, range: Range<usize>, boundary_type: EdgeBehavior) {
        self.assert_in_range(&range);

        let style_clone = style.clone();

        self.remove_style(style, range.clone());
        self.styles.push((style_clone, range, boundary_type));

        self.notify();
    }

    pub fn get_styles(&self, range: Range<usize>) -> Vec<(Style, Range<usize>, EdgeBehavior)> {
        self.assert_in_range(&range);
        let mut styles: Vec<(Style, Range<usize>, EdgeBehavior)> = Vec::new();
        for (style, style_range, edge_behavior) in self.styles.iter() {
            if style_range.start >= range.start && style_range.end <= range.end {
                styles.push((*style, style_range.clone(), *edge_behavior));
            }
        }
        styles
    }

    pub fn remove_style(&mut self, style: Style, range:Range<usize>) {
        self.assert_in_range(&range);

        let mut segmented_styles: Vec<(Style, Range<usize>, EdgeBehavior)> = Vec::new();

        self.styles.retain(|(s, style_range, boundary_type)| {
            if s.name() == style.name() {
                if range.start <= style_range.start {
                    if range.end > style_range.start {
                        if range.end < style_range.end {
                            segmented_styles.push(
                                (
                                    *s,
                                    range.end..style_range.end,
                                    boundary_type.clone()
                                )
                            );
                        }
                        return false;
                    } else {
                        return true;
                    }
                } else if range.start == style_range.start {
                    if range.end < style_range.end {
                        segmented_styles.push(
                            (
                                *s,
                                range.end..style_range.end,
                                boundary_type.clone()
                            )
                        );
                        return false;
                    } else if range.end >= style_range.end {
                        return false;
                    }
                } else if range.start > style_range.start {
                    if range.end < style_range.end {
                        segmented_styles.push(
                            (
                                *s,
                                style_range.start..range.start,
                                boundary_type.clone()
                            )
                        );
                        segmented_styles.push(
                            (
                                *s,
                                range.end..style_range.end,
                                boundary_type.clone()
                            )
                        );
                        return false;
                    } else if range.end >= style_range.end {
                        segmented_styles.push(
                            (
                                *s,
                                style_range.start..range.start,
                                boundary_type.clone()
                            )
                        );
                        return false;
                    }
                }
                return false;
            }
            true
        });

        self.styles.append(&mut segmented_styles);

        self.notify();
    }

    pub fn clear_styles(&mut self) {
        self.styles.clear();
        self.notify();
    }
}

impl Display for StyledText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.string.to_string())
    }
}

impl PartialEq for StyledText {
    fn eq(&self, other: &Self) -> bool {
        self.string == other.string
    }
}

impl PartialEq<String> for StyledText {
    fn eq(&self, other: &String) -> bool {
        self.string == *other
    }
}

impl PartialEq<StyledText> for String {
    fn eq(&self, other: &StyledText) -> bool {
        *self == other.string
    }
}

impl PartialEq<&str> for StyledText {
    fn eq(&self, other: &&str) -> bool {
        self.string == *other
    }
}

impl PartialEq<StyledText> for &str {
    fn eq(&self, other: &StyledText) -> bool {
        *self == other.string
    }
}

impl From<String> for StyledText {
    fn from(string: String) -> Self {
        StyledText::new(string)
    }
}

impl From<&str> for StyledText {
    fn from(string: &str) -> Self {
        StyledText::new(string.to_string())
    }
}

impl FromStr for StyledText {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(StyledText::new(s.to_string()))
    }
}

impl Clone for StyledText {
    /// Observers are not cloned because closures cannot be cloned. And different instances of StyledText should have different observers.
    fn clone(&self) -> Self {
        StyledText {
            string: self.string.clone(),
            styles: self.styles.clone(),
            simple_observers: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl Index<Range<usize>> for StyledText {
    type Output = str;

    fn index(&self, index: Range<usize>) -> &Self::Output {
        &self.string[index]
    }
}

impl AsRef<StyledText> for StyledText {
    fn as_ref(&self) -> &StyledText {
        self
    }
}

impl Observable for StyledText {
    fn add_observer(&mut self, id: usize, observer: Box<dyn FnMut()>) -> Removal {
        self.simple_observers.lock().unwrap().push((id, observer));
        let simple_observers = self.simple_observers.clone();
        Removal::new(move || {
            simple_observers.lock().unwrap().retain(|(observer_id, _)| *observer_id != id);
        })
    }
}

impl <T:AsRef<StyledText> +'static> Add<T> for StyledText {
    type Output = StyledText;

    fn add(self, rhs: T) -> Self::Output {
        let mut output = StyledText::new(self.string.clone() + &rhs.as_ref().string);
        self.styles.iter().for_each(|(style, range, edge_behavior)| {
            output.set_style(style.clone(), range.clone(), *edge_behavior);
        });
        
        let self_len = self.string.len();
        rhs.as_ref().styles.iter().for_each(|(style, range, edge_behavior)| {
            output.set_style(style.clone(), (range.start+self_len)..(range.end+self_len), *edge_behavior);
        });
        
        output
    }
}



// pub(crate) fn foreach_grapheme_cluster(text: &str, mut f: impl FnMut(Range<usize>)->Option<Range<usize>>)->Option<Range<usize>>{
//     let segmenter = GraphemeClusterSegmenter::new();
//     let mut iter = segmenter.segment_str(text);
//     let mut last_index = iter.next();
//     while let Some(next_index) = iter.next() {
//         if let Some(last_index) = last_index {
//             if let Some(range)=f(last_index..next_index) {
//                 return Some(range);
//             }
//         }
//         else{
//             break;
//         }
//         last_index = Some(next_index);
//     }
//     None
// }


