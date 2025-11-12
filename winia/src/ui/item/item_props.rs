use crate::app::WindowContext;
use crate::shared::{SharedDerived, SharedDerivedBool, SharedDerivedF32, SharedDerivedSize, SharedItem, SharedSource};
use crate::ui::item::{ClickSource, CursorMove, FocusRequester, FocusState, Frame, ItemState, LayoutDirection, MouseInput, NeedRedraw, PointerInput, Size};
use parking_lot::Mutex;
use std::any::Any;
use std::collections::HashMap;
use std::fmt::{Debug, Display};
use std::sync::Arc;
use skia_safe::{Path, Rect};
use crate::core::bind_str_to_id;
use crate::{depend, With};
use crate::ui::InnerPosition;

#[derive(Clone, Default)]
pub struct Padding {
    pub start: SharedDerived<f32>,
    pub end: SharedDerived<f32>,
    pub top: SharedDerived<f32>,
    pub bottom: SharedDerived<f32>,
}

impl Debug for Padding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Padding")
            .field("start", &self.start)
            .field("end", &self.end)
            .field("top", &self.top)
            .field("bottom", &self.bottom)
            .finish()
    }
}

impl Display for Padding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Padding {{ start: {}, end: {}, top: {}, bottom: {} }}",
            self.start.get(),
            self.end.get(),
            self.top.get(),
            self.bottom.get()
        )
    }
}

impl Padding {
    pub fn start(mut self, start: impl Into<SharedDerived<f32>>) -> Self {
        self.start = start.into();
        self
    }
    pub fn end(mut self, end: impl Into<SharedDerived<f32>>) -> Self {
        self.end = end.into();
        self
    }
    pub fn top(mut self, top: impl Into<SharedDerived<f32>>) -> Self {
        self.top = top.into();
        self
    }
    pub fn bottom(mut self, bottom: impl Into<SharedDerived<f32>>) -> Self {
        self.bottom = bottom.into();
        self
    }

    pub fn horizontal(horizontal: impl Into<SharedDerived<f32>>) -> Self {
        let horizontal = horizontal.into();
        Self {
            start: horizontal.clone(),
            end: horizontal,
            top: 0.0.into(),
            bottom: 0.0.into(),
        }
    }

    pub fn vertical(vertical: impl Into<SharedDerived<f32>>) -> Self {
        let vertical = vertical.into();
        Self {
            start: 0.0.into(),
            end: 0.0.into(),
            top: vertical.clone(),
            bottom: vertical,
        }
    }

    pub fn all(all: impl Into<SharedDerived<f32>>) -> Self {
        let all = all.into();
        Self {
            start: all.clone(),
            end: all.clone(),
            top: all.clone(),
            bottom: all,
        }
    }
}

