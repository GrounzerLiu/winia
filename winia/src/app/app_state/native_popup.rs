use super::*;
use crate::app::WindowRole;
use crate::app::overlay_host::{
    NativePopupBinding, begin_overlay_close, layout_overlays, sync_overlay_desc,
};
use crate::app::window_state::WindowCapabilities;
use winit::dpi::{LogicalPosition, LogicalSize, PhysicalSize, Position, Size};
use winit::raw_window_handle::HasWindowHandle;
use winit::window::{WindowAttributes, WindowId, WindowPositioner, WindowType};

fn native_overlay_desc(mut desc: crate::overlay::OverlayDesc) -> crate::overlay::OverlayDesc {
    desc.anchor_slot = None;
    desc.position = crate::overlay::PopupPosition::TopLeft;
    desc.offset = (0.0, 0.0);
    desc.fit_around_anchor = false;
    desc.match_anchor_width = false;
    desc
}

#[crate::composable]
fn native_popup_root(ctx: &mut crate::runtime::composer::ComposeCtx) {
    crate::layout::Column::new().build(ctx, |_| {});
}

impl AppState {
    pub(crate) fn close_native_popup_children(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        parent_id: WindowId,
    ) {
        let children: Vec<WindowId> = self
            .windows
            .iter()
            .filter_map(|(id, pw)| match pw.role {
                WindowRole::NativePopup { parent, .. } if parent == parent_id => Some(*id),
                _ => None,
            })
            .collect();
        for child_id in children {
            self.close_window_now(event_loop, child_id);
        }
    }

    pub(crate) fn has_focused_native_popup(&self, parent_id: WindowId, exclude: WindowId) -> bool {
        self.windows.iter().any(|(id, pw)| {
            *id != exclude
                && matches!(pw.role, WindowRole::NativePopup { parent, .. } if parent == parent_id)
                && (pw.native_popup_restore_focus
                    || pw.focused_id.is_some()
                    || pw
                        .overlay
                        .layers
                        .iter()
                        .any(|overlay| overlay.focused_id.is_some()))
        })
    }

    pub(crate) fn sync_native_popups(&mut self, event_loop: &dyn ActiveEventLoop) {
        let parents: Vec<WindowId> = self
            .windows
            .iter()
            .filter_map(|(id, pw)| {
                (!pw.pending_native_overlays.is_empty()
                    || pw
                        .overlay
                        .native_popups
                        .iter()
                        .any(|binding| !binding.active))
                .then_some(*id)
            })
            .collect();

        for parent_id in parents {
            let descs = self
                .windows
                .get_mut(&parent_id)
                .map(|pw| std::mem::take(&mut pw.pending_native_overlays))
                .unwrap_or_default();
            for desc in descs {
                self.sync_native_popup_desc(event_loop, parent_id, desc);
            }

            let inactive: Vec<(u64, WindowId)> = self
                .windows
                .get(&parent_id)
                .map(|pw| {
                    pw.overlay
                        .native_popups
                        .iter()
                        .filter(|binding| !binding.active)
                        .map(|binding| (binding.id, binding.child))
                        .collect()
                })
                .unwrap_or_default();
            for (overlay_id, child_id) in inactive {
                if let Some(child) = self.windows.get_mut(&child_id) {
                    begin_overlay_close(child, overlay_id);
                    if let Some(window) = child.skia_window.as_ref() {
                        window.request_redraw();
                    }
                }
            }
        }

        let finished: Vec<WindowId> = self
            .windows
            .iter()
            .filter_map(|(id, pw)| match pw.role {
                WindowRole::NativePopup { .. } if pw.overlay.layers.is_empty() => Some(*id),
                _ => None,
            })
            .collect();
        for child_id in finished {
            self.close_window_now(event_loop, child_id);
        }
        self.sync_native_popup_visibility();
    }

