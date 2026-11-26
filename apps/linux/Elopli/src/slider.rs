use std::cell::RefCell;
use std::f64::consts::PI;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{cairo, gdk};

use crate::theme;

const HEIGHT: i32 = 40;
const INSET: f64 = 21.0;

type ChangeCallback = Rc<dyn Fn(u8)>;
type LinkCallback = Rc<dyn Fn()>;
type SharedChange = Rc<RefCell<Option<ChangeCallback>>>;
type SharedLink = Rc<RefCell<Option<LinkCallback>>>;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SliderKind {
	/// Brightness with the tapered track; the fill follows the warmth colour.
	Brightness,
	/// Warmth with a warm-to-cool gradient.
	Warmth,
	/// A plain accent-filled slider between two values.
	Plain { min: f64, max: f64 },
}

impl SliderKind {
	fn mapping(self) -> Mapping {
		match self {
			Self::Brightness => Mapping {
				min: 2.0,
				max: 100.0,
				reversed: false,
			},
			Self::Warmth => Mapping {
				min: 0.0,
				max: 100.0,
				reversed: true,
			},
			Self::Plain { min, max } => Mapping {
				min,
				max,
				reversed: false,
			},
		}
	}
}

#[derive(Debug, Clone, Copy)]
struct Mapping {
	min: f64,
	max: f64,
	reversed: bool,
}

impl Mapping {
	fn fraction(self, value: f64) -> f64 {
		let span = (self.max - self.min).max(0.0001);
		let fraction = ((value.clamp(self.min, self.max) - self.min) / span).clamp(0.0, 1.0);
		if self.reversed {
			1.0 - fraction
		} else {
			fraction
		}
	}

	fn value(self, fraction: f64) -> f64 {
		let fraction = if self.reversed {
			1.0 - fraction.clamp(0.0, 1.0)
		} else {
			fraction.clamp(0.0, 1.0)
		};
		self.min + fraction * (self.max - self.min)
	}
}

struct SliderState {
	value: f64,
	mapping: Mapping,
	kind: SliderKind,
	warmth: f64,
	active: bool,
	dragging: bool,
}

/// A custom slider that matches the macOS and Windows tracks.
#[derive(Clone)]
pub struct Slider {
	area: gtk::DrawingArea,
	state: Rc<RefCell<SliderState>>,
	on_change: SharedChange,
	on_link: SharedLink,
}

impl Slider {
	pub fn new(kind: SliderKind, value: u8) -> Self {
		let mapping = kind.mapping();
		let state = Rc::new(RefCell::new(SliderState {
			value: f64::from(value),
			mapping,
			kind,
			warmth: 50.0,
			active: true,
			dragging: false,
		}));
		let area = gtk::DrawingArea::builder()
			.content_height(HEIGHT)
			.hexpand(true)
			.vexpand(false)
			.build();
		area.add_css_class("elopli-slider");
		area.set_cursor_from_name(Some("pointer"));

		{
			let state = Rc::clone(&state);
			area.set_draw_func(move |area, context, width, height| {
				draw_slider(area, context, width, height, &state.borrow());
			});
		}

		let on_change: SharedChange = Rc::new(RefCell::new(None));
		let on_link: SharedLink = Rc::new(RefCell::new(None));

		let click = gtk::GestureClick::new();
		click.set_button(1);
		{
			let state = Rc::clone(&state);
			let area_weak = area.downgrade();
			let on_change = Rc::clone(&on_change);
			let on_link = Rc::clone(&on_link);
			click.connect_pressed(move |gesture, _presses, x, _y| {
				let control = gesture
					.current_event_state()
					.contains(gdk::ModifierType::CONTROL_MASK);
				if control {
					state.borrow_mut().dragging = false;
					if let Some(callback) = on_link.borrow().as_ref() {
						callback();
					}
					return;
				}
				let Some(area) = area_weak.upgrade() else {
					return;
				};
				let value = value_at(&area, x, &state.borrow());
				set_value(&state, &area, value, true);
				if let Some(callback) = on_change.borrow().as_ref() {
					callback(value);
				}
			});
		}
		{
			let state = Rc::clone(&state);
			click.connect_released(move |_, _, _, _| {
				state.borrow_mut().dragging = false;
			});
		}
		area.add_controller(click);

		let motion = gtk::EventControllerMotion::new();
		{
			let state = Rc::clone(&state);
			let area_weak = area.downgrade();
			let on_change = Rc::clone(&on_change);
			motion.connect_motion(move |_, x, _y| {
				if !state.borrow().dragging {
					return;
				}
				let Some(area) = area_weak.upgrade() else {
					return;
				};
				let value = value_at(&area, x, &state.borrow());
				set_value(&state, &area, value, true);
				if let Some(callback) = on_change.borrow().as_ref() {
					callback(value);
				}
			});
		}
		area.add_controller(motion);

		Self {
			area,
			state,
			on_change,
			on_link,
		}
	}

