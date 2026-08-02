//! `winia` 组合系统属性宏。
//!
//! ## `#[composable]`
//!
//! 标记一个"组合函数"（第一个参数必须是 `ctx: &mut ComposeCtx`）为**函数级组合 scope**：
//! 函数体开头自动注入 `ctx.start_scope()`、结尾自动注入 `ctx.end_scope()`。
//! 函数体内（组件外）的 `State::get()` 表达式注册到该函数 scope——
//! 依赖的 State 变化 → scope 失效 → **函数整体重跑**（对标 Compose @Composable）。
//!
//! ```rust
//! use winia::ComposeCtx;
//! use winia::core::state::State;
//!
//! #[composable]
//! fn section2(ctx: &mut ComposeCtx, alpha: &State<f32>) {
//!     // 表达式直接写——注册到本函数 scope，alpha 变化 → 本函数重跑
//!     let w = alpha.get() * 200.0 + 50.0;
//!     Column::new().modifier(Modifier::new().size(w, 30.0)).build(ctx, |ctx| { /* ... */ });
//! }
//! ```
//!
//! 限制（v1）：
//! - 函数必须有一个名为 `ctx` 且类型含 `ComposeCtx` 的参数（放在任意位置）
//! - **不支持提前 `return`**（return 前不会自动 end_scope；用末尾表达式）
//! - 不支持 `async fn` / 泛型 / `where` 子句（v1 简化）

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn, ReturnType, Stmt};

/// 将函数变换为组合 scope：开头 `ctx.start_scope()`，结尾 `ctx.end_scope()`。
#[proc_macro_attribute]
pub fn composable(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    let ItemFn {
        attrs,
        vis,
        sig,
        block,
    } = input;

    // 校验：函数须有名为 `ctx` 的参数（拿到其标识符用于注入调用）
    let ctx_ident = sig
        .inputs
        .iter()
        .find_map(|arg| match arg {
            syn::FnArg::Typed(pat_type) => match &*pat_type.pat {
                syn::Pat::Ident(pat_ident) if pat_ident.ident == "ctx" => Some(pat_ident.ident.clone()),
                _ => None,
            },
            _ => None,
        })
        .expect("#[composable] 函数必须有一个名为 `ctx` 的参数（如 `ctx: &mut ComposeCtx`）");

    // 限制：v1 不支持提前 return
    if let ReturnType::Default = sig.output {
        // ok（无返回）
    } else {
        // 有返回类型：仅当返回 ()（impl Trait 等复杂情况不支持）
        // 简化：非 () 返回 → panic（组合函数应为副作用）
        match &sig.output {
            ReturnType::Type(_, ty) => {
                let is_unit = matches!(&**ty, syn::Type::Tuple(t) if t.elems.is_empty());
                if !is_unit {
                    panic!("#[composable] 函数暂不支持返回值（v1：组合函数应为副作用，返回 ()）");
                }
            }
            _ => {}
        }
    }

    // 提取函数体语句：尾表达式（含分号的普通语句）统一转带分号语句，
    // 保证 end_scope 能安全插在所有语句之后（scope 覆盖整个函数体，含原尾表达式）。
    let mut stmts: Vec<Stmt> = block.stmts.clone();
    // pop 最后一条：表达式（Stmt::Expr，可能带分号）或 let/item 语句
    let tail = stmts.pop();

    // 开头注入 start_scope
    let start = quote! { let __composable_scope = #ctx_ident.start_scope(); };
    // 结尾注入 end_scope（在所有语句之后——scope 覆盖整个函数体）
    let end = quote! { #ctx_ident.end_scope(); };

    let mut new_stmts = vec![syn::parse2::<Stmt>(start).unwrap()];
    new_stmts.extend(stmts);
    if let Some(Stmt::Expr(expr, _)) = tail {
        // 尾表达式补分号转普通语句（组合函数返回 ()，丢弃尾值合法）
        new_stmts.push(Stmt::Expr(expr, Some(Default::default())));
    } else if let Some(o) = tail {
        new_stmts.push(o); // let/item 原样放回
    }
    new_stmts.push(syn::parse2::<Stmt>(end).unwrap());

    let new_block = syn::Block {
        brace_token: block.brace_token,
        stmts: new_stmts,
    };

    let output = quote! {
        #(#attrs)*
        #vis #sig #new_block
    };
    TokenStream::from(output)
}