    fn sync_native_popup_desc(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        parent_id: WindowId,
        mut desc: crate::overlay::OverlayDesc,
    ) {
        let supported = WindowCapabilities::current().native_popup
            && !desc.focus_scope
            && !desc.click_passthrough;
        if !supported {
            if let Some(parent) = self.windows.get_mut(&parent_id) {
                if let Some(binding) = parent
                    .overlay
                    .native_popups
                    .iter_mut()
                    .find(|binding| binding.id == desc.id)
                {
                    binding.active = false;
                }
                desc.host = crate::overlay::PopupHost::InWindow;
                sync_overlay_desc(parent, desc);
            }
            return;
        }

        let existing = self.windows.get(&parent_id).and_then(|pw| {
            pw.overlay
                .native_popups
                .iter()
                .find(|binding| binding.id == desc.id)
                .map(|binding| binding.child)
        });

        if let Some(child_id) = existing {
            if let Some(child) = self.windows.get_mut(&child_id) {
                sync_overlay_desc(child, native_overlay_desc(desc));
                if let Some(window) = child.skia_window.as_ref() {
                    window.request_redraw();
                }
                self.update_native_popup_geometry(parent_id, child_id);
                return;
            }
            if let Some(parent) = self.windows.get_mut(&parent_id) {
                parent
                    .overlay
                    .native_popups
                    .retain(|binding| binding.id != desc.id);
            }
        }

        match self.create_native_popup(event_loop, parent_id, desc) {
            Ok(child_id) => {
                if let Some(window) = self
                    .windows
                    .get(&child_id)
                    .and_then(|pw| pw.skia_window.as_ref())
                {
                    window.request_redraw();
                }
            }
            Err(mut desc) => {
                log::warn!("native popup creation failed; falling back to an in-window overlay");
                desc.host = crate::overlay::PopupHost::InWindow;
                if let Some(parent) = self.windows.get_mut(&parent_id) {
                    sync_overlay_desc(parent, desc);
                }
            }
        }
    }

    fn create_native_popup(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        parent_id: WindowId,
        desc: crate::overlay::OverlayDesc,
    ) -> Result<WindowId, crate::overlay::OverlayDesc> {
        let Some(parent) = self.windows.get(&parent_id) else {
            return Err(desc);
        };
        let Some(parent_surface) = parent.skia_window.as_ref() else {
            return Err(desc);
        };
        let parent_window = parent_surface.window();
        let raw_parent = match parent_window.window_handle() {
            Ok(handle) => handle.as_raw(),
            Err(error) => {
                log::warn!("native popup parent handle unavailable: {error}");
                return Err(desc);
            }
        };
        let positioner = popup_positioner(parent, &desc);
        let attributes = WindowAttributes::default().with_window_type(WindowType::Popup);
        // SAFETY: the parent window is alive and borrowed from its live PerWindow for this call.
        let attributes = unsafe { attributes.with_parent_window(Some(raw_parent)) }
            .with_surface_size(PhysicalSize::new(1, 1))
            .with_positioner(positioner)
            .with_visible(false)
            .with_active(false)
            .with_decorations(false)
            .with_resizable(false)
            .with_transparent(true);
        let window = match event_loop.create_window(attributes) {
            Ok(window) => window,
            Err(error) => {
                log::warn!("native popup create_window failed: {error}");
                return Err(desc);
            }
        };
        let window_id = window.id();
        let scale_factor = window.scale_factor();
        let surface = WgpuWindow::new_async(event_loop, std::sync::Arc::new(window), true);
        crate::accessibility::install(&*surface, window_id.into_raw() as u64);

        let measurement_size = native_measurement_size(parent_window);
        let mut child = PerWindow::new(
            Box::new(native_popup_root),
            measurement_size.0,
            measurement_size.1,
            parent.theme.applied.colors,
        );
        child.skia_window = Some(surface);
        child.transparent = true;
        child.scale_factor = scale_factor;
        child.owner = Some(parent_id.into_raw() as u64);
        child.role = WindowRole::NativePopup {
            parent: parent_id,
            overlay_id: desc.id,
        };
        child.insets = parent.insets.clone();
        child.composer.set_window_insets(child.insets.clone());
        child.composer.adaptive = parent.composer.adaptive.clone();
        child
            .composer
            .set_back_dispatcher(parent.composer.back_dispatcher());
        child.composer.set_window_id(window_id.into_raw() as u64);
        child.clock.frame_interval = parent.clock.frame_interval;

        let overlay_id = desc.id;
        let anchor_slot = desc.anchor_slot;
        let direction = desc.direction;
        let position = desc.position;
        let offset = desc.offset;
        let fit_around_anchor = desc.fit_around_anchor;
        let match_anchor_width = desc.match_anchor_width;
        sync_overlay_desc(&mut child, native_overlay_desc(desc));
        layout_overlays(&mut child);
        if let Some(overlay) = child.overlay.layers.first_mut() {
            overlay.surface = crate::app::overlay_host::OverlaySurface::Native;
            overlay.screen_pos = (0.0, 0.0);
            overlay.composer.screen_origin = (0.0, 0.0);
        }

        self.windows.insert(window_id, child);
        if let Some(parent) = self.windows.get_mut(&parent_id) {
            parent.overlay.native_popups.push(NativePopupBinding {
                id: overlay_id,
                child: window_id,
                anchor_slot,
                direction,
                position,
                offset,
                fit_around_anchor,
                match_anchor_width,
                active: true,
            });
        }
        self.update_native_popup_geometry(parent_id, window_id);
        self.sync_native_popup_visibility();
        Ok(window_id)
    }