	pub fn set_on_change(&self, callback: impl Fn(u8) + 'static) {
		*self.on_change.borrow_mut() = Some(Rc::new(callback));
	}

	pub fn set_on_link(&self, callback: impl Fn() + 'static) {
		*self.on_link.borrow_mut() = Some(Rc::new(callback));
	}

	pub fn value(&self) -> u8 {
		self.state.borrow().value.round().clamp(0.0, 100.0) as u8
	}

	pub fn widget(&self) -> &gtk::DrawingArea {
		&self.area
	}

	/// Update the displayed value without emitting a command.
	pub fn set_value(&self, value: u8) {
		set_value(&self.state, &self.area, value, false);
	}

	/// Update the colour source for a brightness slider.
	pub fn set_warmth(&self, warmth: u8) {
		let mut state = self.state.borrow_mut();
		if (state.warmth - f64::from(warmth)).abs() < 0.5 {
			return;
		}
		state.warmth = f64::from(warmth);
		drop(state);
		self.area.queue_draw();
	}

	pub fn set_active(&self, active: bool) {
		let mut state = self.state.borrow_mut();
		if state.active == active {
			return;
		}
		state.active = active;
		drop(state);
		self.area.queue_draw();
	}

	pub fn is_dragging(&self) -> bool {
		self.state.borrow().dragging
	}
}

fn set_value(state: &Rc<RefCell<SliderState>>, area: &gtk::DrawingArea, value: u8, dragging: bool) {
	let mut state = state.borrow_mut();
	let value = f64::from(value);
	let changed = (state.value - value).abs() > 0.1;
	state.value = value;
	if dragging {
		state.dragging = true;
	}
	let refresh = changed || dragging;
	drop(state);
	if refresh {
		area.queue_draw();
	}
}

fn value_at(area: &gtk::DrawingArea, x: f64, state: &SliderState) -> u8 {
	let width = f64::from(area.width());
	let fraction = ((x - INSET) / (width - INSET * 2.0).max(1.0)).clamp(0.0, 1.0);
	state.mapping.value(fraction).round().clamp(0.0, 100.0) as u8
}

fn draw_slider(
	area: &gtk::DrawingArea,
	context: &cairo::Context,
	width: i32,
	height: i32,
	state: &SliderState,
) {
	let palette = theme::Palette::for_widget(area);
	let width = f64::from(width);
	let height = f64::from(height);
	let center_y = height / 2.0;
	let track_width = (width - INSET * 2.0).max(1.0);
	let fraction = state.mapping.fraction(state.value);
	let thumb_x = INSET + track_width * fraction;

	match state.kind {
		SliderKind::Brightness => {
			let color = theme::temperature_color(state.warmth);
			if state.active {
				draw_glow(context, thumb_x, center_y, fraction, color);
			}
			draw_tapered_track(
				context,
				INSET,
				center_y,
				track_width,
				color,
				palette.track(),
			);
			draw_thumb(context, thumb_x, center_y, color, palette.knob_border());
		}
		SliderKind::Warmth => {
			draw_track(context, INSET, center_y, track_width, palette.track(), 7.0);
			draw_warmth_gradient(context, INSET, center_y, track_width);
			let color = theme::temperature_color(state.value);
			draw_thumb(context, thumb_x, center_y, color, palette.knob_border());
		}
		SliderKind::Plain { .. } => {
			draw_track(context, INSET, center_y, track_width, palette.track(), 6.0);
			draw_track(
				context,
				INSET,
				center_y,
				track_width * fraction,
				palette.accent,
				6.0,
			);
			draw_thumb(
				context,
				thumb_x,
				center_y,
				palette.accent,
				palette.knob_border(),
			);
		}
	}
}

fn draw_track(
	context: &cairo::Context,
	x: f64,
	center_y: f64,
	width: f64,
	color: gdk::RGBA,
	thickness: f64,
) {
	if width <= 0.5 {
		return;
	}
	let height = thickness / 2.0;
	rounded_rect(
		context,
		x - 1.0,
		center_y - height,
		width + 2.0,
		thickness,
		height,
	);
	context.set_source_rgba(
		f64::from(color.red()),
		f64::from(color.green()),
		f64::from(color.blue()),
		f64::from(color.alpha()),
	);
	let _ = context.fill();
}

