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
        // 尾表达式（无分号）不包裹 push/pop——包裹会吞掉尾值
        // （`{ 300.0 }` 变 `{ push; 300.0; pop }` → 块值变 ()，if/块表达式类型错）。
        // 尾表达式通常是值表达式（组合函数返回 ()，无 build）——递归仍注入内部块。
        if let Stmt::Expr(_, None) = stmt {
            out.push(inject_nested(stmt, ctx, counter));
            continue;
        }
        let id = *counter;
        *counter += 1;
        let stmt = inject_nested(stmt, ctx, counter);
        match &stmt {
            // let：显式 push/pop（块包会破坏变量作用域；let 语句本身无
            // return/break/continue——init 表达式内的闭包/循环已注入自己的 guard）
            Stmt::Local(_) => {
                let push: Stmt = syn::parse_quote!(#ctx.push_stmt(#id););
                let pop: Stmt = syn::parse_quote!(#ctx.pop_stmt(););
                out.push(push);
                out.push(stmt);
                out.push(pop);
            }
            // 其他语句：RAII guard 块包——语句块退出（含 return/break/continue/
            // panic 提前退出）guard drop 自动 pop_stmt（显式 pop 会因提前退出泄漏
            // stmt 栈 → 后续语句 key 静默错位）
            _ => {
                out.push(syn::parse_quote!({
                    let __stmt_guard = #ctx.enter_stmt(#id);
                    #stmt
                }));
            }
        }
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
        // let 的 init 表达式也递归注入（`let x = if a { build } else {...}` 的
        // 分支内 build 获得语句级 key——与"语句级 key 不漂移"承诺一致）
        Stmt::Local(mut l) => {
            if let Some(init) = l.init.take() {
                let injected = inject_expr_blocks(*init.expr, ctx, counter);
                l.init = Some(syn::LocalInit {
                    eq_token: init.eq_token,
                    expr: Box::new(injected),
                    diverge: init.diverge,
                });
            }
            Stmt::Local(l)
        }
        other => other, // item 不深入（内嵌闭包/块由语句级 push 覆盖）
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
            e.cond = Box::new(inject_expr_blocks(*e.cond, ctx, counter));
            e.then_branch = inject_block(e.then_branch, ctx, counter);
            if let Some((_, else_expr)) = e.else_branch.take() {
                let else_expr = inject_expr_blocks(*else_expr, ctx, counter);
                e.else_branch = Some((syn::token::Else::default(), Box::new(else_expr)));
            }
            syn::Expr::If(e)
        }
        syn::Expr::ForLoop(mut e) => {
            e.expr = Box::new(inject_expr_blocks(*e.expr, ctx, counter)); // iterable
            e.body = inject_block(e.body, ctx, counter);
            syn::Expr::ForLoop(e)
        }
        syn::Expr::While(mut e) => {
            e.cond = Box::new(inject_expr_blocks(*e.cond, ctx, counter));
            e.body = inject_block(e.body, ctx, counter);
            syn::Expr::While(e)
        }
        syn::Expr::Block(mut e) => {
            e.block = inject_block(e.block, ctx, counter);
            syn::Expr::Block(e)
        }
        syn::Expr::Match(mut e) => {
            e.expr = Box::new(inject_expr_blocks(*e.expr, ctx, counter)); // scrutinee
            for arm in e.arms.iter_mut() {
                if let Some((_, guard)) = &mut arm.guard {
                    let g = std::mem::replace(guard, Box::new(syn::parse_quote!(true)));
                    *guard = Box::new(inject_expr_blocks(*g, ctx, counter));
                }
                let body = std::mem::replace(&mut arm.body, Box::new(syn::parse_quote!(())));
                if let syn::Expr::Block(b) = *body {
                    let injected = inject_block(b.block, ctx, counter);
                    arm.body = Box::new(syn::Expr::Block(syn::ExprBlock {
                        attrs: b.attrs,
                        label: b.label,
                        block: injected,
                    }));
                } else {
                    // 非 Block 臂体（表达式臂）也递归
                    arm.body = Box::new(inject_expr_blocks(*body, ctx, counter));
                }
            }
            syn::Expr::Match(e)
        }
        // 函数调用：func + 参数都遍历（content 闭包通常在 build(...) 参数位）
        syn::Expr::Call(mut e) => {
            let f = std::mem::replace(&mut *e.func, syn::parse_quote!(0));
            *e.func = inject_expr_blocks(f, ctx, counter);
            for arg in e.args.iter_mut() {
                let a = std::mem::replace(arg, syn::parse_quote!(0));
                *arg = inject_expr_blocks(a, ctx, counter);
            }
            syn::Expr::Call(e)
        }
        // 方法调用（build(ctx, |ctx| {...})）：receiver + 参数都遍历（链式 receiver 漏注入修复）
        syn::Expr::MethodCall(mut e) => {
            let recv = std::mem::replace(&mut *e.receiver, syn::parse_quote!(0));
            *e.receiver = inject_expr_blocks(recv, ctx, counter);
            for arg in e.args.iter_mut() {
                let a = std::mem::replace(arg, syn::parse_quote!(0));
                *arg = inject_expr_blocks(a, ctx, counter);
            }
            syn::Expr::MethodCall(e)
        }
        // 闭包：单参数且名为 `ctx` → content 闭包（约定），注入其体
        syn::Expr::Closure(mut e) => {
            let is_content = match e.inputs.first() {
                Some(syn::Pat::Ident(pi)) if e.inputs.len() == 1 && pi.ident == "ctx" => true,
                // 带类型标注的写法 `|ctx: &mut ComposeCtx|`（Pat::Type 内层是 Pat::Ident）
                Some(syn::Pat::Type(pt)) if e.inputs.len() == 1 => {
                    matches!(&*pt.pat, syn::Pat::Ident(pi) if pi.ident == "ctx")
                }
                _ => false,
            };
            if is_content {
                // async 闭包排除（延迟执行——注入的 guard 会在错误时机执行）
                if !e.asyncness.is_some() {
                    let body = std::mem::replace(&mut e.body, Box::new(syn::parse_quote!(())));
                    let injected = inject_expr_blocks(*body, ctx, counter);
                    e.body = Box::new(injected);
                }
            }
            syn::Expr::Closure(e)
        }
        // 括号/引用/解引用/一元：子表达式递归
        syn::Expr::Paren(mut e) => {
            *e.expr = inject_expr_blocks(*e.expr, ctx, counter);
            syn::Expr::Paren(e)
        }
        syn::Expr::Reference(mut e) => {
            *e.expr = inject_expr_blocks(*e.expr, ctx, counter);
            syn::Expr::Reference(e)
        }
        syn::Expr::Unary(mut e) => {
            *e.expr = inject_expr_blocks(*e.expr, ctx, counter);
            syn::Expr::Unary(e)
        }
        // 二元/赋值：左右递归
        syn::Expr::Binary(mut e) => {
            *e.left = inject_expr_blocks(*e.left, ctx, counter);
            *e.right = inject_expr_blocks(*e.right, ctx, counter);
            syn::Expr::Binary(e)
        }
        syn::Expr::Assign(mut e) => {
            *e.left = inject_expr_blocks(*e.left, ctx, counter);
            *e.right = inject_expr_blocks(*e.right, ctx, counter);
            syn::Expr::Assign(e)
        }
        // 字面量集合：元素/字段递归
        syn::Expr::Array(mut e) => {
            for el in e.elems.iter_mut() {
                let a = std::mem::replace(el, syn::parse_quote!(0));
                *el = inject_expr_blocks(a, ctx, counter);
            }
            syn::Expr::Array(e)
        }
        syn::Expr::Tuple(mut e) => {
            for el in e.elems.iter_mut() {
                let a = std::mem::replace(el, syn::parse_quote!(0));
                *el = inject_expr_blocks(a, ctx, counter);
            }
            syn::Expr::Tuple(e)
        }
        syn::Expr::Repeat(mut e) => {
            *e.expr = inject_expr_blocks(*e.expr, ctx, counter);
            *e.len = inject_expr_blocks(*e.len, ctx, counter);
            syn::Expr::Repeat(e)
        }
        syn::Expr::Struct(mut e) => {
            for f in e.fields.iter_mut() {
                let fe = std::mem::replace(&mut f.expr, syn::parse_quote!(0));
                f.expr = inject_expr_blocks(fe, ctx, counter);
            }
            syn::Expr::Struct(e)
        }
        // 索引/字段访问：base 递归
        syn::Expr::Index(mut e) => {
            *e.expr = inject_expr_blocks(*e.expr, ctx, counter);
            *e.index = inject_expr_blocks(*e.index, ctx, counter);
            syn::Expr::Index(e)
        }
        syn::Expr::Field(mut e) => {
            *e.base = inject_expr_blocks(*e.base, ctx, counter);
            syn::Expr::Field(e)
        }
        // 循环/块变体：体注入
        syn::Expr::Loop(mut e) => {
            e.body = inject_block(e.body, ctx, counter);
            syn::Expr::Loop(e)
        }
        syn::Expr::Async(mut e) => {
            e.block = inject_block(e.block, ctx, counter);
            syn::Expr::Async(e)
        }
        syn::Expr::Unsafe(mut e) => {
            e.block = inject_block(e.block, ctx, counter);
            syn::Expr::Unsafe(e)
        }
        syn::Expr::TryBlock(mut e) => {
            e.block = inject_block(e.block, ctx, counter);
            syn::Expr::TryBlock(e)
        }
        _ => expr, // 字面量/路径/宏调用等无子或不可见（宏内容不注入——接受）
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

    // 函数级源码哈希：完整签名 token（含参数类型）+ 源码行号——降低跨模块
    // 同名 composable 的 scope key 碰撞（仅函数名哈希在组件库多模块场景易冲突）
    let sig_str = quote!(#sig).to_string();
    // proc_macro2 的 Span 无 line()（stable）——仅用签名 token（含参数类型）
    // 区分：不同模块同签名同名的 composable 仍可能碰撞（罕见——slot 位置兜底）
    let scope_hash = fnv64(&sig_str);
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
