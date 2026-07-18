

use inflector::Inflector;
use proc_macro::{TokenStream, TokenTree};
use proc_macro2::{Ident, Span};
use proc_macro_crate::{crate_name, FoundCrate};
use quote::{quote, ToTokens};
use syn::parse::{Parse, ParseStream};
use syn::{parse_macro_input, BinOp, DeriveInput, Expr, Fields, GenericParam, ItemStruct, Type};

struct Args {
    names: Vec<Ident>,
    types: Vec<Type>,
}

impl Parse for Args {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut names = Vec::new();
        let mut types = Vec::new();
        while !input.is_empty() {
            if !names.is_empty() {
                let _: syn::Token![,] = input.parse()?;
            }
            let name: Ident = input.parse()?;
            let _: syn::Token![:] = input.parse()?;
            let ty: Type = input.parse()?;
            names.push(name);
            types.push(ty);
        }
        Ok(Args { names, types })
    }
}

#[proc_macro_attribute]
pub fn item(attr: TokenStream, input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as Args);
    let (names, types) = (args.names, args.types);
    let input = parse_macro_input!(input as ItemStruct);
    let name = input.ident.clone();
    let generics = input.generics.clone();
    let generics_name = {
        let mut generics = generics.clone();
        generics.params.iter_mut().for_each(|param| {
            if let GenericParam::Type(type_param) = param {
                type_param.colon_token = None;
                type_param.bounds.clear();
            }
        });
        generics
    };
    let ext_name = format!("{}Ext", name);
    let ext_ident = Ident::new(&ext_name, Span::call_site());
    let ext_fn = Ident::new(name.to_string().to_snake_case().as_str(), Span::call_site());
    let output = quote! {
        #input
        impl #generics #name #generics_name {
            pub fn item(self) -> Item {
                self.item
            }
        }
        impl #generics Into<Item> for #name #generics_name {
            fn into(self) -> Item {
                self.item
            }
        }
        pub trait #ext_ident #generics {
            fn #ext_fn(&self, #(#names: #types),*) -> #name #generics_name;
        }
        impl #generics #ext_ident #generics_name for &WindowContext {
            fn #ext_fn(&self, #(#names: #types),*) -> #name #generics_name {
                #name::new(*self, #(#names),*)
            }
        }

        impl #generics #ext_ident #generics_name for WindowContext {
            fn #ext_fn(&self, #(#names: #types),*) -> #name #generics_name {
                #name::new(self, #(#names),*)
            }
        }
    };
    output.into()
}