fn draw_tapered_track(
	context: &cairo::Context,
	x: f64,
	center_y: f64,
	width: f64,
	color: gdk::RGBA,
	border: gdk::RGBA,
) {
	let left_radius = 1.5;
	let right_radius = 6.0;
	let left_center = x + left_radius;
	let right_center = x + (width - right_radius).max(left_center);

	context.new_path();
	context.move_to(left_center, center_y - left_radius);
	context.line_to(right_center, center_y - right_radius);
	context.arc(right_center, center_y, right_radius, -PI / 2.0, PI / 2.0);
	context.line_to(left_center, center_y + left_radius);
	context.arc(left_center, center_y, left_radius, PI / 2.0, PI * 1.5);
	context.close_path();

	context.set_source_rgba(
		f64::from(border.red()),
		f64::from(border.green()),
		f64::from(border.blue()),
		0.55,
	);
	context.set_line_width(1.0);
	let _ = context.stroke_preserve();

	context.set_source_rgba(
		f64::from(color.red()),
		f64::from(color.green()),
		f64::from(color.blue()),
		f64::from(color.alpha()),
	);
	let _ = context.fill();
}

fn draw_warmth_gradient(context: &cairo::Context, x: f64, center_y: f64, width: f64) {
	let gradient = cairo::LinearGradient::new(x, center_y, x + width, center_y);
	gradient.add_color_stop_rgba(0.0, 1.0, 0.62, 0.29, 1.0);
	gradient.add_color_stop_rgba(0.4, 1.0, 1.0, 1.0, 1.0);
	gradient.add_color_stop_rgba(1.0, 0.55, 0.77, 1.0, 1.0);
	let _ = context.set_source(&gradient);
	context.set_line_width(5.0);
	context.set_line_cap(cairo::LineCap::Round);
	context.move_to(x, center_y);
	context.line_to(x + width, center_y);
	let _ = context.stroke();
}

fn draw_glow(context: &cairo::Context, x: f64, y: f64, fraction: f64, color: gdk::RGBA) {
	let max_radius = 10.0 + fraction * 8.0;
	for (index, radius) in [
		max_radius,
		max_radius - 3.0,
		max_radius - 6.0,
		max_radius - 9.0,
	]
	.into_iter()
	.enumerate()
	{
		if radius > 8.0 {
			context.set_source_rgba(
				f64::from(color.red()),
				f64::from(color.green()),
				f64::from(color.blue()),
				0.03 + index as f64 * 0.02,
			);
			context.arc(x, y, radius, 0.0, PI * 2.0);
			let _ = context.fill();
		}
	}
}

fn draw_thumb(context: &cairo::Context, x: f64, y: f64, color: gdk::RGBA, border: gdk::RGBA) {
	context.set_source_rgba(
		f64::from(color.red()),
		f64::from(color.green()),
		f64::from(color.blue()),
		f64::from(color.alpha()),
	);
	context.arc(x, y, 8.0, 0.0, PI * 2.0);
	let _ = context.fill();

	context.set_source_rgba(
		f64::from(border.red()),
		f64::from(border.green()),
		f64::from(border.blue()),
		f64::from(border.alpha()),
	);
	context.set_line_width(1.2);
	context.arc(x, y, 8.0, 0.0, PI * 2.0);
	let _ = context.stroke();
}

fn rounded_rect(context: &cairo::Context, x: f64, y: f64, width: f64, height: f64, radius: f64) {
	let radius = radius.min(width / 2.0).min(height / 2.0).max(0.0);
	context.new_sub_path();
	context.arc(x + width - radius, y + radius, radius, -PI / 2.0, 0.0);
	context.arc(
		x + width - radius,
		y + height - radius,
		radius,
		0.0,
		PI / 2.0,
	);
	context.arc(x + radius, y + height - radius, radius, PI / 2.0, PI);
	context.arc(x + radius, y + radius, radius, PI, PI * 1.5);
	context.close_path();
}

