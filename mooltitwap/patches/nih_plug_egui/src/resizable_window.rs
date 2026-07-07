//! Resizable window wrapper for Egui editor.

use egui_baseview::egui::emath::GuiRounding;
use egui_baseview::egui::{InnerResponse, UiBuilder};

use crate::egui::{pos2, CentralPanel, Context, Id, Rect, Response, Sense, Ui, Vec2};
use crate::EguiState;

/// Adds a corner to the plugin window that can be dragged in order to resize it.
/// Resizing happens through plugin API, hence a custom implementation is needed.
pub struct ResizableWindow {
    id: Id,
    min_size: Vec2,
    /// If Some((w, h)), the window is constrained to this aspect ratio.
    aspect_ratio: Option<(f32, f32)>,
}

impl ResizableWindow {
    pub fn new(id_source: impl std::hash::Hash) -> Self {
        Self {
            id: Id::new(id_source),
            min_size: Vec2::splat(16.0),
            aspect_ratio: None,
        }
    }

    /// Won't shrink to smaller than this
    #[inline]
    pub fn min_size(mut self, min_size: impl Into<Vec2>) -> Self {
        self.min_size = min_size.into();
        self
    }

    /// Lock the window to a fixed aspect ratio. Pass the original (width, height).
    #[inline]
    pub fn fixed_aspect_ratio(mut self, width: f32, height: f32) -> Self {
        self.aspect_ratio = Some((width, height));
        self
    }

    pub fn show<R>(
        self,
        context: &Context,
        egui_state: &EguiState,
        add_contents: impl FnOnce(&mut Ui) -> R,
    ) -> InnerResponse<R> {
        CentralPanel::default().show(context, move |ui| {
            let ui_rect = ui.clip_rect();
            let mut content_ui =
                ui.new_child(UiBuilder::new().max_rect(ui_rect).layout(*ui.layout()));

            let ret = add_contents(&mut content_ui);

            let corner_size = Vec2::splat(ui.visuals().resize_corner_size);
            let corner_rect = Rect::from_min_size(ui_rect.max - corner_size, corner_size);

            let corner_response = ui.interact(corner_rect, self.id.with("corner"), Sense::drag());

            if let Some(pointer_pos) = corner_response.interact_pointer_pos() {
                let raw_size = (pointer_pos - ui_rect.min + 0.5 * corner_response.rect.size())
                    .max(self.min_size);

                // Apply aspect ratio constraint if set: drive height from width.
                let desired_size = if let Some((aw, ah)) = self.aspect_ratio {
                    let ratio = ah / aw;
                    Vec2::new(raw_size.x, raw_size.x * ratio).max(self.min_size)
                } else {
                    raw_size
                };

                if corner_response.dragged() {
                    // CRITICAL: `desired_size` is in egui POINTS. If the plugin uses
                    // dynamic `set_pixels_per_point` to lock the canvas to a fixed
                    // logical-design size, then 1 egui point != 1 outer logical pixel.
                    // We need to scale the size up by the current canvas → outer
                    // logical-pixel ratio before requesting the host resize, otherwise
                    // every drag would shrink the window back to its canvas size.
                    let (cur_outer_w, _) = egui_state.size();
                    let canvas_w = ui_rect.width().max(1.0);
                    let outer_per_point = cur_outer_w as f32 / canvas_w;
                    let outer = desired_size * outer_per_point;

                    egui_state.set_requested_size((
                        outer.x.round() as u32,
                        outer.y.round() as u32,
                    ));
                }
            }

            paint_resize_corner(&content_ui, &corner_response);

            ret
        })
    }
}

pub fn paint_resize_corner(ui: &Ui, response: &Response) {
    let stroke = ui.style().interact(response).fg_stroke;

    let painter = ui.painter();
    let rect = response.rect.translate(-Vec2::splat(2.0)); // move away from the corner
    let cp = rect.max.round_to_pixels(painter.pixels_per_point());

    let mut w = 2.0;

    while w <= rect.width() && w <= rect.height() {
        painter.line_segment([pos2(cp.x - w, cp.y), pos2(cp.x, cp.y - w)], stroke);
        w += 4.0;
    }
}
