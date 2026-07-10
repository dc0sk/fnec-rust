// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Sweep chart canvas (GUI-CHK-009).
//!
//! A 2-D `canvas::Program` that plots the sweep result — SWR or |Z| against
//! frequency — with 1-2-5 axis ticks and a draggable frequency cursor. All the
//! numeric work (SWR, bounds, ticks, mapping, nearest point) lives in the
//! headless `nec_gui::plot` module; this file only turns those numbers into
//! canvas geometry.

use iced::mouse;
use iced::widget::canvas::{self, Frame, Geometry, Path, Stroke, Text};
use iced::{Point, Rectangle, Renderer, Size, Theme};

use nec_gui::app_state::Message;
use nec_gui::plot::{self, PlotMetric};
use nec_gui::solve::SweepPoint;

/// SWR values above this are clamped for display (an infinite SWR would flatten
/// the rest of the curve against the axis).
const SWR_DISPLAY_MAX: f64 = 10.0;

const MARGIN_LEFT: f32 = 44.0;
const MARGIN_RIGHT: f32 = 12.0;
const MARGIN_TOP: f32 = 12.0;
const MARGIN_BOTTOM: f32 = 26.0;

/// The sweep chart, rebuilt each `view()` from the current sweep result.
pub struct SweepPlot {
    pub points: Vec<SweepPoint>,
    pub metric: PlotMetric,
    /// Frequency cursor as a fraction `0..=1` of the swept range.
    pub cursor_frac: f32,
}

impl SweepPlot {
    /// The plotted y-value of a point under the current metric (SWR clamped).
    fn value(&self, p: &SweepPoint) -> f64 {
        match self.metric {
            PlotMetric::Swr => plot::swr(p.z_re, p.z_im, 50.0).min(SWR_DISPLAY_MAX),
            PlotMetric::ZMag => plot::z_mag(p.z_re, p.z_im),
        }
    }
}

impl canvas::Program<Message> for SweepPlot {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let palette = theme.extended_palette();
        let axis_color = palette.background.strong.color;
        let grid_color = palette.background.weak.color;
        let curve_color = palette.primary.strong.color;
        let text_color = palette.background.strong.text;
        let cursor_color = palette.danger.strong.color;

        let Size {
            width: w,
            height: h,
        } = frame.size();
        let (px0, px1) = (MARGIN_LEFT, w - MARGIN_RIGHT);
        let (py0, py1) = (MARGIN_TOP, h - MARGIN_BOTTOM);
        if px1 <= px0 || py1 <= py0 {
            return vec![frame.into_geometry()];
        }

        // Plot frame.
        let border = Path::rectangle(Point::new(px0, py0), Size::new(px1 - px0, py1 - py0));
        frame.stroke(
            &border,
            Stroke::default().with_color(axis_color).with_width(1.0),
        );

        if self.points.len() < 2 {
            frame.fill_text(Text {
                content: "Run a sweep to plot it.".to_string(),
                position: Point::new(px0 + 8.0, py0 + 8.0),
                color: text_color,
                size: 13.0.into(),
                ..Text::default()
            });
            return vec![frame.into_geometry()];
        }

        let freqs: Vec<f64> = self.points.iter().map(|p| p.freq_mhz).collect();
        let values: Vec<f64> = self.points.iter().map(|p| self.value(p)).collect();
        let (fmin, fmax) = plot::finite_bounds(&freqs).unwrap_or((0.0, 1.0));
        let (mut vmin, mut vmax) = plot::finite_bounds(&values).unwrap_or((0.0, 1.0));
        // SWR floors at 1; give a flat curve some vertical breathing room.
        if self.metric == PlotMetric::Swr {
            vmin = vmin.min(1.0);
        }
        if (vmax - vmin).abs() < 1e-9 {
            vmax = vmin + 1.0;
        }

        let x_px = |f: f64| plot::map_range(f, fmin, fmax, px0 as f64, px1 as f64) as f32;
        let y_px = |v: f64| plot::map_range(v, vmin, vmax, py1 as f64, py0 as f64) as f32;

        // Grid + tick labels.
        for t in plot::nice_ticks(fmin, fmax, 6) {
            let x = x_px(t);
            frame.stroke(
                &Path::line(Point::new(x, py0), Point::new(x, py1)),
                Stroke::default().with_color(grid_color).with_width(1.0),
            );
            frame.fill_text(Text {
                content: format!("{t:.2}"),
                position: Point::new(x - 14.0, py1 + 4.0),
                color: text_color,
                size: 11.0.into(),
                ..Text::default()
            });
        }
        for t in plot::nice_ticks(vmin, vmax, 5) {
            let y = y_px(t);
            frame.stroke(
                &Path::line(Point::new(px0, y), Point::new(px1, y)),
                Stroke::default().with_color(grid_color).with_width(1.0),
            );
            frame.fill_text(Text {
                content: format!("{t:.1}"),
                position: Point::new(2.0, y - 6.0),
                color: text_color,
                size: 11.0.into(),
                ..Text::default()
            });
        }

        // Axis titles.
        frame.fill_text(Text {
            content: format!("{} vs f (MHz)", self.metric.label()),
            position: Point::new(px0 + 4.0, 0.0),
            color: text_color,
            size: 11.0.into(),
            ..Text::default()
        });

        // The curve.
        let curve = Path::new(|b| {
            for (i, (&f, &v)) in freqs.iter().zip(values.iter()).enumerate() {
                let p = Point::new(x_px(f), y_px(v));
                if i == 0 {
                    b.move_to(p);
                } else {
                    b.line_to(p);
                }
            }
        });
        frame.stroke(
            &curve,
            Stroke::default().with_color(curve_color).with_width(2.0),
        );

        // Frequency cursor + selected-point marker.
        let cursor_f = plot::map_range(f64::from(self.cursor_frac), 0.0, 1.0, fmin, fmax);
        if let Some(idx) = plot::nearest_index(&freqs, cursor_f) {
            let x = x_px(freqs[idx]);
            frame.stroke(
                &Path::line(Point::new(x, py0), Point::new(x, py1)),
                Stroke::default().with_color(cursor_color).with_width(1.0),
            );
            let marker = Path::circle(Point::new(x, y_px(values[idx])), 3.0);
            frame.fill(&marker, cursor_color);
        }

        vec![frame.into_geometry()]
    }
}
