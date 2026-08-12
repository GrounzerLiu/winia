//! `winia` 组合系统属性宏。
//!
//! ## `#[composable]`
//!
//! 标记一个"组合函数"（第一个参数必须是 `ctx: &mut ComposeCtx`）为**函数级组合 scope**：
//! 函数体开头自动注入 `ctx.start_scope()`、结尾自动注入 `ctx.end_scope()`。
//! 函数体内（组件外）的 `State::get()` 表达式注册到该函数 scope——
//! 依赖的 State 变化 → scope 失效 → **函数整体重跑**（对标 Compose @Composable）。
//!
//! ```rust,ignore
//! // 示例片段：winia-macros crate 无法依赖 winia（循环依赖）——仅展示 API 形状
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

/// 组合哈希：fnv(scope_hash, 语句id, 语句内序号)——宏展开时计算（编译期固定），
/// 用于 remember/next_key 的显式 key（新 key 系统：编译期编号，运行时零序号）
fn key_for(scope_hash: u64, stmt_id: u32, idx: u32) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    h ^= scope_hash; h = h.wrapping_mul(0x100000001b3);
    h ^= stmt_id as u64; h = h.wrapping_mul(0x100000001b3);
    h ^= (idx as u64).wrapping_mul(0x9E3779B97F4A7C15);
    h
}

/// 语句是否含 ctx 组合调用（智能注入判定——决定是否注入 enter_stmt）：
/// 递归检查表达式树——方法 receiver 是 ctx_ident（ctx.remember/ctx.next_key/
/// ctx.animate_*）、调用参数含 ctx（foo(ctx)、build(ctx, ...)）、content 闭包
/// （参数名 == ctx_ident）。纯计算语句（不含）不注入——零 guard 零展开。
fn stmt_uses_ctx(stmt: &Stmt, ctx: &syn::Ident) -> bool {
    match stmt {
        Stmt::Local(l) => l.init.as_ref().map(|i| expr_uses_ctx(&i.expr, ctx)).unwrap_or(false),
        Stmt::Expr(e, _) => expr_uses_ctx(e, ctx),
        _ => false,
    }
}

fn expr_uses_ctx(expr: &syn::Expr, ctx: &syn::Ident) -> bool {
    match expr {
        syn::Expr::MethodCall(m) => {
            let recv_is_ctx = matches!(&*m.receiver, syn::Expr::Path(p) if p.path.is_ident(ctx));
            if recv_is_ctx { return true; }
            if expr_uses_ctx(&m.receiver, ctx) { return true; }
            m.args.iter().any(|a| expr_uses_ctx(a, ctx))
        }
        syn::Expr::Call(c) => {
            if expr_uses_ctx(&c.func, ctx) { return true; }
            c.args.iter().any(|a| expr_uses_ctx(a, ctx))
        }
        syn::Expr::Closure(cl) => {
            // content 闭包（参数名 == ctx_ident）→ 其体在语句级注入时单独处理——
            // 此处只标记"含 ctx"（其体随后由注入递归覆盖）
            if cl.inputs.iter().any(|p| closure_pat_is_ctx(p, ctx)) { return true; }
            expr_uses_ctx(&cl.body, ctx)
        }
        syn::Expr::If(e) => {
            expr_uses_ctx(&e.cond, ctx)
                || e.then_branch.stmts.iter().any(|s| stmt_uses_ctx(s, ctx))
                || e.else_branch.as_ref().map(|(_, b)| expr_uses_ctx(b, ctx)).unwrap_or(false)
        }
        syn::Expr::Block(b) => b.block.stmts.iter().any(|s| stmt_uses_ctx(s, ctx)),
        syn::Expr::Match(m) => {
            expr_uses_ctx(&m.expr, ctx)
                || m.arms.iter().any(|arm| match &*arm.body {
                    syn::Expr::Block(b) => b.block.stmts.iter().any(|s| stmt_uses_ctx(s, ctx)),
                    body => expr_uses_ctx(body, ctx),
                })
        }
        syn::Expr::ForLoop(f) => expr_uses_ctx(&f.expr, ctx) || f.body.stmts.iter().any(|s| stmt_uses_ctx(s, ctx)),
        syn::Expr::While(w) => expr_uses_ctx(&w.cond, ctx) || w.body.stmts.iter().any(|s| stmt_uses_ctx(s, ctx)),
        syn::Expr::Paren(p) => expr_uses_ctx(&p.expr, ctx),
        syn::Expr::Reference(r) => expr_uses_ctx(&r.expr, ctx),
        syn::Expr::Unary(u) => expr_uses_ctx(&u.expr, ctx),
        syn::Expr::Path(p) => p.path.is_ident(ctx),
        syn::Expr::Tuple(t) => t.elems.iter().any(|e| expr_uses_ctx(e, ctx)),
        _ => false,
    }
}

fn closure_pat_is_ctx(pat: &syn::Pat, ctx: &syn::Ident) -> bool {
    match pat {
        syn::Pat::Ident(pi) => pi.ident == *ctx,
        syn::Pat::Type(pt) => matches!(&*pt.pat, syn::Pat::Ident(pi) if pi.ident == *ctx),
        _ => false,
    }
}