#[proc_macro_attribute]
pub fn observable(_attr: TokenStream, input: TokenStream) -> TokenStream {
    let mut input = parse_macro_input!(input as ItemStruct);
    let struct_name = input.ident.clone();
    let fields = match &input.fields {
        syn::Fields::Named(fields) => &fields.named,
        _ => {
            return syn::Error::new(
                struct_name.span(),
                "only named fields are supported; tuple structs and unit structs are not allowed",
            )
            .to_compile_error()
            .into();
        }
    };

    let field_names: Vec<_> = fields
        .iter()
        .map(|field| field.ident.clone().unwrap())
        .collect();
    let field_types: Vec<_> = fields.iter().map(|field| field.ty.clone()).collect();
    // let set_field_names: Vec<_> = field_names
    //     .iter()
    //     .map(|name| Ident::new(&format!("set_{}", name), Span::call_site()))
    //     .collect();
    let get_field_names: Vec<_> = field_names
        .iter()
        .map(|name| Ident::new(&format!("get_{}", name), Span::call_site()))
        .collect();

    if let Fields::Named(fields) = &mut input.fields {
        let m_fields: TokenStream = quote! {
            struct S {
                id: usize,
                observers: Arc<Mutex<Vec<(usize, Box<dyn FnMut() + Send>)>>>,
            }
        }
            .into();
        let a = parse_macro_input!(m_fields as ItemStruct);
        for field in a.fields.iter() {
            fields.named.insert(0, field.clone());
        }
    }

    let output = quote! {
        #input
        impl #struct_name {
            pub fn new(#(#field_names: impl Into<#field_types>),*) -> Self {
                #(
                    let #field_names = #field_names.into();
                )*
                let id = generate_id();

                let mut self_ = Self {
                    id,
                    observers: Arc::new(Mutex::new(Vec::new())),
                    #(
                        #field_names,
                    )*
                };
                #(
                    self_.#field_names.add_observer(id, {
                        let observers = self_.observers.clone();
                        Box::new(move || {
                            for (_, observer) in observers.lock().iter_mut() {
                                observer();
                            }
                        })
                    });
                )*
                self_
            }

            pub fn add_observer(&self, id: usize, observer: Box<dyn FnMut() + Send>) {
                self.observers.lock().push((id, observer));
            }

            pub fn remove_observer(&self, id: usize) {
                let mut observers = self.observers.lock();
                observers.retain(|(observer_id, _)| *observer_id != id);
            }

            #(
                pub fn #field_names(&mut self, #field_names: impl Into<#field_types>) -> &mut Self {
                    self.#field_names.remove_observer(self.id);
                    let mut #field_names = #field_names.into();
                    #field_names.add_observer(self.id, {
                        let observers = self.observers.clone();
                        Box::new(move || {
                            for (_, observer) in observers.lock().iter_mut() {
                                observer();
                            }
                        })
                    });
                    self.#field_names = #field_names;
                    for (_, observer) in self.observers.lock().iter_mut() {
                        observer();
                    }
                    self
                }
            )*

            #(
                pub fn #get_field_names(&self) -> #field_types {
                    self.#field_names.clone()
                }
            )*
        }

        impl Observable for #struct_name {
            fn add_observer(&mut self, id: usize, observer: Box<dyn FnMut() + Send>) -> Removal {
                self.observers.lock().push((id, observer));
                let observers = self.observers.clone();
                Removal::new(move || {
                    observers.lock().retain(|(i, _)| *i != id);
                })
            }
        }
    };
    output.into()
}

/// Implement `AsRef` for the struct or enum
#[proc_macro_derive(AsRef)]
pub fn derive_as_ref(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let output = quote! {
        impl AsRef<#name> for #name {
            fn as_ref(&self) -> &#name {
                self
            }
        }
    };

    output.into()
}

