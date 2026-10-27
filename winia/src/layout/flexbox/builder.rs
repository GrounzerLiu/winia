//! The composable wrapper: the builder that hands the policy to the runtime.

use super::config::{FlexBoxConfig, FlexBoxConfigInput};
use super::policy::FlexBoxLayout;
use crate::layout::container::build_container;
use crate::modifier::Modifier;
use crate::runtime::composer::ComposeCtx;
use std::sync::Arc;

/// Bumped for every build of a resolver-bearing FlexBox, so the group always Enters (see
/// FlexBox::build). A counter rather than a pointer: an allocator may reuse the address of a freed
/// closure, which would silently Skip again.
static RESOLVER_GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

// ── The builder ──

/// A CSS-flex container — Compose's FlexBox (FlexBox.kt:133).
///
/// FlexItemConfig travels on each child's modifier:
///
///     FlexBox::new()
///         .config(FlexBoxConfig::new().wrap(FlexWrap::Wrap).gap(8.0))
///         .build(ctx, |ctx| {
///             Stack::new()
///                 .modifier(Modifier::new().flex(FlexItemConfig::new().grow(1.0)))
///                 .build(ctx, |_| {});
///         });
#[derive(Default)]
pub struct FlexBox {
    config: FlexBoxConfig,
    resolver: Option<Arc<dyn Fn(&FlexBoxConfigInput) -> FlexBoxConfig>>,
    modifier: Modifier,
}

impl FlexBox {
    pub fn new() -> Self {
        FlexBox {
            config: FlexBoxConfig::new(),
            resolver: None,
            modifier: Modifier::new(),
        }
    }

    /// A fixed configuration.
    pub fn config(mut self, config: FlexBoxConfig) -> Self {
        self.config = config;
        self.resolver = None;
        self
    }

    /// A configuration resolved at MEASURE time from the incoming constraints and the density —
    /// Compose's FlexBoxConfig lambda (FlexBox.kt:1836-2030).
    ///
    /// Read everything the resolver depends on INSIDE the closure (a `State::get()` there registers a
    /// LAYOUT dependency, so a write re-measures). A value captured from the enclosing composition is
    /// frozen once the container Skips: the Skip decision only compares the declared parameters, and
    /// a new closure is not one of them — Compose sidesteps this with `rememberUpdatedState`, which
    /// has no equivalent here.
    ///
    /// Calling config(..) after config_for(..) drops the resolver; the last call wins.
    pub fn config_for(
        mut self,
        resolver: impl Fn(&FlexBoxConfigInput) -> FlexBoxConfig + 'static,
    ) -> Self {
        self.resolver = Some(Arc::new(resolver));
        self
    }

    /// The container's own modifier; the caller's chain comes last.
    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    pub fn build(self, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        // Declare every parameter that decides the layout, so an unchanged container Skips
        // (docs/component-authoring.md 3.2).
        ctx.changed(&self.config);
        let responsive = self.resolver.is_some();
        ctx.changed(&responsive);
        let direction = self
            .modifier
            .get_layout_direction()
            .unwrap_or(crate::layout::direction::current());
        ctx.changed(&direction);
        // A responsive config is a NEW closure on every build, and a closure is not one of the
        // parameters the Skip decision compares — worse, the policy is only installed when the group
        // ENTERS, so a Skip would keep the previous closure forever (Compose compares the config
        // OBJECT instead, so a changed lambda is a changed parameter there). A resolver-bearing
        // FlexBox therefore never Skips: the generation below is a new value on every build, which
        // makes the group Enter and the node dirty. Its cost is a rebuild of this subtree whenever the
        // enclosing scope composes — which is exactly when the config could have changed.
        if self.resolver.is_some() {
            let generation = RESOLVER_GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            ctx.changed(&generation);
        }
        // The item-animation host is cheap and always present; only items carrying the marker
        // allocate state. It rides on the modifier too so the render pass can find the container's
        // exit store and capture the removed items' pictures.
        let item_animations = ctx
            .remember_at_key(ctx.position_key(0x666c7862), || {
                crate::layout::item_animation::ItemAnimationHost::default()
            })
            .peek();
        let modifier = self
            .modifier
            .then(Modifier::new().with_item_animation_host(item_animations.clone()));
        build_container(
            ctx,
            modifier,
            FlexBoxLayout {
                config: self.config,
                resolver: self.resolver,
                direction,
                item_animations,
            },
            content,
        );
    }
}

