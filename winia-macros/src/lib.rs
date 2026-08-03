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

/// FNV-1a 字符串哈希（函数名 → scope 源码哈希；语句 id 的 key 基）
fn fnv64(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// 语句 id 注入：每条语句包裹 `ctx.push_stmt(id) / ctx.pop_stmt()`——
/// id 为编译期按源码结构固定的序号（结构变化不漂移——对标 Compose 调用点 key）。
/// 递归进入控制流体（if/for/while/block/match 的语句块）继续注入；闭包体跳过
/// （content 执行时机不定，由外层调用点管理）。
fn inject_stmt_ids(stmts: Vec<Stmt>, ctx: &syn::Ident, counter: &mut u32) -> Vec<Stmt> {
    let mut out = Vec::new();
    for stmt in stmts {
        let id = *counter;
        *counter += 1;
        let stmt = inject_nested(stmt, ctx, counter);
        let push: Stmt = syn::parse_quote!(#ctx.push_stmt(#id););
        let pop: Stmt = syn::parse_quote!(#ctx.pop_stmt(););
        out.push(push);
        out.push(stmt);
        out.push(pop);
    }
    out
}

/// 递归注入嵌套语句块（控制流体），重建 AST
fn inject_nested(stmt: Stmt, ctx: &syn::Ident, counter: &mut u32) -> Stmt {
    match stmt {
        Stmt::Expr(expr, semi) => {
            let e = inject_expr_blocks(expr, ctx, counter);
            Stmt::Expr(e, semi)
        }
        other => other, // let/item 不深入（内嵌闭包/块由语句级 push 覆盖）
    }
}

fn inject_block(block: syn::Block, ctx: &syn::Ident, counter: &mut u32) -> syn::Block {
    let stmts = inject_stmt_ids(block.stmts, ctx, counter);
    syn::Block {
        brace_token: block.brace_token,
        stmts,
    }
}

fn inject_expr_blocks(expr: syn::Expr, ctx: &syn::Ident, counter: &mut u32) -> syn::Expr {
    match expr {
        syn::Expr::If(mut e) => {
            e.then_branch = inject_block(e.then_branch, ctx, counter);
            if let Some((_, else_expr)) = e.else_branch.take() {
                let else_expr = inject_expr_blocks(*else_expr, ctx, counter);
                e.else_branch = Some((syn::token::Else::default(), Box::new(else_expr)));
            }
            syn::Expr::If(e)
        }
        syn::Expr::ForLoop(mut e) => {
            e.body = inject_block(e.body, ctx, counter);
            syn::Expr::ForLoop(e)
        }
        syn::Expr::While(mut e) => {
            e.body = inject_block(e.body, ctx, counter);
            syn::Expr::While(e)
        }
        syn::Expr::Block(mut e) => {
            e.block = inject_block(e.block, ctx, counter);
            syn::Expr::Block(e)
        }
        syn::Expr::Match(mut e) => {
            for arm in e.arms.iter_mut() {
                let body = std::mem::replace(&mut arm.body, Box::new(syn::parse_quote!(())));
                if let syn::Expr::Block(b) = *body {
                    let injected = inject_block(b.block, ctx, counter);
                    arm.body = Box::new(syn::Expr::Block(syn::ExprBlock {
                        attrs: b.attrs,
                        label: b.label,
                        block: injected,
                    }));
                } else {
                    arm.body = body;
                }
            }
            syn::Expr::Match(e)
        }
        // 函数调用：遍历参数（content 闭包通常在 build(...) 的参数位——闭包识别注入）
        syn::Expr::Call(mut e) => {
            for arg in e.args.iter_mut() {
                let a = std::mem::replace(arg, syn::parse_quote!(0));
                *arg = inject_expr_blocks(a, ctx, counter);
            }
            syn::Expr::Call(e)
        }
        // 方法调用（build(ctx, |ctx| {...}) 的常见形态）：同样遍历参数
        syn::Expr::MethodCall(mut e) => {
            for arg in e.args.iter_mut() {
                let a = std::mem::replace(arg, syn::parse_quote!(0));
                *arg = inject_expr_blocks(a, ctx, counter);
            }
            syn::Expr::MethodCall(e)
        }
        // 闭包：单参数且名为 `ctx` → content 闭包（约定——组件 build 的内容闭包），
        // 注入其体（闭包内节点获得语句级源码位置 key——结构变化不漂移）。
        // 其他闭包（map 回调等）不注入（参数名非 ctx——执行时机不定）。
        syn::Expr::Closure(mut e) => {
            let is_content = match e.inputs.first() {
                Some(syn::Pat::Ident(pi)) if e.inputs.len() == 1 && pi.ident == "ctx" => true,
                _ => false,
            };
            if is_content {
                let body = std::mem::replace(&mut e.body, Box::new(syn::parse_quote!(())));
                let injected = inject_expr_blocks(*body, ctx, counter);
                e.body = Box::new(injected);
            }
            syn::Expr::Closure(e)
        }
        _ => expr, // 其他表达式不深入
    }
}

/// 将函数变换为组合 scope：开头 `ctx.start_scope_keyed(源码哈希)`（函数级 key 稳定），
/// 每条语句包裹 push_stmt/pop_stmt（语句级 key 稳定），结尾 `ctx.end_scope()`。
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

    // 函数名哈希（scope 源码 key——函数级稳定）
    let scope_hash = fnv64(&sig.ident.to_string());
    // 开头注入 start_scope_keyed（函数级 key 稳定）
    let start = quote! { let __composable_scope = #ctx_ident.start_scope_keyed(#scope_hash); };
    // 结尾注入 end_scope（在所有语句之后——scope 覆盖整个函数体）
    let end = quote! { #ctx_ident.end_scope(); };

    let mut body_stmts = stmts;
    if let Some(Stmt::Expr(expr, _)) = tail {
        // 尾表达式补分号转普通语句（组合函数返回 ()，丢弃尾值合法）
        body_stmts.push(Stmt::Expr(expr, Some(Default::default())));
    } else if let Some(o) = tail {
        body_stmts.push(o); // let/item 原样放回
    }
    // 语句 id 注入（每条语句 push_stmt/pop_stmt——语句级 key 稳定）
    let mut stmt_counter: u32 = 0;
    let injected = inject_stmt_ids(body_stmts, &ctx_ident, &mut stmt_counter);

    let mut new_stmts = vec![syn::parse2::<Stmt>(start).unwrap()];
    new_stmts.extend(injected);
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
