use proc_macro::TokenStream;
use proc_macro2::Ident;
use quote::quote;
use syn::{parse_macro_input, parse_quote, Fields, ItemStruct};

#[proc_macro_attribute]
pub fn item(_: TokenStream, input: TokenStream) -> TokenStream {
    let input_clone = input.clone();
    let mut ast = parse_macro_input!(input_clone as ItemStruct);
    let name = ast.ident.clone();

    let fields = match &mut ast.fields {
        Fields::Named(fields) => &mut fields.named,
        Fields::Unnamed(fields) => &mut fields.unnamed,
        Fields::Unit => panic!("Unit struct is not supported"),
    };

    let struct_item: ItemStruct = parse_quote!(
        pub struct T {
            pub(crate) id: usize,
            pub(crate) path: crate::item::ItemPath,
            pub(crate) app: crate::app::SharedApp,
            pub(crate) children: std::vec::Vec<crate::item::Item>,
            pub(crate) layout_direction: crate::item::LayoutDirection,
            pub(crate) active: crate::property::BoolProperty,
            pub(crate) focusable: crate::property::BoolProperty,
            pub(crate) focused: crate::property::BoolProperty,
            pub(crate) focusable_when_clicked: crate::property::BoolProperty,
            pub(crate) is_cursor_inside: bool,
            pub(crate) width: crate::property::SizeProperty,
            pub(crate) height: crate::property::SizeProperty,
            pub(crate) min_width: crate::property::FloatProperty,
            pub(crate) min_height: crate::property::FloatProperty,
            pub(crate) max_width: crate::property::FloatProperty,
            pub(crate) max_height: crate::property::FloatProperty,
            pub(crate) padding_start: crate::property::FloatProperty,
            pub(crate) padding_top: crate::property::FloatProperty,
            pub(crate) padding_end: crate::property::FloatProperty,
            pub(crate) padding_bottom: crate::property::FloatProperty,
            pub(crate) margin_start: crate::property::FloatProperty,
            pub(crate) margin_top: crate::property::FloatProperty,
            pub(crate) margin_end: crate::property::FloatProperty,
            pub(crate) margin_bottom: crate::property::FloatProperty,
            pub(crate) background: crate::property::ItemProperty,
            pub(crate) foreground: crate::property::ItemProperty,
            pub(crate) layout_params: crate::item::LayoutParams,
            pub(crate) on_click: Option<Box<dyn Fn()>>,
            pub(crate) on_pointer_input: Option<Box<dyn Fn(crate::item::PointerAction)>>,
            pub(crate) on_focus: Option<Box<dyn Fn()>>,
            pub(crate) on_blur: Option<Box<dyn Fn()>>,
        }
    );

    struct_item.fields.iter().for_each(|field| {
        fields.push(field.clone());
    });

    fn bind_property(item_name: &Ident, name: &str, property_type: &str) -> proc_macro2::TokenStream {
        let item_name = item_name.clone();
        let name = Ident::new(name, proc_macro2::Span::mixed_site());
        let property_type = Ident::new(property_type, proc_macro2::Span::mixed_site());
        quote!(
            impl #item_name{
                pub fn #name(mut self, value: impl Into<crate::property::#property_type>) -> Self{
                    self.#name = value.into();
                    let app = self.app.clone();
                    self.#name.lock().add_observer(
                        crate::property::Observer::new_without_id(move ||{
                        app.need_re_layout();
                    }));
                    self
                }
            }
        )
    }

    let property_bindings: Vec<proc_macro2::TokenStream> = vec!(
        bind_property(&name, "active", "BoolProperty"),
        bind_property(&name, "focusable", "BoolProperty"),
        bind_property(&name, "focused", "BoolProperty"),
        bind_property(&name, "focusable_when_clicked", "BoolProperty"),
        bind_property(&name, "width", "SizeProperty"),
        bind_property(&name, "height", "SizeProperty"),
        bind_property(&name, "min_width", "FloatProperty"),
        bind_property(&name, "min_height", "FloatProperty"),
        bind_property(&name, "max_width", "FloatProperty"),
        bind_property(&name, "max_height", "FloatProperty"),
        bind_property(&name, "padding_start", "FloatProperty"),
        bind_property(&name, "padding_top", "FloatProperty"),
        bind_property(&name, "padding_end", "FloatProperty"),
        bind_property(&name, "padding_bottom", "FloatProperty"),
        bind_property(&name, "margin_start", "FloatProperty"),
        bind_property(&name, "margin_top", "FloatProperty"),
        bind_property(&name, "margin_end", "FloatProperty"),
        bind_property(&name, "margin_bottom", "FloatProperty"),
        bind_property(&name, "background", "ItemProperty"),
        bind_property(&name, "foreground", "ItemProperty"),
    );


    quote!(
        #ast

        #(
            #property_bindings
        )*

        impl #name{

            pub fn id(mut self, name: &str) -> Self{
                self.id = self.app.id(name);
                self
            }

            pub fn layout_direction(mut self, layout_direction: crate::item::LayoutDirection) -> Self{
                self.layout_direction = layout_direction;
                self
            }


            pub fn children(mut self, children: std::collections::LinkedList<crate::item::Item>) -> Self{
                for mut child in children{
                    let mut path=self.path.clone();
                    path.push(self.children.len());
                    child.set_path(path);
                    self.children.push(child);
                }
                self
            }

            pub fn add_child(mut self, mut child: crate::item::Item) -> Self{
                let mut path=self.path.clone();
                path.push(self.children.len());
                child.set_path(path);
                self.children.push(child);
                self
            }

            pub fn on_click(mut self, on_click: impl Fn() + 'static)->Self{
                self.on_click = Some(Box::new(on_click));
                self
            }

            pub fn on_pointer_input(mut self, on_pointer_input: impl Fn(crate::item::PointerAction) + 'static)->Self{
                self.on_pointer_input = Some(Box::new(on_pointer_input));
                self
            }

            fn start_timer(&self, msg:&str, duration: std::time::Duration)->crate::item::Timer{
                crate::item::Timer::start(&self.app, &self.path, msg, duration)
            }

            fn logical_x(&self, x: f32)->crate::item::LogicalX{
                crate::item::LogicalX::new(x, self.app.content_width(), self.layout_direction)
            }

        }

        impl crate::item::ItemTrait for #name{

            fn get_id(&self) -> usize{
                self.id
            }

            fn get_path(&self) -> &crate::item::ItemPath{
                &self.path
            }

            fn set_path(&mut self, path: crate::item::ItemPath){
                self.path=path;
            }

            fn get_children(&self)->&std::vec::Vec<crate::item::Item>{
                &self.children
            }

            fn get_children_mut(&mut self)->&mut std::vec::Vec<crate::item::Item>{
                &mut self.children
            }

            fn request_focus(&self){
                self.app.request_focus(&self.path);
            }

            fn get_active(&self)->crate::property::BoolProperty{
                self.active.clone()
            }

            fn get_focusable(&self)->crate::property::BoolProperty{
                self.focusable.clone()
            }

            fn get_focused(&self)->crate::property::BoolProperty{
                self.focused.clone()
            }

            fn get_focusable_when_clicked(&self)->crate::property::BoolProperty{
                self.focusable_when_clicked.clone()
            }
            
            fn get_width(&self) -> crate::property::SizeProperty{
                self.width.clone()
            }

            fn get_height(&self) -> crate::property::SizeProperty{
                self.height.clone()
            }

            fn get_padding_start(&self) -> crate::property::FloatProperty{
                self.padding_start.clone()
            }

            fn get_padding_top(&self) -> crate::property::FloatProperty{
                self.padding_top.clone()
            }

            fn get_padding_end(&self) -> crate::property::FloatProperty{
                self.padding_end.clone()
            }

            fn get_padding_bottom(&self) -> crate::property::FloatProperty{
                self.padding_bottom.clone()
            }

            fn get_margin_start(&self) -> crate::property::FloatProperty{
                self.margin_start.clone()
            }

            fn get_margin_top(&self) -> crate::property::FloatProperty{
                self.margin_top.clone()
            }

            fn get_margin_end(&self) -> crate::property::FloatProperty{
                self.margin_end.clone()
            }

            fn get_margin_bottom(&self) -> crate::property::FloatProperty{
                self.margin_bottom.clone()
            }

            fn get_layout_params(&self) -> &crate::item::LayoutParams{
                &self.layout_params
            }

            fn get_layout_params_mut(&mut self) -> &mut crate::item::LayoutParams{
                &mut self.layout_params
            }

            fn set_layout_params(&mut self, layout_params: crate::item::LayoutParams){
                self.layout_params=layout_params;
            }

            fn get_on_click(&self) -> Option<&Box<dyn Fn() + 'static>>{
                self.on_click.as_ref()
            }

            fn get_on_pointer_input(&self) -> Option<&Box<dyn Fn(crate::item::PointerAction) + 'static>>{
                self.on_pointer_input.as_ref()
            }

            fn get_on_focus(&self) -> Option<&Box<dyn Fn() + 'static>>{
                self.on_focus.as_ref()
            }

            fn get_on_blur(&self) -> Option<&Box<dyn Fn() + 'static>>{
                self.on_blur.as_ref()
            }
        }

    )
        .into()
}