pub struct ItemProps {
    pub window_context: WindowContext,
    pub background: SharedItem,
    pub baseline: SharedDerived<Option<f32>>,
    pub blur: SharedDerivedF32,
    pub clipped: SharedDerivedBool,
    pub clip_shape: SharedDerived<Option<Box<dyn Fn(&Frame)-> Path>>>,
    pub custom_props: HashMap<String, Box<dyn Any>>,
    pub enable: SharedDerivedBool,
    pub enable_background_blur: SharedDerivedBool,
    pub focusable: SharedDerivedBool,
    pub focus_requester: SharedDerived<FocusRequester>,
    pub foreground: SharedItem,
    pub height: SharedDerivedSize,
    pub item_state: SharedSource<ItemState>,
    pub layout_direction: SharedDerived<LayoutDirection>,
    pub max_height: SharedDerivedF32,
    pub min_height: SharedDerivedF32,
    pub min_width: SharedDerivedF32,
    pub max_width: SharedDerivedF32,
    pub name: SharedDerived<String>,
    pub need_redraw: Arc<Mutex<NeedRedraw>>,
    pub on_click: Option<Box<dyn FnMut(&ClickSource)>>,
    pub on_cursor_move: Option<Box<dyn FnMut(&CursorMove)>>,
    pub on_focus_changed: Option<Box<dyn FnMut(&FocusState)>>,
    pub on_hover_changed: Option<Box<dyn FnMut(bool)>>,
    pub on_mouse_input: Option<Box<dyn FnMut(&MouseInput) -> bool>>,
    pub on_pointer_input: Option<Box<dyn FnMut(&PointerInput) -> bool>>,
    pub offset_x: SharedDerivedF32,
    pub offset_y: SharedDerivedF32,
    pub opacity: SharedDerivedF32,
    pub padding: Padding,
    pub rotation: SharedDerivedF32,
    pub rotation_center_x: SharedDerived<InnerPosition>,
    pub rotation_center_y: SharedDerived<InnerPosition>,
    pub scale_x: SharedDerivedF32,
    pub scale_y: SharedDerivedF32,
    pub scale_center_x: SharedDerived<InnerPosition>,
    pub scale_center_y: SharedDerived<InnerPosition>,
    pub skew_x: SharedDerivedF32,
    pub skew_y: SharedDerivedF32,
    pub skew_center_x: SharedDerived<InnerPosition>,
    pub skew_center_y: SharedDerived<InnerPosition>,
    pub visible: SharedDerivedBool,
    pub width: SharedDerivedSize,
}
macro_rules! bind_property {
    ($id:ident, $need_redraw:ident, $wc:ident, $property:expr_2021) => {{
        let need_redraw = $need_redraw.clone();
        let e = $wc.event_loop_proxy.clone();
        $property.subscribe($id, move || {
            need_redraw.lock().request();
            e.request_layout();
        });
    }};
}
macro_rules! bind_properties {
    ($id:ident, $need_redraw:ident, $wc:ident, $( $property:expr_2021 ),* ) => {
        $(
            bind_property!($id, $need_redraw, $wc, $property);
        )*
    };
}

impl ItemProps {
    pub fn bind(&self, id: u32, need_redraw: &Arc<Mutex<NeedRedraw>>, wc: &WindowContext) {
        self.name.get().with_mut(|name| {
            if !name.is_empty() {
                bind_str_to_id(name.as_str(), id);
            }
        });
        bind_properties!(
            id,
            need_redraw,
            wc,
            self.blur,
            self.clipped,
            self.enable,
            self.enable_background_blur,
            self.focusable,
            self.focus_requester,
            self.height,
            self.layout_direction,
            self.max_height,
            self.max_width,
            self.min_height,
            self.min_width,
            self.offset_x,
            self.offset_y,
            self.opacity,
            self.rotation,
            self.rotation_center_x,
            self.scale_x,
            self.scale_y,
            self.scale_center_x,
            self.scale_center_y,
            self.visible,
            self.width
        );
    }
}