#[proc_macro_attribute]
pub fn style(_attr: TokenStream, input: TokenStream) -> TokenStream {
    let crare_name = match crate_name("winia") {
        Ok(found_crate) => {
            match found_crate {
                FoundCrate::Itself => "crate".to_string(),
                FoundCrate::Name(name) => {
                    name
                }
            }
        }
        Err(err) => {
            return syn::Error::new(Span::call_site(), err.to_string())
                .to_compile_error()
                .into();
        }
    };
    let crate_ident = Ident::new(&crare_name, Span::call_site());

    let mut input = parse_macro_input!(input as ItemStruct);
    let struct_name = input.ident.clone();

    let fields = match &mut input.fields {
        Fields::Named(fields) => &mut fields.named,
        _ => {
            return syn::Error::new(
                struct_name.span(),
                "only named fields are supported; tuple structs and unit structs are not allowed",
            )
            .to_compile_error()
            .into();
        }
    };

    // name, type, set_xxx, get_xxx, get_xxx (for theme)
    let (field_names, field_types, new_types, get_field_names, get_theme_field_names, set_field_names) = {
        let mut field_names = Vec::new();
        let mut field_types = Vec::new();
        let mut new_types = Vec::new();
        let mut get_field_names = Vec::new();
        let mut get_theme_field_names = Vec::new();
        let mut set_field_names = Vec::new();
        for field in fields.iter() {
            let field_name = field.ident.as_ref().unwrap().to_string();
            let field_type = field.ty.to_token_stream().to_string();
            let new_type = format!("{}::theme::ThemeValue<{}>", crare_name, field_type);
            let get_field_name = format!("get_{}", field_name);
            let set_field_name = format!("set_{}", field_name);
            //Some(theme.get_style(name)?.downcast_ref::<f32>()?)
            let get_theme_field_name = match field_type.as_str() {
                "f32" => "theme.get_dimension(name)".to_string(),
                "Color" => "theme.get_color(name)".to_string(),
                "bool" => "theme.get_bool(name)".to_string(),
                _ => "theme.get_style(name)".to_string(),
            };
            field_names.push(field.ident.clone().unwrap());
            field_types.push(field.ty.clone());
            new_types.push(syn::parse_str::<Type>(&new_type).unwrap());
            get_field_names.push(Ident::new(&get_field_name, Span::call_site()));
            get_theme_field_names.push(
                syn::parse_str::<Expr>(&get_theme_field_name).unwrap()
            );
            set_field_names.push(Ident::new(&set_field_name, Span::call_site()));
        }
        (
            field_names,
            field_types,
            new_types,
            get_field_names,
            get_theme_field_names,
            set_field_names
        )
    };

    let struct_name = input.ident.clone();
    let ext_name = Ident::new(&format!("{}Ext", struct_name), Span::call_site());
    let get_style_name = Ident::new(&format!("get_{}", struct_name.to_string().to_snake_case()), Span::call_site());
    let set_style_name = Ident::new(&format!("set_{}", struct_name.to_string().to_snake_case()), Span::call_site());

    let output = quote! {
        #[derive(Clone)]
        pub struct #struct_name {
            #(
                pub #field_names: #new_types,
            )*
        }

        impl #struct_name {
            #(
                pub fn #set_field_names(&mut self, #field_names: impl Into<#new_types>) -> &mut Self {
                    self.#field_names = #field_names.into();
                    self
                }

                pub fn #get_field_names<'a>(&'a self, theme: &'a #crate_ident::Theme) -> Option<&'a #field_types> {
                    match &self.#field_names {
                        #crate_ident::theme::ThemeValue::Ref(name) => #get_theme_field_names,
                        #crate_ident::theme::ThemeValue::Direct(value) => Some(value),
                    }
                }
            )*
        }

        pub trait #ext_name {
            fn #get_style_name(&self, key: impl Into<String>, item_state: #crate_ident::ui::item::ItemState) -> Option<&#struct_name>;
            fn #set_style_name(&mut self, key: impl Into<String>, style: #crate_ident::theme::StateStyles<#struct_name>);
        }

        impl #ext_name for #crate_ident::Theme {
            fn #get_style_name(&self, key: impl Into<String>, item_state: #crate_ident::ui::item::ItemState) -> Option<&#struct_name> {
                let style: &#crate_ident::theme::StateStyles<#struct_name> = self.get_style(key)?;
                Some(style.get(item_state))
            }

            fn #set_style_name(&mut self, key: impl Into<String>, style: #crate_ident::theme::StateStyles<#struct_name>) {
                let style: Box<dyn std::any::Any + Send> = Box::new(style);
                self.set_style(key, style);
            }
        }
    };
    output.into()
}

