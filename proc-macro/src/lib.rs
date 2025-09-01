use inflector::Inflector;
use proc_macro::TokenStream;
use std::fmt::format;
use proc_macro2::{Ident, Span};
use quote::{quote, ToTokens};
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{parse_macro_input, AngleBracketedGenericArguments, DeriveInput, Expr, ExprMethodCall, Field, FieldMutability, Fields, GenericArgument, GenericParam, Generics, ItemStruct, Lifetime, Path, PathArguments, PathSegment, Stmt, Type, TypePath, TypeReference, Visibility};

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

/*
    pub struct Container {
        pub shape: Shape,
        pub height: Value<f32>,
        pub elevation: Value<f32>,
        pub shadow_color: Value<Color>,
        pub color: Value<Color>,
        pub opacity: Value<f32>,
    }

    impl Style for Container {
        fn from_theme(theme: &Theme, prefix: impl Into<String>) -> Self {
            let prefix = prefix.into();
            Self {
                shape: Shape::from_theme(theme, "_shape" + prefix.to_owned()),
                height: Value::Value(theme.get_dimension(prefix.clone() + "_height").unwrap()),
                elevation: Value::Value(theme.get_dimension(prefix.clone() + "_elevation").unwrap()),
                shadow_color: Value::Value(theme.get_color(prefix.clone() + "_shadow_color").unwrap()),
                color: Value::Value(theme.get_color(prefix.clone() + "_color").unwrap()),
                opacity: Value::Value(theme.get_dimension(prefix.clone() + "_opacity").unwrap()),
            }
        }

        fn apply(&self, theme: &mut Theme, prefix: impl Into<String>) {
            let prefix = prefix.into();
            self.shape.apply(theme, prefix.clone() + "_shape");
            theme.set_dimension(prefix.clone() + "_height", self.height.clone());
            theme.set_dimension(prefix.clone() + "_elevation", self.elevation.clone());
            theme.set_color(prefix.clone() + "_shadow_color", self.shadow_color.clone());
            theme.set_color(prefix.clone() + "_color", self.color.clone());
            theme.set_dimension(prefix.clone() + "_opacity", self.opacity.clone());
        }
    }

        impl Container {
        pub fn new(
            shape: Shape,
            height: impl Into<Value<f32>>,
            elevation: impl Into<Value<f32>>,
            shadow_color: impl Into<Value<Color>>,
            color: impl Into<Value<Color>>,
            opacity: impl Into<Value<f32>>,
        ) -> Self {
            Self {
                shape,
                height: height.into(),
                elevation: elevation.into(),
                shadow_color: shadow_color.into(),
                color: color.into(),
                opacity: opacity.into(),
            }
        }

        pub fn shape(&self) -> &Shape {
            &self.shape
        }

        pub fn shape_mut(&mut self) -> &mut Shape {
            &mut self.shape
        }

        pub fn set_shape(&mut self, shape: Shape) -> &mut Self {
            self.shape = shape;
            self
        }

        pub fn height(&self) -> &f32 {
            match &self.height {
                Value::Value(value) => value,
                Value::Ref(key) => panic!("Value is a reference: {}", key),
            }
        }

        pub fn set_height(&mut self, height: impl Into<Value<f32>>) -> &mut Self {
            self.height = height.into();
            self
        }

        pub fn elevation(&self) -> &f32 {
            match &self.elevation {
                Value::Value(value) => value,
                Value::Ref(key) => panic!("Value is a reference: {}", key),
            }
        }

        pub fn set_elevation(&mut self, elevation: impl Into<Value<f32>>) -> &mut Self {
            self.elevation = elevation.into();
            self
        }

        pub fn shadow_color(&self) -> &Color {
            match &self.shadow_color {
                Value::Value(value) => value,
                Value::Ref(key) => panic!("Value is a reference: {}", key),
            }
        }

        pub fn set_shadow_color(&mut self, shadow_color: impl Into<Value<Color>>) -> &mut Self {
            self.shadow_color = shadow_color.into();
            self
        }

        pub fn color(&self) -> &Color {
            match &self.color {
                Value::Value(value) => value,
                Value::Ref(key) => panic!("Value is a reference: {}", key),
            }
        }

        pub fn set_color(&mut self, color: impl Into<Value<Color>>) -> &mut Self {
            self.color = color.into();
            self
        }

        pub fn opacity(&self) -> &f32 {
            match &self.opacity {
                Value::Value(value) => value,
                Value::Ref(key) => panic!("Value is a reference: {}", key),
            }
        }

        pub fn set_opacity(&mut self, opacity: impl Into<Value<f32>>) -> &mut Self {
            self.opacity = opacity.into();
            self
        }
    }
*/