impl ItemProps {
    pub fn new(window_context: &WindowContext) -> Self {
        let width = SharedDerivedSize::from(Size::Auto);
        let height = SharedDerivedSize::from(Size::Auto);
        let need_redraw = Arc::new(Mutex::new(NeedRedraw {
            is_fixed_size: SharedDerived::from_fn(depend!(width, height), {
                let width = width.clone();
                let height = height.clone();
                move || {
                    let width = width.get();
                    let height = height.get();
                    matches!(
                        (width, height),
                        (Size::Fixed(_), Size::Fixed(_))
                            | (Size::Fill, Size::Fill)
                            | (Size::Fixed(_), Size::Fill)
                            | (Size::Fill, Size::Fixed(_))
                    )
                }
            }),
            need_redraw: true,
            parent: None,
        }));
        Self {
            window_context: window_context.clone(),
            background: SharedItem::none(),
            baseline: None.into(),
            blur: 35.0.into(),
            clipped: true.into(),
            clip_shape: {
                let shape: Box<dyn Fn(&Frame) -> Path> = Box::new(|frame: &Frame| {
                    let mut path = Path::new();
                    path.add_rect(
                        Rect::from_xywh(
                            frame.x(),
                            frame.y(),
                            frame.width(),
                            frame.height(),
                        ),
                        None,
                    );
                    path
                });
                SharedDerived::from(Some(shape))
            },
            custom_props: HashMap::new(),
            enable: true.into(),
            enable_background_blur: false.into(),
            focusable: true.into(),
            focus_requester: Default::default(),
            foreground: SharedItem::none(),
            height,
            item_state: ItemState::Enabled.into(),
            layout_direction: LayoutDirection::LTR.into(),
            max_height: f32::INFINITY.into(),
            max_width: f32::INFINITY.into(),
            min_height: 0.0.into(),
            min_width: 0.0.into(),
            name: "".into(),
            need_redraw,
            on_click: None,
            on_cursor_move: None,
            on_focus_changed: None,
            on_hover_changed: None,
            on_mouse_input: None,
            on_pointer_input: None,
            offset_x: 0.0.into(),
            offset_y: 0.0.into(),
            opacity: 1.0.into(),
            padding: Padding::default(),
            rotation: 0.0.into(),
            rotation_center_x: InnerPosition::Middle(0.0).into(),
            rotation_center_y: InnerPosition::Middle(0.0).into(),
            scale_x: 1.0.into(),
            scale_y: 1.0.into(),
            scale_center_x: InnerPosition::Middle(0.0).into(),
            scale_center_y: InnerPosition::Middle(0.0).into(),
            skew_x: 0.0.into(),
            skew_y: 0.0.into(),
            skew_center_x: InnerPosition::Middle(0.0).into(),
            skew_center_y: InnerPosition::Middle(0.0).into(),
            visible: true.into(),
            width,
        }
    }
}

/*pub trait ItemPropsTrait {
    fn item_props(&self) -> ItemProps;
}
impl ItemPropsTrait for WindowContext {
    fn item_props(&self) -> ItemProps {
        ItemProps::new(self)
    }
}*/

#[macro_export]
macro_rules! impl_setter {
    (
        $struct_name:ty;$($prop_name:ident:$prop_type:ty),*
    ) => {
        impl $struct_name {
            $(
                pub fn $prop_name(mut self, $prop_name: impl Into<$prop_type>) -> Self {
                    self.$prop_name = $prop_name.into();
                    self
                }
            )*
        }
    }
}

#[macro_export]
macro_rules! define_props {
    (
        $trait_name:ident;
        $func_name:ident;
        $struct_name:ident{
            $($prop_name:ident:$prop_type:ty),* $(,)?
        }
    ) => {
        $crate::define_props!(
            $trait_name;
            $func_name;
            $struct_name{
            $($prop_name:$prop_type),*
        }{});
    };
    (
        $trait_name:ident;
        $func_name:ident;
        $struct_name:ident{
            $($prop_name:ident:$prop_type:ty),* $(,)?
        }
        {
            $($prop_name2:ident:$prop_type2:ty),* $(,)?
        }
    ) => {
        pub struct $struct_name {
            pub item_props: $crate::ui::item::ItemProps,
            $(pub $prop_name: $prop_type,)*

            $(pub $prop_name2: $prop_type2,)*
        }
        
        $crate::impl_setter!($struct_name; $($prop_name:$prop_type),*);
        
        $crate::impl_item_props!($struct_name);

        // impl core::convert::From<$crate::ui::item::ItemProps> for $struct_name {
        //     fn from(item_props: $crate::ui::item::ItemProps) -> Self {
        //         $struct_name::new(item_props)
        //     }
        // }

        pub trait $trait_name {
            fn $func_name(&self) -> $struct_name;
        }
        impl $trait_name for $crate::app::WindowContext{
            fn $func_name(&self) -> $struct_name {
                $struct_name::new($crate::ui::item::ItemProps::new(self))
            }
        }
        
        impl std::ops::Deref for $struct_name {
            type Target = $crate::ui::item::ItemProps;
            fn deref(&self) -> &Self::Target {
                &self.item_props
            }
        }
        
        impl $crate::ui::item::SetCustomProp for $struct_name {
            fn set_custom_prop<T: 'static>(
                &mut self,
                name: impl Into<String>,
                value: impl Into<$crate::shared::SharedDerived<T>>,
            ) {
                self.item_props.set_custom_prop(name, value);
            }
        }
    }
}