#[proc_macro_derive(ItemProps, attributes(constructor, not_shared))]
pub fn item_props(input: TokenStream) -> TokenStream {
    let crare_name = match crate_name("winia") {
        Ok(found_crate) => {
            match found_crate {
                FoundCrate::Itself => "crate".to_string(),
                FoundCrate::Name(name) => {
                    name
                }
            }
        }
        Err(err) => {
            return syn::Error::new(Span::call_site(), err.to_string())
                .to_compile_error()
                .into();
        }
    };
    let crate_ident = Ident::new(&crare_name, Span::call_site());

    let input = parse_macro_input!(input as ItemStruct);
    let name = input.ident.clone();
    let generics = input.generics.clone();
    let generics_name = {
        let mut generics = generics.clone();
        generics.params.iter_mut().for_each(|param| {
            if let GenericParam::Type(type_param) = param {
                type_param.colon_token = None;
                type_param.bounds.clear();
            }
        });
        generics
    };

    let mut names = vec![];
    let mut types = vec![];
    let mut settable_field_names = vec![];
    let mut settable_field_types = vec![];
    'out: for field in input.fields.iter() {
        if let Some(ident) = &field.ident && ident == "item_props" {
            continue 'out;
        }
        let mut is_not_shared = false;
        let mut is_constructor = false;
        let mut new_type: Option<Type> = None;
        for attr in field.attrs.iter() {
            if attr.path().is_ident("not_shared") {
                is_not_shared = true;
            } else if attr.path().is_ident("constructor") {
                is_constructor = true;
                if attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident("type") {
                        let value = meta.value()?;
                        let s: Type = value.parse()?;
                        new_type = Some(s);
                        Ok(())
                    } else {
                        Err(meta.error("expected `type`"))
                    }
                }).is_err() {
                    new_type = Some(syn::parse_str::<Type>(
                        format!("impl Into<{}>", field.ty.to_token_stream()).as_str()
                    ).unwrap());
                }
            }
        }
        if let Some(new_type) = new_type && is_constructor {
            names.push(field.ident.clone().unwrap());
            types.push(new_type);
        }
        if !is_not_shared {
            settable_field_names.push(field.ident.clone().unwrap());
            settable_field_types.push(field.ty.clone())
        }
    }

    let clip_shape_type = format!(
        "SharedDerived<Option<Box<dyn Fn(&{}::ui::item::Frame) -> skia_safe::Path>>>",
        crare_name
    );
    let focus_requester_type = format!(
        "SharedDerived<{}::ui::item::FocusRequester>",
        crare_name
    );
    let center_type = format!(
        "SharedDerived<{}::ui::InnerPosition>",
        crare_name
    );
    let (base_prop_names, base_prop_types) = base_item_props(
        &crare_name,
        vec![
            ("blur", "SharedDerivedF32"),
            ("clipped", "SharedDerivedBool"),
            (
                "clip_shape",
                &clip_shape_type,
            ),
            ("background", "SharedItem"),
            ("enable", "SharedDerived<bool>"),
            ("enable_background_blur", "SharedDerivedBool"),
            ("focusable", "SharedDerived<bool>"),
            ("focus_requester", &focus_requester_type),
            ("foreground", "SharedItem"),
            ("height", "SharedDerivedSize"),
            ("max_height", "SharedDerivedF32"),
            ("max_width", "SharedDerivedF32"),
            ("min_height", "SharedDerivedF32"),
            ("min_width", "SharedDerivedF32"),
            ("name", "SharedDerived<String>"),
            ("offset_x", "SharedDerivedF32"),
            ("offset_y", "SharedDerivedF32"),
            ("opacity", "SharedDerivedF32"),
            ("rotation", "SharedDerivedF32"),
            ("rotation_center_x", &center_type),
            ("rotation_center_y", &center_type),
            ("scale_x", "SharedDerivedF32"),
            ("scale_y", "SharedDerivedF32"),
            ("scale_center_x", &center_type),
            ("scale_center_y", &center_type),
            ("skew_x", "SharedDerivedF32"),
            ("skew_y", "SharedDerivedF32"),
            ("visible", "SharedDerivedBool"),
            ("width", "SharedDerivedSize"),
        ],
    );

    let ext_name = format!("{}Trait", name);
    let ext_ident = Ident::new(&ext_name, Span::call_site());
    let ext_fn = Ident::new(name.to_string().to_snake_case().as_str(), Span::call_site());
    let output = quote! {

        impl #generics #name #generics_name {
            #(
                pub fn #settable_field_names(mut self, #settable_field_names: impl Into<#settable_field_types>) -> Self {
                    self.#settable_field_names = #settable_field_names.into();
                    self
                }
            )*

            #(
                pub fn #base_prop_names(mut self, #base_prop_names: impl Into<#base_prop_types>) -> Self {
                    self.#base_prop_names = #base_prop_names.into();
                    self
                }
            )*

            pub fn offset(
                self,
                offset_x: impl Into<#crate_ident::shared::SharedDerivedF32>,
                offset_y: impl Into<#crate_ident::shared::SharedDerivedF32>,
            ) -> Self {
                self.offset_x(offset_x).offset_y(offset_y)
            }

            pub fn padding(mut self, padding: #crate_ident::ui::item::Padding) -> Self {
                self.item_props.padding = padding;
                self
            }

            pub fn size(
                self,
                width: impl Into<#crate_ident::shared::SharedDerivedSize>,
                height: impl Into<#crate_ident::shared::SharedDerivedSize>,
            ) -> Self {
                self.width(width).height(height)
            }

            pub fn on_click<F: 'static + FnMut(&#crate_ident::event::ButtonSource)>(mut self, f: F) -> Self {
                self.item_props.on_click = Some(Box::new(f));
                self
            }
            
            pub fn on_destroy<F: 'static + FnMut()>(mut self, f: F) -> Self {
                self.item_props.on_destroy = Some(Box::new(f));
                self
            }

            pub fn on_focus_changed<F: 'static + FnMut(&#crate_ident::ui::item::FocusState)>(mut self, f: F) -> Self {
                self.item_props.on_focus_changed = Some(Box::new(f));
                self
            }
            
            pub fn on_mounted<F: 'static + FnMut(&mut #crate_ident::ui::item::ItemData)>(mut self, f: F) -> Self {
                let on_mounted: Box<dyn 'static + FnMut(&mut #crate_ident::ui::item::ItemData)> = Box::new(f);
                self.item_props.on_mounted.lock().push(on_mounted);
                self
            }

            pub fn on_pointer_button<F: 'static + FnMut(&#crate_ident::event::PointerButton) -> bool>(mut self, f: F) -> Self {
                self.item_props.on_pointer_button = Some(Box::new(f));
                self
            }

            pub fn on_pointer_moved<F: 'static + FnMut(&#crate_ident::event::PointerMoved) -> bool>(mut self, f: F) -> Self {
                self.item_props.on_pointer_moved = Some(Box::new(f));
                self
            }

            pub fn on_state_changed<F: 'static + FnMut(#crate_ident::shared::SharedSource<#crate_ident::ui::item::ItemState>, #crate_ident::ui::item::ItemState)>(mut self, f: F) -> Self {
                self.item_props.on_state_changed = Box::new(f);
                self
            }
            
            pub fn on_unmounted<F: 'static + FnMut()>(mut self, f: F) -> Self {
                self.item_props.on_unmounted = Some(Box::new(f));
                self
            }
        }

        pub trait #ext_ident #generics {
            fn #ext_fn(&self, #(#names: #types),*) -> #name #generics_name;
        }