    fn sync_native_popup_visibility(&mut self) {
        let bindings: Vec<(WindowId, WindowId)> = self
            .windows
            .iter()
            .flat_map(|(parent_id, pw)| {
                pw.overlay
                    .native_popups
                    .iter()
                    .map(move |binding| (*parent_id, binding.child))
            })
            .collect();
        for (parent_id, child_id) in bindings {
            let Some(parent) = self.windows.get(&parent_id) else {
                continue;
            };
            let Some(parent_surface) = parent.skia_window.as_ref() else {
                continue;
            };
            let parent_window = parent_surface.window();
            let Some(visible) = parent_window.is_visible() else {
                continue;
            };
            let minimized = parent_window.is_minimized().unwrap_or(false);
            if let Some(child) = self.windows.get(&child_id)
                && let Some(child_surface) = child.skia_window.as_ref()
            {
                let child_window = child_surface.window();
                let should_show = visible && !minimized;
                if child_window.is_visible() != Some(should_show) {
                    child_window.set_visible(should_show);
                    if should_show {
                        child_surface.request_redraw();
                    }
                }
            }
        }
    }

    pub(crate) fn refresh_native_popup_positions(&mut self, parent_id: WindowId) {
        let children: Vec<WindowId> = self
            .windows
            .get(&parent_id)
            .map(|pw| {
                pw.overlay
                    .native_popups
                    .iter()
                    .map(|binding| binding.child)
                    .collect()
            })
            .unwrap_or_default();
        for child_id in children {
            self.update_native_popup_geometry(parent_id, child_id);
        }
    }

    pub(crate) fn dismiss_native_popups_outside(
        &mut self,
        parent_id: WindowId,
        position: (f32, f32),
    ) {
        let Some(parent) = self.windows.get(&parent_id) else {
            return;
        };
        let bindings = parent.overlay.native_popups.clone();
        let mut close = Vec::new();
        for binding in bindings {
            let Some(child) = self.windows.get(&binding.child) else {
                continue;
            };
            let Some(overlay) = child
                .overlay
                .layers
                .iter()
                .find(|overlay| overlay.id == binding.id)
            else {
                continue;
            };
            if !overlay.dismiss_on_outside || overlay.closing {
                continue;
            }
            let placement =
                popup_placement_for_binding(parent, &binding, (child.width, child.height));
            let (left, top) = placement.to_parent_origin();
            let inside = position.0 >= left
                && position.0 < left + child.width
                && position.1 >= top
                && position.1 < top + child.height;
            if !inside {
                close.push((binding.child, binding.id));
            }
        }
        for (child_id, overlay_id) in close {
            if let Some(child) = self.windows.get_mut(&child_id) {
                begin_overlay_close(child, overlay_id);
                if let Some(surface) = child.skia_window.as_ref() {
                    surface.request_redraw();
                }
            }
        }
    }

    fn update_native_popup_geometry(&mut self, parent_id: WindowId, child_id: WindowId) {
        let Some(binding) = self
            .windows
            .get(&parent_id)
            .and_then(|pw| {
                pw.overlay
                    .native_popups
                    .iter()
                    .find(|binding| binding.child == child_id)
            })
            .cloned()
        else {
            return;
        };
        let Some(measurement_size) = self.windows.get(&parent_id).and_then(|parent| {
            parent
                .skia_window
                .as_ref()
                .map(|surface| native_measurement_size(surface.window()))
        }) else {
            return;
        };
        let size = {
            let Some(child) = self.windows.get_mut(&child_id) else {
                return;
            };
            child.width = measurement_size.0;
            child.height = measurement_size.1;
            layout_overlays(child);
            child
                .overlay
                .layers
                .first()
                .and_then(|overlay| overlay.composer.layout_root_idx())
                .map(|root| {
                    let measured =
                        child.overlay.layers[0].composer.arena_nodes()[root].measured_size;
                    (measured.width.max(1.0), measured.height.max(1.0))
                })
                .unwrap_or((1.0, 1.0))
        };
        let Some(parent) = self.windows.get(&parent_id) else {
            return;
        };
        let placement = popup_placement_for_binding(parent, &binding, size);
        let positioner = placement.to_winit_positioner();
        let origin = placement.to_parent_origin();
        let Some(child) = self.windows.get_mut(&child_id) else {
            return;
        };
        if let Some(surface) = child.skia_window.as_ref() {
            let window = surface.window();
            let current = window.surface_size().to_logical::<f32>(child.scale_factor);
            if (current.width - size.0).abs() > 0.5 || (current.height - size.1).abs() > 0.5 {
                let _ = window.request_surface_size(Size::Logical(LogicalSize::new(
                    size.0 as f64,
                    size.1 as f64,
                )));
            }
            if window.positioner() != positioner {
                window.set_positioner(positioner);
            }
            let _ = window.set_outer_position(Position::Logical(LogicalPosition::new(
                origin.0 as f64,
                origin.1 as f64,
            )));
        }
        child.width = size.0;
        child.height = size.1;
        layout_overlays(child);
        if let Some(overlay) = child.overlay.layers.first_mut() {
            overlay.screen_pos = (0.0, 0.0);
            overlay.composer.screen_origin = (0.0, 0.0);
        }
    }
}