/*#[proc_macro_attribute]
pub fn style(_attr: TokenStream, input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as ItemStruct);
    let struct_name = input.ident.clone();
    let fields = match &input.fields {
        syn::Fields::Named(fields) => &fields.named,
        _ => panic!("Only support named fields"),
    };

    let field_names: Vec<_> = fields
        .iter()
        .map(|field| field.ident.clone().unwrap())
        .collect();
    let field_types: Vec<_> = fields.iter().map(|field| field.ty.clone()).collect();

    let field_names_and_types: Vec<_> = field_names
        .iter()
        .zip(field_types.iter())
        .collect();

    // (name, type)
    let mut style_names: Vec<Ident> = Vec::new();
    let mut style_types: Vec<Ident> = Vec::new();
    let mut unit_names: Vec<Ident> = Vec::new();
    let mut unit_types: Vec<Ident> = Vec::new();

    for (name, type_) in field_names_and_types.into_iter() {
        match type_ {
            Type::Path(TypePath { path, .. }) => {
                let last = path.segments.last().unwrap();
                match &last.arguments {
                    PathArguments::AngleBracketed(arguments) => {
                        let args = &arguments.args;
                        match args.first().unwrap() {
                            GenericArgument::Type(ty) => {
                                let ty = ty.clone();
                                match ty {
                                    Type::Path(path) => {
                                        let path = &path.path;
                                        let last = path.segments.last().unwrap();
                                        let ident = &last.ident;
                                        match ident.to_string().as_str() {
                                            "f32" => {
                                                unit_names.push(name.clone());
                                                unit_types.push(Ident::new(
                                                    ident.to_string().as_str(),
                                                    Span::call_site(),
                                                ));
                                            }
                                            "Color" => {
                                                unit_names.push(name.clone());
                                                unit_types.push(Ident::new(
                                                    ident.to_string().as_str(),
                                                    Span::call_site(),
                                                ));
                                            }
                                            "bool" =>{
                                                unit_names.push(name.clone());
                                                unit_types.push(Ident::new(
                                                    ident.to_string().as_str(),
                                                    Span::call_site(),
                                                ));
                                            }
                                            _ => panic!("Only support f32, Color, bool"),
                                        }
                                    }
                                    _ => panic!("Only support path type"),
                                }
                            }
                            _ => panic!("Only support generic arguments"),
                        }
                    }
                    _ => {
                        style_names.push(name.clone());
                        style_types.push(Ident::new(
                            last.ident.to_string().as_str(),
                            Span::call_site(),
                        ));
                    }
                }
            }
            _ => panic!("Only support path type"),
        }
    }

    let style_mut: Vec<_> = style_names
        .iter()
        .map(|name| Ident::new(&format!("{}_mut", name), Span::call_site()))
        .collect();
    let set_style: Vec<_> = style_names
        .iter()
        .map(|name| Ident::new(&format!("set_{}", name), Span::call_site()))
        .collect();

    let set_unit: Vec<_> = unit_names
        .iter()
        .map(|name| Ident::new(&format!("set_{}", name), Span::call_site()))
        .collect();

    let style_suffixes: Vec<_> = style_names
        .iter()
        .map(|name| proc_macro2::Literal::string(&("_".to_owned() + &name.to_string())))
        .collect();
    let unit_suffixes: Vec<_> = unit_names
        .iter()
        .map(|name| proc_macro2::Literal::string(&("_".to_owned() + &name.to_string())))
        .collect();

    let unit_get: Vec<_> = unit_types
        .iter()
        .map(|ty| {
            let name = ty.to_string();
            match name.as_str() {
                "f32" => Ident::new("get_dimension", Span::call_site()),
                "Color" => Ident::new("get_color", Span::call_site()),
                "bool" => Ident::new("get_bool", Span::call_site()),
                _ => panic!("Only support f32, Color, bool"),
            }
        })
        .collect();

    let unit_set: Vec<_> = unit_types
        .iter()
        .map(|ty| {
            let name = ty.to_string();
            match name.as_str() {
                "f32" => Ident::new("set_dimension", Span::call_site()),
                "Color" => Ident::new("set_color", Span::call_site()),
                "bool" => Ident::new("set_bool", Span::call_site()),
                _ => panic!("Only support f32, Color, bool"),
            }
        })
        .collect();


    let output = quote! {
        #[derive(Clone)]
        #input
        impl Style for #struct_name {
            fn from_theme(theme: &Theme, prefix: impl Into<String>) -> Self {
                let prefix = prefix.into();
                Self {
                    #(
                        #style_names: #style_types::from_theme(theme, (prefix.to_owned() + #style_suffixes)),
                    )*
                    #(
                        #unit_names: Value::Direct(theme.#unit_get(prefix.to_owned() + #unit_suffixes).unwrap()),
                    )*
                }
            }

            fn apply(&self, theme: &mut Theme, prefix: impl Into<String>) {
                let prefix = prefix.into();
                #(
                    self.#style_names.apply(theme, prefix.to_owned() + #style_suffixes);
                )*
                #(
                    theme.#unit_set(prefix.to_owned() + #unit_suffixes, self.#unit_names.clone());
                )*
            }
        }

        impl #struct_name {
            pub fn new(
                #(
                    #style_names: #style_types,
                )*
                #(
                    #unit_names: impl Into<Value<#unit_types>>,
                )*
            ) -> Self {
                Self {
                    #(
                        #style_names: #style_names,
                    )*
                    #(
                        #unit_names: #unit_names.into(),
                    )*
                }
            }

            #(
                pub fn #style_names(&self) -> &#style_types {
                    &self.#style_names
                }
            )*

            #(
                pub fn #style_mut(&mut self) -> &mut #style_types {
                    &mut self.#style_names
                }
            )*

            #(
                pub fn #set_style(&mut self, #style_names: #style_types) -> &mut Self {
                    self.#style_names = #style_names;
                    self
                }
            )*

            #(
                pub fn #unit_names(&self) -> &#unit_types {
                    match &self.#unit_names {
                        Value::Direct(value) => value,
                        Value::Reference(key) => panic!("Value is a reference: {}", key),
                    }
                }
            )*

            #(
                pub fn #set_unit(&mut self, #unit_names: impl Into<Value<#unit_types>>) -> &mut Self {
                    self.#unit_names = #unit_names.into();
                    self
                }
            )*
        }
    };
    output.into()
}*/