/*        impl #generics #ext_ident #generics_name for &#crate_ident::app::WindowContext {
            fn #ext_fn(&self, #(#names: #types),*) -> #name #generics_name {
                #name::new(#crate_ident::ui::item::ItemProps::new(self), #(#names),*)
            }
        }*/

        impl #generics #ext_ident #generics_name for #crate_ident::app::WindowContext {
            fn #ext_fn(&self, #(#names: #types),*) -> #name #generics_name {
                #name::new(#crate_ident::ui::item::ItemProps::new(&self), #(#names),*)
            }
        }

        impl #generics std::ops::Deref for #name #generics_name {
            type Target = #crate_ident::ui::item::ItemProps;
            fn deref(&self) -> &Self::Target {
                &self.item_props
            }
        }

        impl #generics std::ops::DerefMut for #name #generics_name {
            fn deref_mut(&mut self) -> &mut Self::Target {
                &mut self.item_props
            }
        }

        impl #generics #crate_ident::ui::item::SetCustomProp for #name #generics_name {
            fn set_custom_prop<P: 'static>(
                &mut self,
                name: impl Into<String>,
                value: impl Into<#crate_ident::shared::SharedDerived<P>>,
            ) {
                self.item_props.set_custom_prop(name, value);
            }
        }
        /*pub trait ItemPropsTrait {
    fn bind(
        &self,
        e: &EventLoopProxy,
        id: u32,
        need_redraw: &Arc<Mutex<NeedRedraw>>,
    );
    fn to_item_props(self) -> ItemProps;
}
*/
        impl #generics #crate_ident::ui::item::ItemPropsTrait for #name #generics_name {
            fn bind(&self,id: u32) {
                self.item_props.bind(id);
                let e = self.window_context.event_loop_proxy().clone();
                let item_updater = self.item_updater.clone();
                #(
                    self.#settable_field_names.subscribe(id,
                        {
                            let item_updater = item_updater.clone();
                            let e = e.clone();
                            move ||{
                                item_updater.lock().request_update();
                                e.request_update_layout();
                            }
                        }
                    );
                )*
            }

            fn to_item_props(self) -> #crate_ident::ui::item::ItemProps {
                self.item_props
            }
        }
    };
    output.into()
}