/*impl_setter! (
    ItemProps;
    blur: SharedDerivedF32,
    clipped:SharedDerivedBool,
    clip_shape:SharedDerived<Option<Box<dyn Fn(&Frame)-> Path>>>,
    background:SharedItem,
    enable:SharedDerived<bool>,
    enable_background_blur: SharedDerivedBool,
    focusable:SharedDerived<bool>,
    focus_requester:SharedDerived<FocusRequester>,
    foreground:SharedItem,
    height:SharedDerivedSize,
    max_height:SharedDerivedF32,
    max_width:SharedDerivedF32,
    min_height:SharedDerivedF32,
    min_width:SharedDerivedF32,
    name:SharedDerived<String>,
    offset_x:SharedDerivedF32,
    offset_y:SharedDerivedF32,
    opacity:SharedDerivedF32,
    rotation:SharedDerivedF32,
    rotation_center_x:SharedDerived<InnerPosition>,
    rotation_center_y:SharedDerived<InnerPosition>,
    scale_x:SharedDerivedF32,
    scale_y:SharedDerivedF32,
    scale_center_x:SharedDerived<InnerPosition>,
    scale_center_y:SharedDerived<InnerPosition>,
    skew_x:SharedDerivedF32,
    skew_y:SharedDerivedF32,
    visible:SharedDerivedBool,
    width:SharedDerivedSize
);*/

/*impl ItemProps {
    pub fn size(
        self,
        width: impl Into<SharedDerivedSize>,
        height: impl Into<SharedDerivedSize>,
    ) -> Self {
        self.width(width).height(height)
    }

    pub fn padding(mut self, padding: Padding) -> Self {
        self.padding = padding;
        self
    }

    pub fn offset(
        self,
        offset_x: impl Into<SharedDerivedF32>,
        offset_y: impl Into<SharedDerivedF32>,
    ) -> Self {
        self.offset_x(offset_x).offset_y(offset_y)
    }
}
*/
impl ItemProps {
    pub fn clamp_width(&self, width: f32) -> f32 {
        let min_width = self.min_width.get();
        let max_width = self.max_width.get();
        if width < min_width {
            min_width
        } else if width > max_width {
            max_width
        } else {
            width
        }
            .clamp(0.0, f32::INFINITY)
    }

    pub fn clamp_height(&self, height: f32) -> f32 {
        let min_height = self.min_height.get();
        let max_height = self.max_height.get();
        if height < min_height {
            min_height
        } else if height > max_height {
            max_height
        } else {
            height
        }
            .clamp(0.0, f32::INFINITY)
    }

    pub fn set_custom_prop<T: 'static>(
        &mut self,
        name: impl Into<String>,
        value: impl Into<SharedDerived<T>>,
    ) {
        let value = value.into();
        let value: Box<dyn Any> = Box::new(value);
        self.custom_props.insert(name.into(), value);
    }

    pub fn get_custom<T: 'static>(&self, name: &str) -> Option<&T> {
        self.custom_props
            .get(name)
            .and_then(|v| v.downcast_ref::<T>())
    }

/*    pub fn on_click<F: 'static + FnMut(&ClickSource)>(mut self, f: F) -> Self {
        self.on_click = Some(Box::new(f));
        self
    }

    pub fn on_focus_changed<F: 'static + FnMut(&FocusState)>(mut self, f: F) -> Self {
        self.on_focus_changed = Some(Box::new(f));
        self
    }

    pub fn on_mouse_input<F: 'static + FnMut(&MouseInput) -> bool>(mut self, f: F) -> Self {
        self.on_mouse_input = Some(Box::new(f));
        self
    }

    pub fn on_pointer_input<F: 'static + FnMut(&PointerInput) -> bool>(mut self, f: F) -> Self {
        self.on_pointer_input = Some(Box::new(f));
        self
    }*/
}

pub trait SetCustomProp {
    fn set_custom_prop<T: 'static>(
        &mut self,
        name: impl Into<String>,
        value: impl Into<SharedDerived<T>>,
    );
}