/// 语句级入口：扫描替换语句内的 `ctx.remember`/`ctx.next_key` 为编译期 key
/// 版本（remember_at_key/next_key_at）——key = fnv(scope, 语句id, 语句内序号)。
/// 不进入 content 闭包体（参数名 == ctx_ident——其体由递归 inject_stmt_ids
/// 用各自语句 id 处理，避免双重替换）。
fn replace_keyed_calls_stmt(
    stmt: &mut Stmt,
    ctx: &syn::Ident,
    scope_hash: u64,
    stmt_id: u32,
    rem_idx: &mut u32,
    key_idx: &mut u32,
) {
    match stmt {
        Stmt::Local(l) => {
            if let Some(init) = &mut l.init {
                replace_keyed_calls(&mut *init.expr, ctx, scope_hash, stmt_id, rem_idx, key_idx);
            }
        }
        Stmt::Expr(e, _) => {
            replace_keyed_calls(e, ctx, scope_hash, stmt_id, rem_idx, key_idx);
        }
        _ => {}
    }
}

/// 表达式递归替换（新 key 系统：remember/next_key 编译期编号）。
/// ⚠ 不进入 content 闭包体（参数名 == ctx_ident）——由语句级处理。
fn replace_keyed_calls(
    expr: &mut syn::Expr,
    ctx: &syn::Ident,
    scope_hash: u64,
    stmt_id: u32,
    rem_idx: &mut u32,
    key_idx: &mut u32,
) {
    match expr {
        syn::Expr::MethodCall(m) => {
            let recv_is_ctx = matches!(&*m.receiver, syn::Expr::Path(p) if p.path.is_ident(ctx));
            if recv_is_ctx && m.method == "remember" {
                // ctx.remember(init) → ctx.remember_at_key(KEY, init)
                let key = key_for(scope_hash, stmt_id, *rem_idx);
                *rem_idx += 1;
                m.method = syn::Ident::new("remember_at_key", m.method.span());
                m.args.insert(0, syn::parse_quote!(#key));
                // 递归其余参数（init 闭包——content 闭包跳过）
                for a in m.args.iter_mut().skip(1) {
                    replace_keyed_calls(a, ctx, scope_hash, stmt_id, rem_idx, key_idx);
                }
                return;
            }
            if recv_is_ctx && m.method == "next_key" && m.args.is_empty() {
                // ctx.next_key() → ctx.next_key_at(KEY)
                let key = key_for(scope_hash, stmt_id, *key_idx);
                *key_idx += 1;
                m.method = syn::Ident::new("next_key_at", m.method.span());
                m.args.insert(0, syn::parse_quote!(#key));
                return;
            }
            replace_keyed_calls(&mut *m.receiver, ctx, scope_hash, stmt_id, rem_idx, key_idx);
            for a in &mut m.args {
                replace_keyed_calls(a, ctx, scope_hash, stmt_id, rem_idx, key_idx);
            }
        }
        syn::Expr::Call(c) => {
            replace_keyed_calls(&mut *c.func, ctx, scope_hash, stmt_id, rem_idx, key_idx);
            for a in &mut c.args {
                replace_keyed_calls(a, ctx, scope_hash, stmt_id, rem_idx, key_idx);
            }
        }
        syn::Expr::Closure(cl) => {
            // content 闭包（参数名 == ctx_ident）——体由语句级处理，跳过
            if cl.inputs.iter().any(|p| closure_pat_is_ctx(p, ctx)) { return; }
            replace_keyed_calls(&mut cl.body, ctx, scope_hash, stmt_id, rem_idx, key_idx);
        }
        syn::Expr::If(e) => {
            replace_keyed_calls(&mut *e.cond, ctx, scope_hash, stmt_id, rem_idx, key_idx);
            for s in &mut e.then_branch.stmts {
                replace_keyed_calls_stmt(s, ctx, scope_hash, stmt_id, rem_idx, key_idx);
            }
            if let Some((_, b)) = &mut e.else_branch {
                if let syn::Expr::Block(eb) = &mut **b {
                    for s in &mut eb.block.stmts {
                        replace_keyed_calls_stmt(s, ctx, scope_hash, stmt_id, rem_idx, key_idx);
                    }
                } else {
                    replace_keyed_calls(b, ctx, scope_hash, stmt_id, rem_idx, key_idx);
                }
            }
        }
        syn::Expr::Block(b) => {
            for s in &mut b.block.stmts {
                replace_keyed_calls_stmt(s, ctx, scope_hash, stmt_id, rem_idx, key_idx);
            }
        }
        syn::Expr::Paren(p) => replace_keyed_calls(&mut *p.expr, ctx, scope_hash, stmt_id, rem_idx, key_idx),
        syn::Expr::Reference(r) => replace_keyed_calls(&mut *r.expr, ctx, scope_hash, stmt_id, rem_idx, key_idx),
        syn::Expr::Unary(u) => replace_keyed_calls(&mut *u.expr, ctx, scope_hash, stmt_id, rem_idx, key_idx),
        syn::Expr::Match(m) => {
            replace_keyed_calls(&mut *m.expr, ctx, scope_hash, stmt_id, rem_idx, key_idx);
            for arm in &mut m.arms {
                match &mut *arm.body {
                    syn::Expr::Block(ab) => {
                        for s in &mut ab.block.stmts {
                            replace_keyed_calls_stmt(s, ctx, scope_hash, stmt_id, rem_idx, key_idx);
                        }
                    }
                    body => replace_keyed_calls(body, ctx, scope_hash, stmt_id, rem_idx, key_idx),
                }
            }
        }
        syn::Expr::ForLoop(f) => {
            replace_keyed_calls(&mut *f.expr, ctx, scope_hash, stmt_id, rem_idx, key_idx);
            for s in &mut f.body.stmts {
                replace_keyed_calls_stmt(s, ctx, scope_hash, stmt_id, rem_idx, key_idx);
            }
        }
        syn::Expr::While(w) => {
            replace_keyed_calls(&mut *w.cond, ctx, scope_hash, stmt_id, rem_idx, key_idx);
            for s in &mut w.body.stmts {
                replace_keyed_calls_stmt(s, ctx, scope_hash, stmt_id, rem_idx, key_idx);
            }
        }
        syn::Expr::Tuple(t) => {
            for e in &mut t.elems {
                replace_keyed_calls(e, ctx, scope_hash, stmt_id, rem_idx, key_idx);
            }
        }
        _ => {}
    }
}

/// 语句 id 注入：每条语句包裹 `ctx.push_stmt(id) / ctx.pop_stmt()`——
/// id 为编译期按源码结构固定的序号（结构变化不漂移——对标 Compose 调用点 key）。
/// 递归进入控制流体（if/for/while/block/match 的语句块）继续注入；闭包体跳过
/// （content 执行时机不定，由外层调用点管理）。
/// 同时扫描替换语句内的 `ctx.remember`/`ctx.next_key` 为编译期 key 版本
/// （remember_at_key/next_key_at——新 key 系统：编译期编号，运行时零序号）。
/// ⚠ 智能注入：只对含 ctx 组合调用的语句注入（stmt_uses_ctx）——纯计算语句
/// 零 guard 零展开（展开体积随实际组合调用数而非语句数）。
fn inject_stmt_ids(stmts: Vec<Stmt>, ctx: &syn::Ident, counter: &mut u32, scope_hash: u64) -> Vec<Stmt> {
    let mut out = Vec::new();
    for stmt in stmts {
        let id = *counter;
        *counter += 1;
        // 先递归注入（if/while/for/match 条件与体、闭包体、let init、表达式嵌套的
        // content 闭包）——再按语句形态决定包裹方式（尾表达式不包 / let init guard
        // 块包 / 其他 guard 块包）。⚠ 此注入点必须对每条语句执行——漏掉会导致
        // 嵌套闭包体 key 退化路径哈希（结构变化漂移复发）
        let mut stmt = inject_nested(stmt, ctx, counter, scope_hash);
        // 智能注入：语句不含 ctx 组合调用 → 原样放回（不注入、不替换）
        if !stmt_uses_ctx(&stmt, ctx) {
            out.push(stmt);
            continue;
        }
        // 扫描替换本语句内的 remember/next_key（语句内序号从 0 起——
        // 闭包体内语句由递归 inject_stmt_ids 用各自语句 id 处理）
        let mut rem_idx = 0u32;
        let mut key_idx = 0u32;
        replace_keyed_calls_stmt(&mut stmt, ctx, scope_hash, id, &mut rem_idx, &mut key_idx);
        match stmt {
            // 尾表达式（无分号）不包裹 push/pop——包裹会吞掉尾值
            // （`{ 300.0 }` 变 `{ push; 300.0; pop }` → 块值变 ()，if/块表达式类型错）。
            // 尾表达式通常是值表达式（组合函数返回 ()，无 build）——递归仍注入内部块。
            Stmt::Expr(e, None) => {
                out.push(Stmt::Expr(e, None));
            }
            // let：init 用 guard 块包（`let x = { guard; <init> };`）——init 表达式内
            // 的提前 return/panic 离开块时 guard drop 自动 pop（显式 push/pop 会泄漏）；
            // 块包不破坏变量作用域（let 在块外，值 = init 块的值）
            Stmt::Local(mut l) => {
                if let Some(init) = l.init.take() {
                    let pat = l.pat;
                    let init_expr = *init.expr;
                    let diverge = init.diverge.map(|(else_tok, else_expr)| {
                        (else_tok, Box::new(inject_expr_blocks(*else_expr, ctx, counter, scope_hash)))
                    });
                    let mut new_l = syn::Local { attrs: l.attrs, let_token: l.let_token, pat, init: None, semi_token: l.semi_token };
                    new_l.init = Some(syn::LocalInit {
                        eq_token: init.eq_token,
                        expr: Box::new(syn::parse_quote!({ let __stmt_guard = #ctx.enter_stmt(#id); #init_expr })),
                        diverge,
                    });
                    out.push(Stmt::Local(new_l));
                } else {
                    // 无 init（`let x;`）——无 build——不注入
                    out.push(Stmt::Local(l));
                }
            }
            // 其他语句：RAII guard 块包——语句块退出（含 return/break/continue/
            // panic 提前退出）guard drop 自动 pop_stmt（显式 pop 会因提前退出泄漏
            // stmt 栈 → 后续语句 key 静默错位）
            stmt => {
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
fn inject_nested(stmt: Stmt, ctx: &syn::Ident, counter: &mut u32, scope_hash: u64) -> Stmt {
    match stmt {
        Stmt::Expr(expr, semi) => {
            let e = inject_expr_blocks(expr, ctx, counter, scope_hash);
            Stmt::Expr(e, semi)
        }
        // let 的 init 表达式也递归注入（`let x = if a { build } else {...}` 的
        // 分支内 build 获得语句级 key——与"语句级 key 不漂移"承诺一致）
        Stmt::Local(mut l) => {
            if let Some(init) = l.init.take() {
                let injected = inject_expr_blocks(*init.expr, ctx, counter, scope_hash);
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

fn inject_block(block: syn::Block, ctx: &syn::Ident, counter: &mut u32, scope_hash: u64) -> syn::Block {
    let stmts = inject_stmt_ids(block.stmts, ctx, counter, scope_hash);
    syn::Block {
        brace_token: block.brace_token,
        stmts,
    }
}

fn inject_expr_blocks(expr: syn::Expr, ctx: &syn::Ident, counter: &mut u32, scope_hash: u64) -> syn::Expr {
    match expr {
        syn::Expr::If(mut e) => {
            e.cond = Box::new(inject_expr_blocks(*e.cond, ctx, counter, scope_hash));
            e.then_branch = inject_block(e.then_branch, ctx, counter, scope_hash);
            if let Some((_, else_expr)) = e.else_branch.take() {
                let else_expr = inject_expr_blocks(*else_expr, ctx, counter, scope_hash);
                e.else_branch = Some((syn::token::Else::default(), Box::new(else_expr)));
            }
            syn::Expr::If(e)
        }
        syn::Expr::ForLoop(mut e) => {
            e.expr = Box::new(inject_expr_blocks(*e.expr, ctx, counter, scope_hash)); // iterable
            e.body = inject_block(e.body, ctx, counter, scope_hash);
            syn::Expr::ForLoop(e)
        }
        syn::Expr::While(mut e) => {
            e.cond = Box::new(inject_expr_blocks(*e.cond, ctx, counter, scope_hash));
            e.body = inject_block(e.body, ctx, counter, scope_hash);
            syn::Expr::While(e)
        }
        syn::Expr::Block(mut e) => {
            e.block = inject_block(e.block, ctx, counter, scope_hash);
            syn::Expr::Block(e)
        }
        syn::Expr::Match(mut e) => {
            e.expr = Box::new(inject_expr_blocks(*e.expr, ctx, counter, scope_hash)); // scrutinee
            for arm in e.arms.iter_mut() {
                if let Some((_, guard)) = &mut arm.guard {
                    let g = std::mem::replace(guard, Box::new(syn::parse_quote!(true)));
                    *guard = Box::new(inject_expr_blocks(*g, ctx, counter, scope_hash));
                }
                let body = std::mem::replace(&mut arm.body, Box::new(syn::parse_quote!(())));
                if let syn::Expr::Block(b) = *body {
                    let injected = inject_block(b.block, ctx, counter, scope_hash);
                    arm.body = Box::new(syn::Expr::Block(syn::ExprBlock {
                        attrs: b.attrs,
                        label: b.label,
                        block: injected,
                    }));
                } else {
                    // 非 Block 臂体（表达式臂）也递归
                    arm.body = Box::new(inject_expr_blocks(*body, ctx, counter, scope_hash));
                }
            }
            syn::Expr::Match(e)
        }
        // 函数调用：func + 参数都遍历（content 闭包通常在 build(...) 参数位）
        syn::Expr::Call(mut e) => {
            let f = std::mem::replace(&mut *e.func, syn::parse_quote!(0));
            *e.func = inject_expr_blocks(f, ctx, counter, scope_hash);
            for arg in e.args.iter_mut() {
                let a = std::mem::replace(arg, syn::parse_quote!(0));
                *arg = inject_expr_blocks(a, ctx, counter, scope_hash);
            }
            syn::Expr::Call(e)
        }
        // 方法调用（build(ctx, |ctx| {...})）：receiver + 参数都遍历（链式 receiver 漏注入修复）
        syn::Expr::MethodCall(mut e) => {
            let recv = std::mem::replace(&mut *e.receiver, syn::parse_quote!(0));
            *e.receiver = inject_expr_blocks(recv, ctx, counter, scope_hash);
            for arg in e.args.iter_mut() {
                let a = std::mem::replace(arg, syn::parse_quote!(0));
                *arg = inject_expr_blocks(a, ctx, counter, scope_hash);
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
                    let injected = inject_expr_blocks(*body, ctx, counter, scope_hash);
                    e.body = Box::new(injected);
                }
            }
            syn::Expr::Closure(e)
        }
        // 括号/引用/解引用/一元：子表达式递归
        syn::Expr::Paren(mut e) => {
            *e.expr = inject_expr_blocks(*e.expr, ctx, counter, scope_hash);
            syn::Expr::Paren(e)
        }
        syn::Expr::Reference(mut e) => {
            *e.expr = inject_expr_blocks(*e.expr, ctx, counter, scope_hash);
            syn::Expr::Reference(e)
        }
        syn::Expr::Unary(mut e) => {
            *e.expr = inject_expr_blocks(*e.expr, ctx, counter, scope_hash);
            syn::Expr::Unary(e)
        }
        // 二元/赋值：左右递归
        syn::Expr::Binary(mut e) => {
            *e.left = inject_expr_blocks(*e.left, ctx, counter, scope_hash);
            *e.right = inject_expr_blocks(*e.right, ctx, counter, scope_hash);
            syn::Expr::Binary(e)
        }
        syn::Expr::Assign(mut e) => {
            *e.left = inject_expr_blocks(*e.left, ctx, counter, scope_hash);
            *e.right = inject_expr_blocks(*e.right, ctx, counter, scope_hash);
            syn::Expr::Assign(e)
        }
        // 字面量集合：元素/字段递归
        syn::Expr::Array(mut e) => {
            for el in e.elems.iter_mut() {
                let a = std::mem::replace(el, syn::parse_quote!(0));
                *el = inject_expr_blocks(a, ctx, counter, scope_hash);
            }
            syn::Expr::Array(e)
        }
        syn::Expr::Tuple(mut e) => {
            for el in e.elems.iter_mut() {
                let a = std::mem::replace(el, syn::parse_quote!(0));
                *el = inject_expr_blocks(a, ctx, counter, scope_hash);
            }
            syn::Expr::Tuple(e)
        }
        syn::Expr::Repeat(mut e) => {
            *e.expr = inject_expr_blocks(*e.expr, ctx, counter, scope_hash);
            *e.len = inject_expr_blocks(*e.len, ctx, counter, scope_hash);
            syn::Expr::Repeat(e)
        }
        syn::Expr::Struct(mut e) => {
            for f in e.fields.iter_mut() {
                let fe = std::mem::replace(&mut f.expr, syn::parse_quote!(0));
                f.expr = inject_expr_blocks(fe, ctx, counter, scope_hash);
            }
            syn::Expr::Struct(e)
        }
        // 索引/字段访问：base 递归
        syn::Expr::Index(mut e) => {
            *e.expr = inject_expr_blocks(*e.expr, ctx, counter, scope_hash);
            *e.index = inject_expr_blocks(*e.index, ctx, counter, scope_hash);
            syn::Expr::Index(e)
        }
        syn::Expr::Field(mut e) => {
            *e.base = inject_expr_blocks(*e.base, ctx, counter, scope_hash);
            syn::Expr::Field(e)
        }
        // 循环/块变体：体注入
        syn::Expr::Loop(mut e) => {
            e.body = inject_block(e.body, ctx, counter, scope_hash);
            syn::Expr::Loop(e)
        }
        // async 块排除（延迟恢复——await 期间事件循环继续，guard 会悬挂在
        // thread_local 栈上串扰其他窗口组合；与 async 闭包排除一致）
        syn::Expr::Async(e) => syn::Expr::Async(e),
        syn::Expr::Unsafe(mut e) => {
            e.block = inject_block(e.block, ctx, counter, scope_hash);
            syn::Expr::Unsafe(e)
        }
        syn::Expr::TryBlock(mut e) => {
            e.block = inject_block(e.block, ctx, counter, scope_hash);
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

    // 提取函数体语句（RAII scope guard 支持返回值——函数体保持原样，
    // 尾表达式自然返回；guard 声明在开头、函数返回时最后 drop）
    let mut stmts: Vec<Stmt> = block.stmts.clone();
    let tail = stmts.pop();

    // 函数级源码哈希：完整签名 token（含参数类型）——降低跨模块
    // 同名 composable 的 scope key 碰撞（仅函数名哈希在组件库多模块场景易冲突）
    let sig_str = quote!(#sig).to_string();
    // proc_macro2 的 Span 无 line()（stable）——仅用签名 token（含参数类型）
    // 区分：不同模块同签名同名的 composable 仍可能碰撞（罕见——slot 位置兜底）
    let scope_hash = fnv64(&sig_str);
    // 开头注入 RAII scope guard（Drop 自动 end_scope——支持返回值/提前 return）
    let start = quote! { let __composable_scope = #ctx_ident.start_scope_guarded(#scope_hash); };

    let mut body_stmts = stmts;
    if let Some(o) = tail {
        body_stmts.push(o); // 尾表达式/let/item 原样放回——返回值自然保留
    }
    // 语句 id 注入（每条语句 push_stmt/pop_stmt——语句级 key 稳定；
    // 智能注入：只注入含 ctx 组合调用的语句；remember/next_key 编译期替换）
    let mut stmt_counter: u32 = 0;
    let injected = inject_stmt_ids(body_stmts, &ctx_ident, &mut stmt_counter, scope_hash);

    let mut new_stmts = vec![syn::parse2::<Stmt>(start).unwrap()];
    new_stmts.extend(injected);
    // 不再注入 end_scope——由 __composable_scope guard drop 自动执行

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

/// 根组合入口宏（闭包版 #[composable]——保留外部捕获）。
///
/// 用法：`winia::app_root!(|ctx| { ... })` 得到注入语句级 key 的根闭包，
/// 传给 `app::run_app`。日常用法请直接使用 `winia::run_app!`（合并入口）。
///
/// 展开：闭包体注入 `start_scope_keyed(固定根哈希)` + 每条语句 `enter_stmt`
/// （语句级稳定 key——结构变化不漂移）+ `end_scope()`——根闭包内所有组件
/// 调用点获得与 #[composable] 相同的稳定 key。
///
/// 与 #[composable] 的区别：作用于闭包（可捕获 main 局部变量，如 tokio runtime），
/// 且只有一个根入口（scope key 固定常量——无需源码哈希）。
#[proc_macro]
pub fn app_root(input: TokenStream) -> TokenStream {
    TokenStream::from(transform_root_closure(input))
}

/// 应用入口宏（合并写法）：`winia::run_app!(|ctx| { ... })`——
/// 等价 `app::run_app(winia::app_root!(|ctx| { ... }))`，根闭包自动获得
/// 语句级稳定 key（结构变化不漂移）。
#[proc_macro]
pub fn run_app(input: TokenStream) -> TokenStream {
    let closure = transform_root_closure(input);
    // 展开为 run_app(注入闭包)——用绝对路径（宏展开处 crate 名为 winia）
    TokenStream::from(quote!(::winia::app::run_app(#closure)))
}

/// 单语句 key 标记（`#[composable_keyed]` 轻量模式下使用）：
/// 展开为 `{ let __stmt_guard = ctx.enter_stmt(位置哈希); <语句> }`——
/// 语句 id 由**调用位置**（line:column 哈希）派生——全局唯一（编译期固定），
/// 无需宏扫描序号。语句块内可直接使用外层 `ctx`。
///
/// ```rust,ignore
/// #[composable_keyed]
/// fn light_ui(ctx: &mut ComposeCtx) {
///     keyed_stmt!({ Text::new("x").build(ctx); });   // 注入 enter_stmt
/// }
/// ```
#[proc_macro]
pub fn keyed_stmt(input: TokenStream) -> TokenStream {
    // proc_macro::Span（非 proc_macro2）stable 提供 line()/column()——
    // 调用位置全局唯一（文件+行+列由编译器保证）
    let span = proc_macro::Span::call_site();
    let loc = format!("{}:{}", span.line(), span.column());
    let id = fnv64(&loc) as u32;
    let stmts: syn::Block = syn::parse_macro_input!(input as syn::Block);
    TokenStream::from(quote!({
        let __stmt_guard = ctx.enter_stmt(#id);
        #stmts
    }))
}

/// 轻量组合函数宏：**不注入语句 id**（区别于 #[composable] 全量注入）——
/// 只注入 RAII scope guard（remember/next_key 有稳定 base）+ 编译期替换
/// remember/next_key（fnv(scope, 扫描序号, 语句内序号)）。
/// 组件调用（build）需用 `keyed_stmt!` 标记获得语句 id——未标记的组件
/// 调用内部 next_key 无稳定源 → 运行期 panic（fail-fast）。
#[proc_macro_attribute]
pub fn composable_keyed(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    let ItemFn { attrs, vis, sig, block } = input;

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
        .expect("#[composable_keyed] 函数必须有一个名为 `ctx` 的参数");

    let sig_str = quote!(#sig).to_string();
    let scope_hash = fnv64(&sig_str);
    let start = quote! { let __composable_scope = #ctx_ident.start_scope_guarded(#scope_hash); };

    // 遍历语句：编译期替换 remember/next_key（扫描序号——函数内语句顺序）
    // 不包裹 enter_stmt（智能注入不做——轻量模式由 keyed_stmt! 显式标记）
    let mut stmt_counter: u32 = 0;
    let stmts = replace_keyed_stmts(block.stmts.clone(), &ctx_ident, scope_hash, &mut stmt_counter);

    let mut new_stmts = vec![syn::parse2::<Stmt>(start).unwrap()];
    new_stmts.extend(stmts);

    let new_block = syn::Block { brace_token: block.brace_token, stmts: new_stmts };
    let output = quote! {
        #(#attrs)*
        #vis #sig #new_block
    };
    TokenStream::from(output)
}

/// 语句级 remember/next_key 替换（composable_keyed 用——不注入 enter_stmt；
/// 扫描序号 stmt_idx 代替语句 id——函数内语句顺序编译期固定）
fn replace_keyed_stmts(
    stmts: Vec<Stmt>,
    ctx: &syn::Ident,
    scope_hash: u64,
    stmt_idx: &mut u32,
) -> Vec<Stmt> {
    let mut out = Vec::new();
    for mut stmt in stmts {
        let mut rem_idx = 0u32;
        let mut key_idx = 0u32;
        let idx = *stmt_idx;
        *stmt_idx += 1;
        replace_keyed_calls_stmt(&mut stmt, ctx, scope_hash, idx, &mut rem_idx, &mut key_idx);
        out.push(stmt);
    }
    out
}

/// 根闭包变换共享逻辑：注入 start_scope_keyed + 语句级 key + end_scope。
fn transform_root_closure(input: proc_macro::TokenStream) -> proc_macro2::TokenStream {
    let closure = syn::parse::<syn::ExprClosure>(input)
        .expect("app_root!/run_app! 需要一个闭包参数（|ctx| { ... }）");
    // 校验：单参且名为 ctx（与 run_app 签名一致）——兼容 `|ctx|` 与 `|ctx: &mut ComposeCtx|`
    let ctx_ident = closure
        .inputs
        .iter()
        .find_map(|arg| match arg {
            syn::Pat::Ident(pat_ident) if pat_ident.ident == "ctx" => Some(pat_ident.ident.clone()),
            // 带类型注解的参数（Pat::Type）——解包内层 ident
            syn::Pat::Type(pt) => match &*pt.pat {
                syn::Pat::Ident(pat_ident) if pat_ident.ident == "ctx" => Some(pat_ident.ident.clone()),
                _ => None,
            },
            _ => None,
        })
        .expect("app_root! 闭包必须有一个名为 `ctx` 的参数（如 |ctx| { ... }）");

    // 闭包体取 Block 的语句（Box<Block> 字段自动解引用）
    let body_stmts = match &*closure.body {
        syn::Expr::Block(b) => b.block.stmts.clone(),
        other => panic!("app_root! 闭包体必须是块表达式（|ctx| {{ ... }}）"),
    };
    let brace_token = match &*closure.body {
        syn::Expr::Block(b) => b.block.brace_token,
        _ => unreachable!(),
    };

    /// 语句级 key 注入（与 #[composable] 相同——递归覆盖 content 闭包）
    ///
    /// 已知限制：scope hash = fnv64(签名 token)（不含模块路径）——跨模块同名同签名
    /// #[composable] 函数在**同一组合位置交替调用**（if 分支 A/B）时 key 确定性碰撞。
    /// 位置隔离（不同调用位置不串位）兜底大部分场景；该交替场景罕见，文档化接受。
    // 根 scope key：固定常量（应用唯一根入口——无跨模块碰撞问题）
    let root_hash = 0xA11C_E0F0_0000_0001u64; // app_root 根入口专用（完整 64 位）
    let mut stmt_counter: u32 = 0;
    let injected = inject_stmt_ids(body_stmts, &ctx_ident, &mut stmt_counter, root_hash);
    let start = quote! { let __app_root_scope = #ctx_ident.start_scope_guarded(#root_hash); };

    let mut new_stmts = vec![syn::parse2::<Stmt>(start).unwrap()];
    new_stmts.extend(injected);

    let new_block = syn::Block {
        brace_token,
        stmts: new_stmts,
    };

    // 重建闭包（保留捕获/属性，body 换注入后的块）
    let mut out_closure = closure;
    out_closure.body = Box::new(syn::Expr::Block(syn::ExprBlock {
        attrs: Vec::new(),
        label: None,
        block: new_block,
    }));
    quote!(#out_closure)
}

#[cfg(test)]
mod inject_tests {
    use super::*;

    fn ctx_ident() -> syn::Ident {
        syn::Ident::new("ctx", proc_macro2::Span::call_site())
    }

    /// 防回归：content 闭包（单参 ctx）必须被注入（嵌套 enter_stmt）——
    /// 若 inject_nested 统一注入点被误删，闭包体 key 退化路径哈希（结构变化漂移复发）
    #[test]
    fn test_content_closure_body_injected() {
        let body: syn::Block = syn::parse_quote!({
            Column::new().build(ctx, |ctx| { Text::new("a").build(ctx); });
        });
        let mut counter = 0;
        let stmts = inject_stmt_ids(body.stmts, &ctx_ident(), &mut counter, 0x1234);
        let out = quote!(#(#stmts)*).to_string();
        let n = out.matches("enter_stmt").count();
        assert!(n >= 2, "顶层 + content 闭包体都应注入 enter_stmt（实际 {n}）——嵌套注入丢失");
    }

    /// 防回归：if 分支内的 build（带分号语句）也应注入（控制流体递归）
    #[test]
    fn test_if_branch_injected() {
        let body: syn::Block = syn::parse_quote!({
            if cond {
                Text::new("a").build(ctx);
            } else {
                Text::new("b").build(ctx);
            };
        });
        let mut counter = 0;
        let stmts = inject_stmt_ids(body.stmts, &ctx_ident(), &mut counter, 0x1234);
        let out = quote!(#(#stmts)*).to_string();
        let n = out.matches("enter_stmt").count();
        assert!(n >= 3, "顶层（带分号非尾）+ if 两分支都应注入（实际 {n}）");
    }

    /// 防回归：let init 内嵌闭包（content）也应注入
    #[test]
    fn test_let_init_closure_injected() {
        let body: syn::Block = syn::parse_quote!({
            let x = Column::new().build(ctx, |ctx| { Text::new("a").build(ctx); });
        });
        let mut counter = 0;
        let stmts = inject_stmt_ids(body.stmts, &ctx_ident(), &mut counter, 0x1234);
        let out = quote!(#(#stmts)*).to_string();
        let n = out.matches("enter_stmt").count();
        assert!(n >= 2, "let init 内 content 闭包应注入（实际 {n}）");
    }

    /// 尾表达式（无分号）不包裹——但内部块仍注入
    #[test]
    fn test_tail_expr_not_wrapped_but_inner_injected() {
        let body: syn::Block = syn::parse_quote!({
            let v = if c { 1 } else { 2 };
            v
        });
        let mut counter = 0;
        let stmts = inject_stmt_ids(body.stmts, &ctx_ident(), &mut counter, 0x1234);
        let out = quote!(#(#stmts)*).to_string();
        // 智能注入：let v = if c {...} 为纯计算（无 ctx）——不注入（零 guard）
        assert!(!out.contains("enter_stmt"), "纯计算语句不应注入（实际含 enter_stmt）");
    }
}

#[cfg(test)]
mod inject_tests_extra {

    /// 从展开输出提取 remember_at_key 的编译期 key（quote 美化带空格——
    /// split "remember_at_key" 后每段以 " (数字" 开头）
    fn extract_keys(out: &str) -> Vec<u64> {
        out.split("remember_at_key")
            .skip(1)
            .filter_map(|seg| {
                let digits: String = seg.chars()
                    .skip_while(|c| !c.is_ascii_digit())
                    .take_while(|c| c.is_ascii_digit())
                    .collect();
                digits.parse().ok()
            })
            .collect()
    }

    use super::*;

    fn ctx_ident() -> syn::Ident {
        syn::Ident::new("ctx", proc_macro2::Span::call_site())
    }

    fn enter_count(body: syn::Block) -> usize {
        let mut counter = 0;
        let stmts = inject_stmt_ids(body.stmts, &ctx_ident(), &mut counter, 0x1234);
        quote!(#(#stmts)*).to_string().matches("enter_stmt").count()
    }

    #[test]
    fn test_match_arm_body_injected() {
        let body: syn::Block = syn::parse_quote!({
            match mode {
                0 => { Text::new("a").build(ctx); },
                _ => { Text::new("b").build(ctx); },
            };
        });
        let n = enter_count(body);
        assert!(n >= 3, "顶层 + 两臂体都应注入（实际 {n}）");
    }

    #[test]
    fn test_for_body_injected() {
        let body: syn::Block = syn::parse_quote!({
            for i in 0..5 {
                Text::new("x").build(ctx);
            };
        });
        let n = enter_count(body);
        assert!(n >= 2, "顶层 for + 循环体都应注入（实际 {n}）");
    }

    #[test]
    fn test_nested_closure_injected() {
        // content 闭包内的 content 闭包——双层注入
        let body: syn::Block = syn::parse_quote!({
            Column::new().build(ctx, |ctx| {
                Row::new().build(ctx, |ctx| { Text::new("x").build(ctx); });
            });
        });
        let n = enter_count(body);
        assert!(n >= 3, "顶层 + 外层闭包 + 内层闭包都应注入（实际 {n}）");
    }

    #[test]
    fn test_let_else_diverge_injected() {
        let body: syn::Block = syn::parse_quote!({
            let Some(x) = opt else { return; };
        });
        let n = enter_count(body);
        // 智能注入：let-else 纯计算（无 ctx）——不注入
        assert_eq!(n, 0, "纯计算 let-else 不应注入（实际 {n}）");
    }

    /// 智能注入：纯计算语句不注入（零 guard）——只注入含 ctx 组合调用的语句
    #[test]
    fn test_smart_inject_skips_pure_calc() {
        let body: syn::Block = syn::parse_quote!({
            let doubled = w * 2.0;
            let label = format!("{:.1}", doubled);
            Text::new("x").build(ctx);
            let a = ctx.remember(|| 0);
        });
        let mut counter = 0;
        let stmts = inject_stmt_ids(body.stmts, &ctx_ident(), &mut counter, 0x1234);
        let out = quote!(#(#stmts)*).to_string();
        assert_eq!(out.matches("enter_stmt").count(), 2,
            "纯计算语句不应注入——只注入含 ctx 的语句（实际 enter_stmt={}）", out.matches("enter_stmt").count());
        assert!(!out.contains("let doubled = {"), "纯计算 let 不应被 guard 块包裹");
    }

    /// remember 替换为编译期 key 版本（remember_at_key——不同语句不同 key）
    #[test]
    fn test_remember_replaced_with_compile_key() {
        let body: syn::Block = syn::parse_quote!({
            ctx.remember(|| 0);
            ctx.remember(|| 1);
        });
        let mut counter = 0;
        let stmts = inject_stmt_ids(body.stmts, &ctx_ident(), &mut counter, 0x1234);
        let out = quote!(#(#stmts)*).to_string();
        assert_eq!(out.matches("remember_at_key").count(), 2, "两个 remember 都应替换");
        assert!(!out.contains("ctx.remember("), "不应残留未替换的 ctx.remember(");
        // 两个 key 不同（不同语句 id）
        let keys = extract_keys(&out);
        assert_eq!(keys.len(), 2, "应解析出 2 个 key（实际 {:?}）", keys);
        assert_ne!(keys[0], keys[1], "不同语句的 remember key 必须不同");
    }

    /// 同语句内两个 remember——key 不同（语句内序号区分）
    #[test]
    fn test_same_stmt_remember_keys_distinct() {
        let body: syn::Block = syn::parse_quote!({
            let (a, b) = (ctx.remember(|| 0), ctx.remember(|| 1));
        });
        let mut counter = 0;
        let stmts = inject_stmt_ids(body.stmts, &ctx_ident(), &mut counter, 0x1234);
        let out = quote!(#(#stmts)*).to_string();
        let keys = extract_keys(&out);
        assert_eq!(keys.len(), 2, "应解析出 2 个 key（实际 {:?}）", keys);
        assert_ne!(keys[0], keys[1], "同语句内两个 remember 序号不同 → key 不同");
    }

    /// content 闭包内的 remember 也替换（用闭包体语句 id——与顶层 key 不同）
    #[test]
    fn test_closure_remember_replaced() {
        let body: syn::Block = syn::parse_quote!({
            Column::new().build(ctx, |ctx| {
                ctx.remember(|| 0);
            });
        });
        let mut counter = 0;
        let stmts = inject_stmt_ids(body.stmts, &ctx_ident(), &mut counter, 0x1234);
        let out = quote!(#(#stmts)*).to_string();
        assert!(out.contains("remember_at_key"), "content 闭包内的 remember 也应替换");
    }
}