fn base_item_props(crate_name: &String, name_and_types: Vec<(&str, &str)>) -> (Vec<Ident>, Vec<Type>) {
    let mut names = vec![];
    let mut types = vec![];
    for (name, ty) in name_and_types {
        let (ident, ty) = name_and_type(crate_name, name, ty);
        names.push(ident);
        types.push(ty);
    }
    (names, types)
}

fn name_and_type(crate_name: &String, name: &str, ty: &str) -> (Ident, Type) {
    let ident = Ident::new(name, Span::call_site());
    let type_str = format!("{}::shared::{}", crate_name, ty);
    let ty: Type = syn::parse_str(&type_str).unwrap();
    (ident, ty)
}


#[proc_macro_derive(FieldRef, attributes(not_ref))]
pub fn field_ref(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as ItemStruct);
    let struct_name = input.ident.clone();
    let fields = match &input.fields {
        syn::Fields::Named(fields) => &fields.named,
        _ => {
            return syn::Error::new(
                struct_name.span(),
                "only named fields are supported; tuple structs and unit structs are not allowed",
            )
            .to_compile_error()
            .into();
        }
    };

    let field_names: Vec<_> = fields
        .iter()
        .filter_map(|field| {
            for attr in field.attrs.iter() {
                if attr.path().is_ident("not_ref") {
                    return None;
                }
            }
            Some(field.ident.clone().unwrap())
        })
        .collect();
    let field_types: Vec<_> = fields
        .iter()
        .filter_map(|field| {
            for attr in field.attrs.iter() {
                if attr.path().is_ident("not_ref") {
                    return None;
                }
            }
            Some(field.ty.clone())
        })
        .collect();

    let output = quote! {
        impl #struct_name {
            pub fn as_ref(&self) -> #struct_name {
                #struct_name {
                    #(
                        #field_names: &self.#field_names,
                    )*
                }
            }
        }

        pub struct #struct_name<'a> {
            #(
                pub #field_names: &'a #field_types,
            )*
        }
    };
    output.into()
}

// Replace all keywords "move" to "use"
#[proc_macro_attribute]
pub fn closure_use(_args: TokenStream, input: TokenStream) -> TokenStream {
    process_token_stream(input)
}

fn process_token_stream(input: TokenStream) -> TokenStream {
    input.into_iter().map(|token_tree|{
        match token_tree {
            TokenTree::Group(group) => {
                let new_stream = process_token_stream(group.stream());
                let mut new_group = proc_macro::Group::new(group.delimiter(), new_stream);
                new_group.set_span(group.span());
                TokenTree::Group(new_group)
            }
            TokenTree::Ident(ident) if ident.to_string() == "move" => {
                TokenTree::Ident(proc_macro::Ident::new("use", ident.span()))
            }
            other => {
                other
            }
        }
    }).collect()
}

#[proc_macro]
pub fn shared_derived_clone(input: TokenStream) -> TokenStream {
    let expr: syn::Expr = syn::parse(input).unwrap();
    let name = get_name(&expr).expect("Expected a variable or field access");
    let output = quote! {
        let #name = #expr.clone();
    };
    output.into()
}

fn get_name(expr: &Expr) -> Option<Ident> {
    match expr {
        syn::Expr::Path(expr_path) => {
            Some(expr_path.path.segments.last().unwrap().ident.clone())
        }
        syn::Expr::Field(
            syn::ExprField {
                member: syn::Member::Named(ident),
                ..
            }
        ) => {
            Some(ident.clone())
        }
        syn::Expr::MethodCall(
            syn::ExprMethodCall {
                method,
                ..
            }
        ) => {
            Some(method.clone())
        }
        syn::Expr::Group(
            syn::ExprGroup {
                expr,
                ..
            }
        ) => get_name(expr),
        _ => None
    }
}