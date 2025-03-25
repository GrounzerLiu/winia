/*use std::any::{Any, TypeId};
use std::sync::{Arc, Mutex};
use crate::core::{get_id_by_str, RefClone};
use crate::shared::{Children, Gettable, Observable, Property};
use crate::ui::app::AppContext;
use crate::ui::Item;
use crate::ui::item::{CustomProperty, ItemEvent};

#[derive(Clone, Copy, Default)]
pub enum HorizontalAlignment {
    #[default]
    Start,
    Middle,
    End,
}

#[derive(Clone, Copy, Default)]
pub enum VerticalAlignment {
    #[default]
    Top,
    Center,
    Bottom,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ID {
    Parent,
    Sibling(usize),
}

/// The `middle` alignment and `center` alignment have higher priority than other alignments.
#[derive(Clone, Copy, Default)]
pub struct Alignment {
    start: Option<(ID, HorizontalAlignment)>,
    middle: Option<(ID, HorizontalAlignment)>,
    end: Option<(ID, HorizontalAlignment)>,
    top: Option<(ID, VerticalAlignment)>,
    center: Option<(ID, VerticalAlignment)>,
    bottom: Option<(ID, VerticalAlignment)>,
}

impl Alignment {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn start_align(mut self, id: &str, alignment: HorizontalAlignment) -> Self {
        self.start = Some((
            if id.is_empty() {
                ID::Parent
            } else {
                ID::Sibling(get_id_by_str(id).unwrap_or_else(|| panic!("Invalid id: {}", id)))
            },
            alignment
        ));
        self
    }

    pub fn middle_align(mut self, id: &str, alignment: HorizontalAlignment) -> Self {
        self.middle = Some((
            if id.is_empty() {
                ID::Parent
            } else {
                ID::Sibling(get_id_by_str(id).unwrap_or_else(|| panic!("Invalid id: {}", id)))
            }
            , alignment));
        self
    }
    pub fn end_align(mut self, id: &str, alignment: HorizontalAlignment) -> Self {
        self.end = Some((
            if id.is_empty() {
                ID::Parent
            } else {
                ID::Sibling(get_id_by_str(id).unwrap_or_else(|| panic!("Invalid id: {}", id)))
            },
            alignment
        ));
        self
    }
    pub fn top_align(mut self, id: &str, alignment: VerticalAlignment) -> Self {
        self.top = Some((
            if id.is_empty() {
                ID::Parent
            } else {
                ID::Sibling(get_id_by_str(id).unwrap_or_else(|| panic!("Invalid id: {}", id)))
            },
            alignment
        ));
        self
    }
    pub fn center_align(mut self, id: &str, alignment: VerticalAlignment) -> Self {
        self.center = Some((
            if id.is_empty() {
                ID::Parent
            } else {
                ID::Sibling(get_id_by_str(id).unwrap_or_else(|| panic!("Invalid id: {}", id)))
            },
            alignment
        ));
        self
    }
    pub fn bottom_align(mut self, id: &str, alignment: VerticalAlignment) -> Self {
        self.bottom = Some((
            if id.is_empty() {
                ID::Parent
            } else {
                ID::Sibling(get_id_by_str(id).unwrap_or_else(|| panic!("Invalid id: {}", id)))
            },
            alignment
        ));
        self
    }
}

pub trait RelativeAlignment {
    fn alignment(self, alignment: impl Into<Property<Alignment>>) -> Self;
    fn get_alignment(&self) -> Option<Property<Alignment>>;
}

impl RelativeAlignment for Item {
    fn alignment(self, alignment: impl Into<Property<Alignment>>) -> Self {
        let id = self.get_id();
        if let Some(mut alignment) = self.get_alignment() {
            alignment.remove_observer(id);
        }
        let app_context = self.get_app_context();
        let mut alignment = alignment.into();
        alignment.add_observer(
            id,
            Box::new(move || {
                app_context.request_re_layout();
            }),
        );
        self.custom_property("relative_alignment", CustomProperty::Any(Box::new(alignment)))
    }

    fn get_alignment(&self) -> Option<Property<Alignment>> {
        if let Some(alignment) = self.get_custom_property("relative_alignment") {
            if let CustomProperty::Any(alignment) = alignment {
                if let Some(alignment) = alignment.downcast_ref::<Property<Alignment>>() {
                    return Some(alignment.ref_clone());
                }
            }
        }
        None
    }
}


struct ItemRelation {
    index: usize,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    done: bool,
    alignment: Alignment,
    be_aligned: Vec<ItemRelation>,
}

struct ItemRelationManager {
    item_relations: Vec<ItemRelation>,
}

impl ItemRelationManager {
    fn new(item: &Item) -> Self {
        let mut done:Vec<ItemRelation> = Vec::new();
        let mut be_aligned:Vec<ItemRelation> = Vec::new();

        for (index, child) in item.get_children().items().iter().enumerate() {
            let alignment = child.get_alignment().expect("Relative alignment is not set");
            let relation = ItemRelation {
                index,
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 0.0,
                done: false,
                alignment: alignment.get(),
                be_aligned: Vec::new(),
            };
            be_aligned.push(relation);
        }




        Self {

        }
    }
}


struct RelativeProperty {}

pub struct Relative {
    item: Item,
    shared: Arc<Mutex<RelativeProperty>>,
}

impl Relative {
    pub fn new(app_context: AppContext, children: Children) -> Self {
        let shared = Arc::new(Mutex::new(RelativeProperty {}));
        let item_event = ItemEvent::new()
            .measure({
                move |item, width_mode, height_mode| {}
            });
        Self {}
    }
}*/