#[macro_export]
macro_rules! impl_item_props {
    ($name:ty) => {
        $crate::base_impl_item_props!(
            $name;
            blur: $crate::shared::SharedDerivedF32,
            clipped:$crate::shared::SharedDerivedBool,
            clip_shape:$crate::shared::SharedDerived<Option<Box<dyn Fn(&$crate::ui::item::Frame)-> skia_safe::Path>>>,
            background:$crate::shared::SharedItem,
            enable:$crate::shared::SharedDerived<bool>,
            enable_background_blur: $crate::shared::SharedDerivedBool,
            focusable:$crate::shared::SharedDerived<bool>,
            focus_requester:$crate::shared::SharedDerived<$crate::ui::item::FocusRequester>,
            foreground:$crate::shared::SharedItem,
            height:$crate::shared::SharedDerivedSize,
            max_height:$crate::shared::SharedDerivedF32,
            max_width:$crate::shared::SharedDerivedF32,
            min_height:$crate::shared::SharedDerivedF32,
            min_width:$crate::shared::SharedDerivedF32,
            name:$crate::shared::SharedDerived<String>,
            offset_x:$crate::shared::SharedDerivedF32,
            offset_y:$crate::shared::SharedDerivedF32,
            opacity:$crate::shared::SharedDerivedF32,
            rotation:$crate::shared::SharedDerivedF32,
            rotation_center_x:$crate::shared::SharedDerived<$crate::ui::InnerPosition>,
            rotation_center_y:$crate::shared::SharedDerived<$crate::ui::InnerPosition>,
            scale_x:$crate::shared::SharedDerivedF32,
            scale_y:$crate::shared::SharedDerivedF32,
            scale_center_x:$crate::shared::SharedDerived<$crate::ui::InnerPosition>,
            scale_center_y:$crate::shared::SharedDerived<$crate::ui::InnerPosition>,
            skew_x:$crate::shared::SharedDerivedF32,
            skew_y:$crate::shared::SharedDerivedF32,
            visible:$crate::shared::SharedDerivedBool,
            width:$crate::shared::SharedDerivedSize
        );
    }
}

#[macro_export]
macro_rules! base_impl_item_props {
    (
        $struct_name:ty;$($prop_name:ident:$prop_type:ty),*
    ) => {
        impl $struct_name {
            $(
                pub fn $prop_name(mut self, $prop_name: impl Into<$prop_type>) -> Self {
                    self.item_props.$prop_name = $prop_name.into();
                    self
                }
            )*
        }
        
        impl $struct_name {
            pub fn offset(
                self,
                offset_x: impl Into<$crate::shared::SharedDerivedF32>,
                offset_y: impl Into<$crate::shared::SharedDerivedF32>,
            ) -> Self {
                self.offset_x(offset_x).offset_y(offset_y)
            }
            
            pub fn padding(mut self, padding: $crate::ui::item::Padding) -> Self {
                self.item_props.padding = padding;
                self
            }
            
            pub fn size(
                self,
                width: impl Into<$crate::shared::SharedDerivedSize>,
                height: impl Into<$crate::shared::SharedDerivedSize>,
            ) -> Self {
                self.width(width).height(height)
            }
        }
        
        impl $struct_name {
            pub fn on_click<F: 'static + FnMut(&$crate::ui::item::ClickSource)>(mut self, f: F) -> Self {
                self.item_props.on_click = Some(Box::new(f));
                self
            }
            
            pub fn on_focus_changed<F: 'static + FnMut(&$crate::ui::item::FocusState)>(mut self, f: F) -> Self {
                self.item_props.on_focus_changed = Some(Box::new(f));
                self
            }
            
            pub fn on_mouse_input<F: 'static + FnMut(&$crate::ui::item::MouseInput) -> bool>(mut self, f: F) -> Self {
                self.item_props.on_mouse_input = Some(Box::new(f));
                self
            }
            
            pub fn on_pointer_input<F: 'static + FnMut(&$crate::ui::item::PointerInput) -> bool>(mut self, f: F) -> Self {
                self.item_props.on_pointer_input = Some(Box::new(f));
                self
            }
        }
    };
}