fn native_measurement_size(window: &dyn winit::window::Window) -> (f32, f32) {
    crate::app::native_popup::work_area::from_window(window)
        .map(|area| area.size)
        .unwrap_or_else(|| parent_measurement_size(window.surface_size(), window.scale_factor()))
}

fn parent_measurement_size(size: PhysicalSize<u32>, scale: f64) -> (f32, f32) {
    if !scale.is_finite() || scale <= 0.0 {
        return (1.0, 1.0);
    }
    let logical = size.to_logical::<f32>(scale);
    (logical.width.max(1.0), logical.height.max(1.0))
}

fn native_fit_bounds(parent: &PerWindow) -> Option<((f32, f32), (f32, f32))> {
    parent
        .skia_window
        .as_ref()
        .and_then(|surface| crate::app::native_popup::work_area::from_window(surface.window()))
        .map(|area| (area.origin, area.size))
}

fn popup_positioner(parent: &PerWindow, desc: &crate::overlay::OverlayDesc) -> WindowPositioner {
    popup_placement(
        parent,
        desc.anchor_slot,
        desc.direction,
        desc.position,
        desc.offset,
        desc.fit_around_anchor,
        desc.match_anchor_width,
        (1.0, 1.0),
    )
    .to_winit_positioner()
}

fn popup_placement_for_binding(
    parent: &PerWindow,
    binding: &NativePopupBinding,
    content_size: (f32, f32),
) -> crate::app::native_popup::PopupPlacement {
    let placement = popup_placement(
        parent,
        binding.anchor_slot,
        binding.direction,
        binding.position,
        binding.offset,
        binding.fit_around_anchor,
        binding.match_anchor_width,
        content_size,
    );
    match native_fit_bounds(parent) {
        Some(bounds) => placement.with_fit_bounds(bounds),
        None => placement,
    }
}

fn popup_placement(
    parent: &PerWindow,
    anchor_slot: Option<u64>,
    requested_direction: Option<crate::layout::LayoutDirection>,
    position: crate::overlay::PopupPosition,
    offset: (f32, f32),
    fit_around_anchor: bool,
    match_anchor_width: bool,
    content_size: (f32, f32),
) -> crate::app::native_popup::PopupPlacement {
    let nodes = parent.composer.arena_nodes();
    let root = parent.composer.layout_root_idx();
    let anchor_node =
        anchor_slot.and_then(|slot| nodes.iter().position(|node| node.slot_key == slot));
    let direction = requested_direction
        .or_else(|| anchor_node.map(|index| nodes[index].layout_direction))
        .or_else(|| root.map(|index| nodes[index].layout_direction))
        .unwrap_or(crate::layout::LayoutDirection::Ltr);
    let anchor_rect = anchor_node.map(|index| {
        let node = &nodes[index];
        (
            node.position.x,
            node.position.y,
            node.measured_size.width,
            node.measured_size.height,
        )
    });
    let content_size = if match_anchor_width {
        anchor_rect
            .map(|rect| (rect.2.max(1.0), content_size.1))
            .unwrap_or(content_size)
    } else {
        content_size
    };
    crate::app::native_popup::PopupPlacement::new(
        (0.0, 0.0),
        anchor_rect,
        content_size,
        (parent.width, parent.height),
        position,
        direction,
        offset,
        fit_around_anchor,
        match_anchor_width,
    )
}

#[cfg(test)]
mod tests {
    use super::parent_measurement_size;
    use winit::dpi::PhysicalSize;

    #[test]
    fn missing_work_area_falls_back_to_the_parent_window_size() {
        assert_eq!(
            parent_measurement_size(PhysicalSize::new(800, 600), 2.0),
            (400.0, 300.0)
        );
        assert_eq!(
            parent_measurement_size(PhysicalSize::new(0, 0), 1.0),
            (1.0, 1.0)
        );
        assert_eq!(
            parent_measurement_size(PhysicalSize::new(800, 600), 0.0),
            (1.0, 1.0)
        );
    }
}
