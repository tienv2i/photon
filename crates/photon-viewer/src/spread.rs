use std::collections::HashMap;

use iced::widget::{Shader, container};
use iced::{Element, Fill};
use photon_canvas::{CanvasEvent, CanvasTool, ImageCanvasProgram};
use photon_protocol::Language;
use photon_ui::theme::canvas_backdrop_style;

use crate::types::{DualSpreadSlots, ViewerMessage, ViewerViewState};

pub(crate) fn build_viewer_double_panes<'a>(
    state: ViewerViewState<'a>,
    _lang: Language,
    dark: bool,
    canvas_tool: CanvasTool,
) -> Element<'a, ViewerMessage> {
    let gap = f32::from(state.viewer_manga_page_gap);
    let (left_slot, right_slot) = dual_spread_slots(state);
    let left_path = left_slot.map(|(_, p)| p.to_owned());
    let right_path = right_slot.map(|(_, p)| p.to_owned());

    let canvas: Element<'_, ViewerMessage> = Element::from(
        Shader::new(ImageCanvasProgram {
            frame: state.viewer_frame.cloned(),
            pair_frame: state.viewer_pair_frame.cloned(),
            dual_gap_px: gap,
            compare_frame: None,
            zoom: state.zoom,
            pan_x: state.pan_x,
            pan_y: state.pan_y,
            adjustments: state.viewer_adj,
            dark_theme: dark,
            crop_rect: None,
            crop_aspect: None,
            quarter_turns: state.viewer_quarter_turns,
            tool: canvas_tool,
            preview_angle_rad: 0.0,
            preview_fill: [0.0; 4],
            canvas_bg: None,
            viewport_bg: state.viewport_background,
            compare_mode: 0,
            compare_split_x: 0.5,
            brush_radius_px: 0.0,
            selection_mask: None,
            has_selection: false,
            quick_mask_active: false,
            marching_ants_phase: 0.0,
            restrict_pan_to_hand: false,
            stabilizer_radius: 0.0,
            aa_mode: state.aa_mode,
            denoise_mode: state.denoise_mode,
            ..Default::default()
        })
        .width(Fill)
        .height(Fill),
    )
    .map(move |ev| match ev {
        CanvasEvent::ContextMenuRequested { is_right_pair, .. } => {
            let target = if is_right_pair {
                right_path.clone().or_else(|| left_path.clone())
            } else {
                left_path.clone()
            };
            ViewerMessage::ViewerCanvas(target, ev)
        }
        other => ViewerMessage::ViewerCanvas(None, other),
    });

    container(canvas)
        .width(Fill)
        .height(Fill)
        .style(canvas_backdrop_style)
        .into()
}

pub fn compute_dual_spreads(
    images: &[String],
    sizes: &HashMap<String, (u32, u32)>,
    separate_cover: bool,
) -> Vec<Vec<usize>> {
    let count = images.len();
    if count == 0 {
        return Vec::new();
    }
    let is_landscape = |idx: usize| -> bool {
        images
            .get(idx)
            .and_then(|p| sizes.get(p))
            .is_some_and(|(w, h)| *w > 0 && *h > 0 && *w > *h)
    };
    let mut spreads = Vec::new();
    let mut i = 0;
    if separate_cover && count > 1 {
        spreads.push(vec![0]);
        i = 1;
    }
    while i < count {
        if is_landscape(i) || i + 1 >= count || is_landscape(i + 1) {
            spreads.push(vec![i]);
            i += 1;
        } else {
            spreads.push(vec![i, i + 1]);
            i += 2;
        }
    }
    spreads
}

pub fn dual_spread_slots_for<'a>(
    images: &'a [String],
    sizes: &HashMap<String, (u32, u32)>,
    current_index: Option<usize>,
    separate_cover: bool,
    manga_rtl: bool,
) -> DualSpreadSlots<'a> {
    let spreads = compute_dual_spreads(images, sizes, separate_cover);
    if spreads.is_empty() {
        return (None, None);
    }
    let idx = current_index.unwrap_or(0);
    let spread = spreads
        .iter()
        .find(|sp| sp.contains(&idx))
        .unwrap_or(&spreads[0]);
    if spread.len() == 1 {
        let i = spread[0];
        return (images.get(i).map(|p| (i, p.as_str())), None);
    }
    let (li, ri) = if manga_rtl {
        (spread[1], spread[0])
    } else {
        (spread[0], spread[1])
    };
    (
        images.get(li).map(|p| (li, p.as_str())),
        images.get(ri).map(|p| (ri, p.as_str())),
    )
}

pub fn dual_spread_slots<'a>(state: ViewerViewState<'a>) -> DualSpreadSlots<'a> {
    dual_spread_slots_for(
        &state.snapshot.viewer_images,
        state.image_sizes,
        state.snapshot.meta.viewer_index,
        state.viewer_manga_cover_separate,
        state.viewer_manga_rtl,
    )
}

pub fn step_dual_spread(state: ViewerViewState<'_>, delta: isize) -> Option<String> {
    let images = &state.snapshot.viewer_images;
    let spreads =
        compute_dual_spreads(images, state.image_sizes, state.viewer_manga_cover_separate);
    if spreads.is_empty() {
        return None;
    }
    let cur_img = state.snapshot.meta.viewer_index.unwrap_or(0);
    let cur_sp = spreads
        .iter()
        .position(|sp| sp.contains(&cur_img))
        .unwrap_or(0) as isize;
    let next_sp = (cur_sp + delta).rem_euclid(spreads.len() as isize) as usize;
    let target_idx = spreads[next_sp][0];
    images.get(target_idx).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_dual_spreads_respects_separate_cover_and_landscape() {
        let imgs = vec![
            "/tmp/01.png".to_owned(),
            "/tmp/02.png".to_owned(),
            "/tmp/03.png".to_owned(),
            "/tmp/04.png".to_owned(),
            "/tmp/05.png".to_owned(),
        ];
        let mut sizes = HashMap::new();
        sizes.insert("/tmp/01.png".to_owned(), (800, 1200));
        sizes.insert("/tmp/02.png".to_owned(), (800, 1200));
        sizes.insert("/tmp/03.png".to_owned(), (800, 1200));
        sizes.insert("/tmp/04.png".to_owned(), (1600, 900));
        sizes.insert("/tmp/05.png".to_owned(), (800, 1200));

        let spreads = compute_dual_spreads(&imgs, &sizes, true);
        assert_eq!(spreads, vec![vec![0], vec![1, 2], vec![3], vec![4]]);

        let (left, right) = dual_spread_slots_for(&imgs, &sizes, Some(1), true, true);
        assert_eq!(left, Some((2, "/tmp/03.png")));
        assert_eq!(right, Some((1, "/tmp/02.png")));
    }
}