/*
struct DividerProperty {
    id: usize,
    observers: Arc<Mutex<Vec<(usize, Box<dyn FnMut() + Send>)>>>,
    thickness: SharedF32,
    color: SharedColor,
}

impl DividerProperty {
    pub fn new(thickness: impl Into<SharedF32>, color: impl Into<SharedColor>) -> Self {
        let thickness = thickness.into();
        let color = color.into();
        let id = generate_id();

        let mut self_ = Self {
            id,
            simple_observers: Arc::new(Mutex::new(Vec::new())),
            thickness,
            color,
        };
        self_.thickness.add_observer(id, {
            let property = self_.clone();
            Box::new(move || {
                property.notify();
            })
        });
        self_.color.add_observer(id, {
            let property = self_.clone();
            Box::new(move || {
                property.notify();
            })
        });

        self_
    }

    pub fn notify(&self) {
        for (_, observer) in self.observers.lock().iter_mut() {
            observer();
        }
    }

    pub fn set_thickness(&self, thickness: impl Into<SharedF32>) {
        self.thickness.remove_observer(self.id);
        let mut thickness = thickness.into();
        thickness.add_observer(self.id, {
            let property = self.clone();
            Box::new(move || {
                property.notify();
            })
        });
        self.notify();
    }

    pub fn get_thickness(&self) -> SharedF32 {
        self.thickness.clone()
    }
}
impl Observable for DividerProperty {
    fn add_observer(&mut self, id: usize, observer: Box<dyn FnMut() + Send>) -> Removal {
        self.simple_observers.lock().push((id, observer));
        let simple_observers = self.simple_observers.clone();
        Removal::new(move || {
            simple_observers.lock().retain(|(i, _)| *i != id);
        })
    }
}
*/