/// A two-handle slider for the pulse low and peak values.
#[derive(Clone)]
pub struct RangeSlider {
	area: gtk::DrawingArea,
	state: Rc<RefCell<RangeState>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Handle {
	Low,
	Peak,
}

struct RangeState {
	low: f64,
	peak: f64,
	active: Option<Handle>,
	dragging: bool,
}

impl RangeSlider {
	pub fn new(low: u8, peak: u8, on_change: impl Fn(u8, u8) + 'static) -> Self {
		let state = Rc::new(RefCell::new(RangeState {
			low: f64::from(low.min(peak)),
			peak: f64::from(peak.max(low)),
			active: None,
			dragging: false,
		}));
		let area = gtk::DrawingArea::builder()
			.content_height(HEIGHT)
			.hexpand(true)
			.build();
		area.set_cursor_from_name(Some("pointer"));

		{
			let state = Rc::clone(&state);
			area.set_draw_func(move |area, context, width, height| {
				draw_range(area, context, width, height, &state.borrow());
			});
		}

		let on_change = Rc::new(on_change);

		let click = gtk::GestureClick::new();
		click.set_button(1);
		{
			let state = Rc::clone(&state);
			let area_weak = area.downgrade();
			let on_change = Rc::clone(&on_change);
			click.connect_pressed(move |_, _, x, _| {
				let Some(area) = area_weak.upgrade() else {
					return;
				};
				let (low, peak) = {
					let mut state = state.borrow_mut();
					let handle = nearest_handle(&area, x, &state);
					state.active = Some(handle);
					state.dragging = true;
					update_range(&area, x, &mut state, handle);
					(state.low.round() as u8, state.peak.round() as u8)
				};
				on_change(low, peak);
			});
		}
		{
			let state = Rc::clone(&state);
			click.connect_released(move |_, _, _, _| {
				let mut state = state.borrow_mut();
				state.dragging = false;
				state.active = None;
				let _ = &mut *state;
			});
		}
		area.add_controller(click);

		let motion = gtk::EventControllerMotion::new();
		{
			let state = Rc::clone(&state);
			let area_weak = area.downgrade();
			let on_change = Rc::clone(&on_change);
			motion.connect_motion(move |_, x, _| {
				let Some(active) = state.borrow().active else {
					return;
				};
				if !state.borrow().dragging {
					return;
				}
				let Some(area) = area_weak.upgrade() else {
					return;
				};
				let (low, peak) = {
					let mut state = state.borrow_mut();
					update_range(&area, x, &mut state, active);
					(state.low.round() as u8, state.peak.round() as u8)
				};
				on_change(low, peak);
			});
		}
		area.add_controller(motion);

		Self { area, state }
	}

	pub fn low(&self) -> u8 {
		self.state.borrow().low.round().clamp(0.0, 100.0) as u8
	}

	pub fn peak(&self) -> u8 {
		self.state.borrow().peak.round().clamp(2.0, 100.0) as u8
	}

	pub fn widget(&self) -> &gtk::DrawingArea {
		&self.area
	}
}

fn nearest_handle(area: &gtk::DrawingArea, x: f64, state: &RangeState) -> Handle {
	let width = f64::from(area.width());
	let track = (width - INSET * 2.0).max(1.0);
	let low_x = INSET + track * (state.low / 100.0);
	let peak_x = INSET + track * (state.peak / 100.0);
	if (x - low_x).abs() <= (x - peak_x).abs() {
		Handle::Low
	} else {
		Handle::Peak
	}
}

fn update_range(area: &gtk::DrawingArea, x: f64, state: &mut RangeState, handle: Handle) {
	let width = f64::from(area.width());
	let fraction = ((x - INSET) / (width - INSET * 2.0).max(1.0)).clamp(0.0, 1.0);
	let value = (fraction * 100.0).round();
	match handle {
		Handle::Low => state.low = value.min(state.peak),
		Handle::Peak => state.peak = value.max(state.low).max(2.0),
	}
	area.queue_draw();
}

fn draw_range(
	area: &gtk::DrawingArea,
	context: &cairo::Context,
	width: i32,
	height: i32,
	state: &RangeState,
) {
	let palette = theme::Palette::for_widget(area);
	let width = f64::from(width);
	let height = f64::from(height);
	let center_y = height / 2.0;
	let track = (width - INSET * 2.0).max(1.0);
	draw_track(context, INSET, center_y, track, palette.track(), 6.0);

	let low_x = INSET + track * (state.low / 100.0);
	let peak_x = INSET + track * (state.peak / 100.0);
	draw_track(
		context,
		low_x,
		center_y,
		(peak_x - low_x).max(0.0),
		palette.accent,
		6.0,
	);
	draw_thumb(
		context,
		low_x,
		center_y,
		palette.accent,
		palette.knob_border(),
	);
	draw_thumb(
		context,
		peak_x,
		center_y,
		palette.accent,
		palette.knob_border(),
	);
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn brightness_maps_from_two_to_one_hundred() {
		let mapping = SliderKind::Brightness.mapping();
		assert_eq!(mapping.fraction(2.0), 0.0);
		assert_eq!(mapping.fraction(100.0), 1.0);
		assert!((mapping.value(0.5) - 51.0).abs() < 0.001);
	}

	#[test]
	fn warmth_runs_warm_on_the_left() {
		let mapping = SliderKind::Warmth.mapping();
		assert_eq!(mapping.fraction(100.0), 0.0);
		assert_eq!(mapping.fraction(0.0), 1.0);
		assert!((mapping.value(0.0) - 100.0).abs() < 0.001);
		assert!((mapping.value(1.0) - 0.0).abs() < 0.001);
	}

	#[test]
	fn plain_maps_its_own_range() {
		let mapping = SliderKind::Plain {
			min: 0.0,
			max: 100.0,
		}
		.mapping();
		assert_eq!(mapping.fraction(0.0), 0.0);
		assert_eq!(mapping.fraction(100.0), 1.0);
	}
}