#[proc_macro_attribute]
pub fn observable(_attr: TokenStream, input: TokenStream) -> TokenStream {
    let mut input = parse_macro_input!(input as ItemStruct);
    let struct_name = input.ident.clone();
    let fields = match &input.fields {
        syn::Fields::Named(fields) => &fields.named,
        _ => panic!("Only support named fields"),
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

    match &mut input.fields {
        Fields::Named(fields) => {
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
        _ => {}
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

/*#[proc_macro_attribute]
pub fn style(_attr: TokenStream, input: TokenStream) -> TokenStream {
    let mut input = parse_macro_input!(input as ItemStruct);

    input.generics = syn::parse_str::<Generics>("<\'t>").unwrap();

    let fields = match &mut input.fields {
        Fields::Named(fields) => &mut fields.named,
        _ => panic!("Only support named fields"),
    };

    let struct_name = input.ident.clone();

    let (field_names, field_types, field_types_with_generic) = {
        let mut field_names = Vec::new();
        let mut field_types = Vec::new();
        let mut field_types_with_generic = Vec::new();
        while let Some(mut field) = fields.pop() {
            let field = field.value_mut();
            let field_name = field.ident.clone().unwrap();
            field_names.push(field_name);
            match &mut field.ty {
                Type::Path(type_path) => {
                    let last = type_path.path.segments.last_mut().unwrap();
                    let type_name = last.ident.to_string();
                    match type_name.as_str() {
                        "f32" | "Color" | "bool" => {
                            let type_path = TypePath {
                                qself: None,
                                path: Path {
                                    leading_colon: None,
                                    segments: {
                                        let mut segments = Punctuated::new();
                                        segments.push(PathSegment {
                                            ident: Ident::new("StyleProperty", Span::call_site()),
                                            arguments: PathArguments::AngleBracketed(
                                                AngleBracketedGenericArguments {
                                                    colon2_token: None,
                                                    lt_token: Default::default(),
                                                    args: {
                                                        let mut args = Punctuated::new();
                                                        args.push(GenericArgument::Lifetime(
                                                            syn::parse_str::<Lifetime>("\'_")
                                                                .unwrap(),
                                                        ));
                                                        args.push(GenericArgument::Type(
                                                            Type::Path(type_path.clone()),
                                                        ));
                                                        args
                                                    },
                                                    gt_token: Default::default(),
                                                },
                                            ),
                                        });
                                        segments
                                    },
                                },
                            };
                            field_types.push(Type::Path(TypePath {
                                qself: None,
                                path: Path {
                                    leading_colon: None,
                                    segments: {
                                        let mut segments = Punctuated::new();
                                        segments.push(PathSegment {
                                            ident: Ident::new("StyleProperty", Span::call_site()),
                                            arguments: PathArguments::None,
                                        });
                                        segments
                                    },
                                },
                            }));
                            field_types_with_generic.push(Type::Path(type_path));
                        }
                        _ => {
                            field_types.push(Type::Path(type_path.clone()));
                            let mut type_path = type_path.clone();
                            let last = type_path.path.segments.last_mut().unwrap();
                            last.arguments =
                                PathArguments::AngleBracketed(AngleBracketedGenericArguments {
                                    colon2_token: None,
                                    lt_token: Default::default(),
                                    args: {
                                        let mut args = Punctuated::new();
                                        args.push(GenericArgument::Lifetime(
                                            syn::parse_str::<Lifetime>("\'_").unwrap(),
                                        ));
                                        args
                                    },
                                    gt_token: Default::default(),
                                });
                            field_types_with_generic.push(Type::Path(type_path))
                        }
                    }
                }
                _ => panic!("Only support path type"),
            }
        }
        (field_names, field_types, field_types_with_generic)
    };

    fields.push(Field {
        attrs: vec![],
        vis: Visibility::Inherited,
        mutability: FieldMutability::None,
        ident: Some(Ident::new("prefix", Span::call_site())),
        colon_token: None,
        ty: Type::Path(TypePath {
            qself: None,
            path: Path {
                leading_colon: None,
                segments: {
                    let mut segments = Punctuated::new();
                    segments.push(PathSegment {
                        ident: Ident::new("String", Span::call_site()),
                        arguments: PathArguments::None,
                    });
                    segments
                },
            },
        }),
    });

    fields.push(Field {
        attrs: vec![],
        vis: Visibility::Inherited,
        mutability: FieldMutability::None,
        ident: Some(Ident::new("theme", Span::call_site())),
        colon_token: None,
        ty: Type::Reference(TypeReference {
            and_token: Default::default(),
            lifetime: Some(syn::parse_str::<Lifetime>("\'t").unwrap()),
            mutability: Some(Default::default()),
            elem: Box::new(Type::Path(TypePath {
                qself: None,
                path: Path {
                    leading_colon: None,
                    segments: {
                        let mut segments = Punctuated::new();
                        segments.push(PathSegment {
                            ident: Ident::new("Theme", Span::call_site()),
                            arguments: PathArguments::None,
                        });
                        segments
                    },
                },
            })),
        }),
    });

    // let field_types = fields.iter().map(|field| {
    //     let mut field_type = field.ty.clone();
    //     if let Type::Path(type_path) = &mut field_type {
    //         let last = type_path.path.segments.last_mut().unwrap();
    //         last.arguments = PathArguments::None;
    //     }
    //     field_type
    // }).collect::<Vec<_>>();

    let _field_names = field_names
        .iter()
        .map(|name| proc_macro2::Literal::string(&format!("_{}", name)))
        .collect::<Vec<_>>();

    let output = quote! {
        #input
        impl<'t> #struct_name<'t> {
            pub fn new(
                theme: &'t mut Theme,
                prefix: impl Into<String>
            ) -> Self {
                Self {
                    prefix: prefix.into(),
                    theme,
                }
            }

            pub fn key(&self) -> String {
                self.prefix.clone()
            }

            #(
                pub fn #field_names(&mut self) -> #field_types_with_generic {
                    #field_types::new(self.theme, self.prefix.clone() + #_field_names)
                }
            )*
        }
    };

    output.into()
}*/

#[proc_macro_attribute]
pub fn style(_attr: TokenStream, input: TokenStream) -> TokenStream {
    let mut input = parse_macro_input!(input as ItemStruct);


    let fields = match &mut input.fields {
        Fields::Named(fields) => &mut fields.named,
        _ => panic!("Only support named fields"),
    };
    // name, type, set_xxx, get_xxx, get_xxx (for theme)
    let (field_names, field_types, new_types, get_field_names, get_theme_field_names) = {
        let mut field_names = Vec::new();
        let mut field_types = Vec::new();
        let mut new_types = Vec::new();
        let mut get_field_names = Vec::new();
        let mut get_theme_field_names = Vec::new();
        for field in fields.iter() {
            let field_name = field.ident.as_ref().unwrap().to_string();
            let field_type = field.ty.to_token_stream().to_string();
            let new_type = format!("State<{}>", field_type);
            let get_field_name = format!("get_{}", field_name);
            //Some(theme.get_style(name)?.downcast_ref::<f32>()?)
            let get_theme_field_name = match field_type.as_str() {
                "f32" => "theme.get_dimension(name)".to_string(),
                "Color" => "theme.get_color(name)".to_string(),
                "bool" => "theme.get_bool(name)".to_string(),
                _=> "theme.get_style(name)".to_string(),
            };
            field_names.push(field.ident.clone().unwrap());
            field_types.push(field.ty.clone());
            new_types.push(syn::parse_str::<Type>(&new_type).unwrap());
            get_field_names.push(Ident::new(&get_field_name, Span::call_site()));
            get_theme_field_names.push(
                syn::parse_str::<Expr>(&get_theme_field_name).unwrap()
            );
        }
        (
            field_names,
            field_types,
            new_types,
            get_field_names,
            get_theme_field_names,
        )
    };

    let struct_name = input.ident.clone();
/*
        pub fn get_thickness<'a>(&'a self, theme: &'a Theme, item_state: ItemState) -> Option<&'a f32> {
            match item_state {
                ItemState::Enabled => match self.thickness.get_enabled() {
                    ThemeValue::Ref(name) => theme.get_dimension(name),
                    ThemeValue::Direct(value) => Some(value),
                }
                ItemState::Disabled => match self.thickness.get_disabled() {
                    ThemeValue::Ref(name) => theme.get_dimension(name),
                    ThemeValue::Direct(value) => Some(value),
                },
                ItemState::Focused => match self.thickness.get_enabled() {
                    ThemeValue::Ref(name) => theme.get_dimension(name),
                    ThemeValue::Direct(value) => Some(value),
                },
                ItemState::Hovered => match self.thickness.get_enabled() {
                    ThemeValue::Ref(name) => theme.get_dimension(name),
                    ThemeValue::Direct(value) => Some(value),
                },
                ItemState::Pressed => match self.thickness.get_enabled() {
                    ThemeValue::Ref(name) => Some(theme.get_style(name)?.downcast_ref::<f32>()?),
                    ThemeValue::Direct(value) => Some(value),
                },
            }
        }*/
    let output = quote! {
        pub struct #struct_name {
            #(
                pub #field_names: #new_types,
            )*
        }
        
        impl #struct_name {
            #(
                pub fn #get_field_names<'a>(&'a self, theme: &'a Theme, item_state: ItemState) -> Option<&'a #field_types> {
                    match item_state {
                        ItemState::Enabled => match self.#field_names.get_enabled() {
                            ThemeValue::Ref(name) => #get_theme_field_names,
                            ThemeValue::Direct(value) => Some(value),
                        },
                        ItemState::Disabled => match self.#field_names.get_disabled() {
                            ThemeValue::Ref(name) => #get_theme_field_names,
                            ThemeValue::Direct(value) => Some(value),
                        },
                        ItemState::Focused => match self.#field_names.get_focused() {
                            ThemeValue::Ref(name) => #get_theme_field_names,
                            ThemeValue::Direct(value) => Some(value),
                        },
                        ItemState::Hovered => match self.#field_names.get_hovered() {
                            ThemeValue::Ref(name) => #get_theme_field_names,
                            ThemeValue::Direct(value) => Some(value),
                        },
                        ItemState::Pressed => match self.#field_names.get_pressed() {
                            ThemeValue::Ref(name) => #get_theme_field_names,
                            ThemeValue::Direct(value) => Some(value),
                        },
                    }
                }
            )*
        }
    };
    println!("Output: {}", output);
    output.into()
}

/*
impl ButtonStyle {
    pub fn set_container_color(&mut self, item_state: ItemState, color: impl Into<StyleValue<Color>>) -> &mut Self {
        match item_state {
            ItemState::Enabled => self.container_color.enabled = Some(color.into()),
            ItemState::Disabled => self.container_color.disabled = Some(color.into()),
            ItemState::Focused => self.container_color.focused = Some(color.into()),
            ItemState::Hovered => self.container_color.hovered = Some(color.into()),
            ItemState::Pressed => self.container_color.pressed = Some(color.into()),
        }
        self
    }

    pub fn get_container_color(&self, theme: &Theme, item_state: ItemState) -> Option<&Color> {
        match item_state {
            ItemState::Enabled => match &self.container_color.enabled? {
                StyleValue::Parent => {
                    let parent_style = theme
                        .get_style(self.parent.as_ref()?)?
                        .downcast_ref::<ButtonStyle>()?;
                    parent_style.get_container_color(theme, item_state)
                }
                StyleValue::Ref(name) => theme.get_color(name),
                StyleValue::Direct(value) => Some(value),
            },
            ItemState::Disabled => {}
            ItemState::Focused => {}
            ItemState::Hovered => {}
            ItemState::Pressed => {}
        }
    }
}*/