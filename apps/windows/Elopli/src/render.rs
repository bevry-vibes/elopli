use std::collections::HashMap;
use std::ptr::null_mut;

use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Gdi::{
	BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, HBITMAP, HDC,
	HGDIOBJ, SRCCOPY, SelectObject,
};
use windows::Win32::Graphics::GdiPlus::{
	CombineModeReplace, FillModeWinding, FontStyleBold, FontStyleRegular, GdipAddPathArc,
	GdipAddPathBezier, GdipAddPathLine, GdipClosePathFigure, GdipCreateFont,
	GdipCreateFontFamilyFromName, GdipCreateFromHDC, GdipCreatePath, GdipCreatePen1,
	GdipCreateSolidFill, GdipCreateStringFormat, GdipDeleteBrush, GdipDeleteFont,
	GdipDeleteFontFamily, GdipDeleteGraphics, GdipDeletePath, GdipDeletePen,
	GdipDeleteStringFormat, GdipDrawEllipse, GdipDrawLine, GdipDrawPath, GdipDrawString,
	GdipFillEllipse, GdipFillPath, GdipFillRectangle, GdipGraphicsClear, GdipMeasureString,
	GdipResetClip, GdipSetClipPath, GdipSetClipRect, GdipSetPenLineJoin, GdipSetPixelOffsetMode,
	GdipSetSmoothingMode, GdipSetStringFormatAlign, GdipSetStringFormatFlags,
	GdipSetStringFormatLineAlign, GdipSetTextRenderingHint, GdiplusShutdown, GdiplusStartup,
	GdiplusStartupInput, GpBrush, GpFont, GpFontFamily, GpGraphics, GpPath, GpPen, GpStringFormat,
	LineJoinRound, Ok as GpOk, PixelOffsetModeHighQuality, RectF, SmoothingModeAntiAlias,
	StringAlignment, StringAlignmentCenter, StringAlignmentFar, StringAlignmentNear,
	StringFormatFlagsNoWrap, TextRenderingHintClearTypeGridFit, UnitPixel,
};
use windows::Win32::UI::WindowsAndMessaging::GetClientRect;
use windows::core::PCWSTR;

use crate::icons::{Command, Icon, commands};
use crate::theme::{Theme, rgba};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextAlign {
	Left,
	Center,
	Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct TextKey {
	size: u16,
	weight: u16,
	align: TextAlign,
	center_vertical: bool,
	wrap: bool,
}

impl TextKey {
	fn new(size: f32, weight: u16, align: TextAlign, center_vertical: bool, wrap: bool) -> Self {
		Self {
			size: (size * 4.0).round() as u16,
			weight,
			align,
			center_vertical,
			wrap,
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct PenKey {
	color: u32,
	width: u16,
}

pub struct Renderer {
	token: usize,
	graphics: *mut GpGraphics,
	dc: HDC,
	memory_dc: HDC,
	bitmap: HBITMAP,
	old_bitmap: HGDIOBJ,
	width: i32,
	height: i32,
	family: *mut GpFontFamily,
	fonts: HashMap<TextKey, *mut GpFont>,
	formats: HashMap<TextKey, *mut GpStringFormat>,
	brushes: HashMap<u32, *mut GpBrush>,
	pens: HashMap<PenKey, *mut GpPen>,
	scale: f32,
}

impl Renderer {
	pub fn new(_window: HWND, _width: u32, _height: u32, dpi: f32) -> windows::core::Result<Self> {
		let input = GdiplusStartupInput {
			GdiplusVersion: 1,
			..Default::default()
		};
		let mut token = 0usize;
		let status = unsafe { GdiplusStartup(&mut token, &input, null_mut()) };
		if status != GpOk {
			return Err(windows::core::Error::new(
				windows::core::HRESULT(0x8000_4005u32 as i32),
				"GDI+ did not start",
			));
		}
		let mut family = null_mut();
		let name = crate::win32::wide("Segoe UI");
		let status =
			unsafe { GdipCreateFontFamilyFromName(PCWSTR(name.as_ptr()), null_mut(), &mut family) };
		if status != GpOk {
			unsafe {
				GdiplusShutdown(token);
			}
			return Err(windows::core::Error::new(
				windows::core::HRESULT(0x8000_4005u32 as i32),
				"the Segoe UI font family is unavailable",
			));
		}
		Ok(Self {
			token,
			graphics: null_mut(),
			dc: HDC::default(),
			memory_dc: HDC::default(),
			bitmap: HBITMAP::default(),
			old_bitmap: HGDIOBJ::default(),
			width: 0,
			height: 0,
			family,
			fonts: HashMap::new(),
			formats: HashMap::new(),
			brushes: HashMap::new(),
			pens: HashMap::new(),
			scale: (dpi / 96.0).max(1.0),
		})
	}

	pub fn resize(
		&mut self,
		_window: HWND,
		_width: u32,
		_height: u32,
		dpi: f32,
	) -> windows::core::Result<()> {
		self.scale = (dpi / 96.0).max(1.0);
		Ok(())
	}

	pub fn begin(&mut self, window: HWND, dc: HDC, background: u32) {
		self.dc = dc;
		if dc.0.is_null() {
			return;
		}
		let mut rect = RECT::default();
		unsafe {
			let _ = GetClientRect(window, &mut rect);
		}
		self.width = (rect.right - rect.left).max(1);
		self.height = (rect.bottom - rect.top).max(1);
		self.memory_dc = unsafe { CreateCompatibleDC(Some(dc)) };
		if self.memory_dc.0.is_null() {
			return;
		}
		self.bitmap = unsafe { CreateCompatibleBitmap(dc, self.width, self.height) };
		if self.bitmap.0.is_null() {
			self.cleanup_buffer();
			return;
		}
		self.old_bitmap = unsafe { SelectObject(self.memory_dc, HGDIOBJ(self.bitmap.0)) };
		let mut graphics = null_mut();
		let status = unsafe { GdipCreateFromHDC(self.memory_dc, &mut graphics) };
		if status != GpOk {
			self.cleanup_buffer();
			return;
		}
		self.graphics = graphics;
		unsafe {
			let _ = GdipSetSmoothingMode(graphics, SmoothingModeAntiAlias);
			let _ = GdipSetTextRenderingHint(graphics, TextRenderingHintClearTypeGridFit);
			let _ = GdipSetPixelOffsetMode(graphics, PixelOffsetModeHighQuality);
			let _ = GdipGraphicsClear(graphics, background);
		}
	}

	pub fn end(&mut self) {
		if !self.graphics.is_null() {
			unsafe {
				let _ = GdipDeleteGraphics(self.graphics);
			}
			self.graphics = null_mut();
		}
		if !self.dc.0.is_null() && !self.memory_dc.0.is_null() && self.width > 0 && self.height > 0
		{
			unsafe {
				let _ = BitBlt(
					self.dc,
					0,
					0,
					self.width,
					self.height,
					Some(self.memory_dc),
					0,
					0,
					SRCCOPY,
				);
			}
		}
		self.cleanup_buffer();
		self.dc = HDC::default();
	}

	fn cleanup_buffer(&mut self) {
		if !self.memory_dc.0.is_null() {
			if !self.bitmap.0.is_null() {
				unsafe {
					let _ = SelectObject(self.memory_dc, self.old_bitmap);
					let _ = DeleteObject(HGDIOBJ(self.bitmap.0));
				}
				self.bitmap = HBITMAP::default();
			}
			unsafe {
				let _ = DeleteDC(self.memory_dc);
			}
			self.memory_dc = HDC::default();
		}
		self.old_bitmap = HGDIOBJ::default();
		self.width = 0;
		self.height = 0;
	}

	fn graphics(&self) -> Option<*mut GpGraphics> {
		if self.graphics.is_null() {
			None
		} else {
			Some(self.graphics)
		}
	}

	fn brush(&mut self, color: u32) -> Option<*mut GpBrush> {
		if let Some(brush) = self.brushes.get(&color) {
			return Some(*brush);
		}
		let mut brush = null_mut();
		let status = unsafe { GdipCreateSolidFill(color, &mut brush) };
		if status != GpOk {
			return None;
		}
		let brush = brush as *mut GpBrush;
		self.brushes.insert(color, brush);
		Some(brush)
	}

	fn pen(&mut self, color: u32, width: f32) -> Option<*mut GpPen> {
		let key = PenKey {
			color,
			width: (width * 100.0).round() as u16,
		};
		if let Some(pen) = self.pens.get(&key) {
			return Some(*pen);
		}
		let mut pen = null_mut();
		let status =
			unsafe { GdipCreatePen1(color, (width * self.scale).max(1.0), UnitPixel, &mut pen) };
		if status != GpOk {
			return None;
		}
		unsafe {
			let _ = GdipSetPenLineJoin(pen, LineJoinRound);
		}
		self.pens.insert(key, pen);
		Some(pen)
	}

	fn font(&mut self, size: f32, weight: u16) -> Option<*mut GpFont> {
		let key = TextKey::new(size, weight, TextAlign::Left, false, false);
		if let Some(font) = self.fonts.get(&key) {
			return Some(*font);
		}
		let mut font = null_mut();
		let style = if weight >= 600 {
			FontStyleBold.0
		} else {
			FontStyleRegular.0
		};
		let status =
			unsafe { GdipCreateFont(self.family, size * self.scale, style, UnitPixel, &mut font) };
		if status != GpOk {
			return None;
		}
		self.fonts.insert(key, font);
		Some(font)
	}

	fn format(
		&mut self,
		size: f32,
		weight: u16,
		align: TextAlign,
		center_vertical: bool,
		wrap: bool,
	) -> Option<*mut GpStringFormat> {
		let key = TextKey::new(size, weight, align, center_vertical, wrap);
		if let Some(format) = self.formats.get(&key) {
			return Some(*format);
		}
		let mut format = null_mut();
		let status = unsafe { GdipCreateStringFormat(0, 0, &mut format) };
		if status != GpOk {
			return None;
		}
		let alignment: StringAlignment = match align {
			TextAlign::Left => StringAlignmentNear,
			TextAlign::Center => StringAlignmentCenter,
			TextAlign::Right => StringAlignmentFar,
		};
		unsafe {
			let _ = GdipSetStringFormatAlign(format, alignment);
			let _ = GdipSetStringFormatLineAlign(
				format,
				if center_vertical {
					StringAlignmentCenter
				} else {
					StringAlignmentNear
				},
			);
			if !wrap {
				let _ = GdipSetStringFormatFlags(format, StringFormatFlagsNoWrap.0);
			}
		}
		self.formats.insert(key, format);
		Some(format)
	}

	fn rect(&self, rect: Rect) -> RectF {
		RectF {
			X: rect.x * self.scale,
			Y: rect.y * self.scale,
			Width: rect.w * self.scale,
			Height: rect.h * self.scale,
		}
	}

	pub fn text(
		&mut self,
		text: &str,
		rect: Rect,
		size: f32,
		weight: u16,
		align: TextAlign,
		color: u32,
	) {
		self.text_ex(text, rect, size, weight, align, false, false, color);
	}

	#[allow(clippy::too_many_arguments)]
	pub fn text_ex(
		&mut self,
		text: &str,
		rect: Rect,
		size: f32,
		weight: u16,
		align: TextAlign,
		center_vertical: bool,
		wrap: bool,
		color: u32,
	) {
		let (Some(graphics), Some(font), Some(format), Some(brush)) = (
			self.graphics(),
			self.font(size, weight),
			self.format(size, weight, align, center_vertical, wrap),
			self.brush(color),
		) else {
			return;
		};
		let units = text.encode_utf16().collect::<Vec<_>>();
		let layout = self.rect(rect);
		unsafe {
			let _ = GdipDrawString(
				graphics,
				PCWSTR(units.as_ptr()),
				units.len() as i32,
				font,
				&layout,
				format,
				brush,
			);
		}
	}

	pub fn measure(&mut self, text: &str, size: f32, weight: u16, max_width: f32) -> (f32, f32) {
		let (Some(graphics), Some(font), Some(format)) = (
			self.graphics(),
			self.font(size, weight),
			self.format(size, weight, TextAlign::Left, false, false),
		) else {
			return (0.0, size);
		};
		let units = text.encode_utf16().collect::<Vec<_>>();
		let layout = RectF {
			X: 0.0,
			Y: 0.0,
			Width: max_width * self.scale,
			Height: 10_000.0,
		};
		let mut bounds = RectF::default();
		let status = unsafe {
			GdipMeasureString(
				graphics,
				PCWSTR(units.as_ptr()),
				units.len() as i32,
				font,
				&layout,
				format,
				&mut bounds,
				null_mut(),
				null_mut(),
			)
		};
		if status == GpOk {
			(bounds.Width / self.scale, bounds.Height / self.scale)
		} else {
			(0.0, size)
		}
	}

	pub fn fill_rect(&mut self, rect: Rect, color: u32) {
		let (Some(graphics), Some(brush)) = (self.graphics(), self.brush(color)) else {
			return;
		};
		let rect = self.rect(rect);
		unsafe {
			let _ = GdipFillRectangle(graphics, brush, rect.X, rect.Y, rect.Width, rect.Height);
		}
	}

	pub fn fill_round(&mut self, rect: Rect, radius: f32, color: u32) {
		let (Some(graphics), Some(brush)) = (self.graphics(), self.brush(color)) else {
			return;
		};
		if let Some(path) = self.rounded_path(rect, radius) {
			unsafe {
				let _ = GdipFillPath(graphics, brush, path);
				let _ = GdipDeletePath(path);
			}
		}
	}

	pub fn stroke_round(&mut self, rect: Rect, radius: f32, color: u32, width: f32) {
		let (Some(graphics), Some(pen)) = (self.graphics(), self.pen(color, width)) else {
			return;
		};
		if let Some(path) = self.rounded_path(rect, radius) {
			unsafe {
				let _ = GdipDrawPath(graphics, pen, path);
				let _ = GdipDeletePath(path);
			}
		}
	}

	fn rounded_path(&self, rect: Rect, radius: f32) -> Option<*mut GpPath> {
		let mut path = null_mut();
		let status = unsafe { GdipCreatePath(FillModeWinding, &mut path) };
		if status != GpOk {
			return None;
		}
		let x = rect.x * self.scale;
		let y = rect.y * self.scale;
		let w = rect.w * self.scale;
		let h = rect.h * self.scale;
		let r = (radius * self.scale).min(w / 2.0).min(h / 2.0);
		unsafe {
			let _ = GdipAddPathArc(path, x, y, r * 2.0, r * 2.0, 180.0, 90.0);
			let _ = GdipAddPathLine(path, x + r, y, x + w - r, y);
			let _ = GdipAddPathArc(path, x + w - r * 2.0, y, r * 2.0, r * 2.0, 270.0, 90.0);
			let _ = GdipAddPathLine(path, x + w, y + r, x + w, y + h - r);
			let _ = GdipAddPathArc(
				path,
				x + w - r * 2.0,
				y + h - r * 2.0,
				r * 2.0,
				r * 2.0,
				0.0,
				90.0,
			);
			let _ = GdipAddPathLine(path, x + w - r, y + h, x + r, y + h);
			let _ = GdipAddPathArc(path, x, y + h - r * 2.0, r * 2.0, r * 2.0, 90.0, 90.0);
			let _ = GdipAddPathLine(path, x, y + h - r, x, y + r);
			let _ = GdipClosePathFigure(path);
		}
		Some(path)
	}

	pub fn line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, color: u32, width: f32) {
		let (Some(graphics), Some(pen)) = (self.graphics(), self.pen(color, width)) else {
			return;
		};
		unsafe {
			let _ = GdipDrawLine(
				graphics,
				pen,
				x1 * self.scale,
				y1 * self.scale,
				x2 * self.scale,
				y2 * self.scale,
			);
		}
	}

	pub fn circle(&mut self, cx: f32, cy: f32, radius: f32, color: u32) {
		let (Some(graphics), Some(brush)) = (self.graphics(), self.brush(color)) else {
			return;
		};
		let r = radius * self.scale;
		unsafe {
			let _ = GdipFillEllipse(
				graphics,
				brush,
				cx * self.scale - r,
				cy * self.scale - r,
				r * 2.0,
				r * 2.0,
			);
		}
	}

	pub fn ring(&mut self, cx: f32, cy: f32, radius: f32, color: u32, width: f32) {
		let (Some(graphics), Some(pen)) = (self.graphics(), self.pen(color, width)) else {
			return;
		};
		let r = radius * self.scale;
		unsafe {
			let _ = GdipDrawEllipse(
				graphics,
				pen,
				cx * self.scale - r,
				cy * self.scale - r,
				r * 2.0,
				r * 2.0,
			);
		}
	}

	pub fn gradient_rect(&mut self, rect: Rect, stops: &[(f32, u32)], horizontal: bool) {
		if stops.is_empty() {
			return;
		}
		let slices = ((rect.w * self.scale / 2.0).ceil() as i32).clamp(2, 256);
		let width = rect.w / slices as f32 + 0.5;
		for index in 0..slices {
			let t = index as f32 / (slices - 1) as f32;
			let color = sample_gradient(stops, t);
			let x = if horizontal {
				rect.x + rect.w * t
			} else {
				rect.x
			};
			let y = if horizontal {
				rect.y
			} else {
				rect.y + rect.h * t
			};
			let (w, h) = if horizontal {
				(width, rect.h)
			} else {
				(rect.w, width)
			};
			self.fill_rect(Rect::new(x, y, w, h), color);
		}
	}

	pub fn gradient_round(
		&mut self,
		rect: Rect,
		radius: f32,
		stops: &[(f32, u32)],
		horizontal: bool,
	) {
		let Some(graphics) = self.graphics() else {
			return;
		};
		let Some(path) = self.rounded_path(rect, radius) else {
			return;
		};
		unsafe {
			let _ = GdipSetClipPath(graphics, path, CombineModeReplace);
		}
		self.gradient_rect(rect, stops, horizontal);
		unsafe {
			let _ = GdipResetClip(graphics);
			let _ = GdipDeletePath(path);
		}
	}

	pub fn push_clip(&mut self, rect: Rect) {
		let Some(graphics) = self.graphics() else {
			return;
		};
		let rect = self.rect(rect);
		unsafe {
			let _ = GdipSetClipRect(
				graphics,
				rect.X,
				rect.Y,
				rect.Width,
				rect.Height,
				CombineModeReplace,
			);
		}
	}

	pub fn pop_clip(&mut self) {
		if let Some(graphics) = self.graphics() {
			unsafe {
				let _ = GdipResetClip(graphics);
			}
		}
	}

	pub fn icon(&mut self, icon: Icon, cx: f32, cy: f32, size: f32, color: u32) {
		let (Some(graphics), Some(brush)) = (self.graphics(), self.brush(color)) else {
			return;
		};
		let mut path = null_mut();
		if unsafe { GdipCreatePath(FillModeWinding, &mut path) } != GpOk {
			return;
		}
		let scale = size / 20.0;
		let dpi = self.scale;
		let x = |value: f32| (cx + (value - 10.0) * scale) * dpi;
		let y = |value: f32| (cy + (value - 10.0) * scale) * dpi;
		let mut current = (0.0f32, 0.0f32);
		let mut start = (0.0f32, 0.0f32);
		let mut open = false;
		for command in commands(icon) {
			match *command {
				Command::Move(x0, y0) => {
					if open {
						unsafe {
							let _ = GdipClosePathFigure(path);
						}
					}
					current = (x0, y0);
					start = current;
					open = true;
				}
				Command::Line(x0, y0) => {
					if open {
						unsafe {
							let _ = GdipAddPathLine(path, x(current.0), y(current.1), x(x0), y(y0));
						}
					}
					current = (x0, y0);
				}
				Command::Horizontal(x0) => {
					if open {
						unsafe {
							let _ = GdipAddPathLine(
								path,
								x(current.0),
								y(current.1),
								x(x0),
								y(current.1),
							);
						}
					}
					current.0 = x0;
				}
				Command::Vertical(y0) => {
					if open {
						unsafe {
							let _ = GdipAddPathLine(
								path,
								x(current.0),
								y(current.1),
								x(current.0),
								y(y0),
							);
						}
					}
					current.1 = y0;
				}
				Command::Curve(x1, y1, x2, y2, x0, y0) => {
					if open {
						unsafe {
							let _ = GdipAddPathBezier(
								path,
								x(current.0),
								y(current.1),
								x(x1),
								y(y1),
								x(x2),
								y(y2),
								x(x0),
								y(y0),
							);
						}
					}
					current = (x0, y0);
				}
				Command::Close => {
					if open {
						unsafe {
							let _ = GdipClosePathFigure(path);
						}
					}
					open = false;
					current = start;
				}
			}
		}
		if open {
			unsafe {
				let _ = GdipClosePathFigure(path);
			}
		}
		unsafe {
			let _ = GdipFillPath(graphics, brush, path);
			let _ = GdipDeletePath(path);
		}
	}

	pub fn slash(&mut self, cx: f32, cy: f32, size: f32, color: u32, background: u32) {
		let extent = size * 0.4;
		self.line(
			cx - extent,
			cy + extent,
			cx + extent,
			cy - extent,
			background,
			2.6,
		);
		self.line(
			cx - extent,
			cy + extent,
			cx + extent,
			cy - extent,
			color,
			1.2,
		);
	}

	/// A capsule that tapers from a thin left edge to a thick right edge,
	/// matching the macOS brightness slider.
	pub fn tapered_track(
		&mut self,
		track: Rect,
		left_height: f32,
		right_height: f32,
		fill: u32,
		border: u32,
	) {
		let (Some(graphics), Some(brush)) = (self.graphics(), self.brush(fill)) else {
			return;
		};
		let mut path = null_mut();
		if unsafe { GdipCreatePath(FillModeWinding, &mut path) } != GpOk {
			return;
		}
		let center = track.y + track.h / 2.0;
		let left = left_height.min(track.h).max(1.0);
		let right = right_height.min(track.h).max(1.0);
		let left_radius = left / 2.0;
		let right_radius = right / 2.0;
		let left_x = track.x + left_radius;
		let right_x = track.right() - right_radius;
		let scale = self.scale;
		unsafe {
			let _ = GdipAddPathLine(
				path,
				left_x * scale,
				(center - left_radius) * scale,
				right_x * scale,
				(center - right_radius) * scale,
			);
			let _ = GdipAddPathArc(
				path,
				(right_x - right_radius) * scale,
				(center - right_radius) * scale,
				right_radius * 2.0 * scale,
				right_radius * 2.0 * scale,
				-90.0,
				180.0,
			);
			let _ = GdipAddPathLine(
				path,
				right_x * scale,
				(center + right_radius) * scale,
				left_x * scale,
				(center + left_radius) * scale,
			);
			let _ = GdipAddPathArc(
				path,
				(left_x - left_radius) * scale,
				(center - left_radius) * scale,
				left_radius * 2.0 * scale,
				left_radius * 2.0 * scale,
				90.0,
				180.0,
			);
			let _ = GdipClosePathFigure(path);
			let _ = GdipFillPath(graphics, brush, path);
			if let Some(pen) = self.pen(border, 1.0) {
				let _ = GdipDrawPath(graphics, pen, path);
			}
			let _ = GdipDeletePath(path);
		}
	}

	pub fn toggle(&mut self, rect: Rect, on: bool, theme: &Theme) {
		let track = if on { theme.accent } else { theme.toggle_off };
		self.fill_round(rect, rect.h / 2.0, track);
		let radius = rect.h / 2.0 - 3.0;
		let cx = if on {
			rect.x + rect.w - radius - 3.0
		} else {
			rect.x + radius + 3.0
		};
		self.circle(cx, rect.y + rect.h / 2.0, radius, theme.knob);
	}

	pub fn shadow_round(&mut self, rect: Rect, radius: f32) {
		for index in 0..4 {
			let spread = index as f32 * 1.5;
			let alpha = 10 - index * 2;
			self.stroke_round(
				Rect {
					x: rect.x - spread,
					y: rect.y - spread,
					w: rect.w + spread * 2.0,
					h: rect.h + spread * 2.0,
				},
				radius + spread,
				rgba(0, 0, 0, alpha),
				1.0,
			);
		}
	}
}

impl Drop for Renderer {
	fn drop(&mut self) {
		self.end();
		for (_, font) in self.fonts.drain() {
			unsafe {
				let _ = GdipDeleteFont(font);
			}
		}
		for (_, format) in self.formats.drain() {
			unsafe {
				let _ = GdipDeleteStringFormat(format);
			}
		}
		for (_, brush) in self.brushes.drain() {
			unsafe {
				let _ = GdipDeleteBrush(brush);
			}
		}
		for (_, pen) in self.pens.drain() {
			unsafe {
				let _ = GdipDeletePen(pen);
			}
		}
		if !self.family.is_null() {
			unsafe {
				let _ = GdipDeleteFontFamily(self.family);
			}
			self.family = null_mut();
		}
		if self.token != 0 {
			unsafe {
				GdiplusShutdown(self.token);
			}
			self.token = 0;
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
	pub x: f32,
	pub y: f32,
	pub w: f32,
	pub h: f32,
}

impl Rect {
	pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
		Self { x, y, w, h }
	}

	pub fn right(self) -> f32 {
		self.x + self.w
	}

	pub fn bottom(self) -> f32 {
		self.y + self.h
	}

	pub fn inset(self, value: f32) -> Self {
		Self {
			x: self.x + value,
			y: self.y + value,
			w: (self.w - value * 2.0).max(0.0),
			h: (self.h - value * 2.0).max(0.0),
		}
	}

	pub fn contains(self, x: f32, y: f32) -> bool {
		x >= self.x && x <= self.right() && y >= self.y && y <= self.bottom()
	}
}

fn sample_gradient(stops: &[(f32, u32)], t: f32) -> u32 {
	let mut previous = stops[0];
	for &(position, color) in stops {
		if t <= position {
			let span = (position - previous.0).max(0.0001);
			let factor = ((t - previous.0) / span).clamp(0.0, 1.0);
			return lerp_color(previous.1, color, factor);
		}
		previous = (position, color);
	}
	previous.1
}

fn lerp_color(from: u32, to: u32, factor: f32) -> u32 {
	let channel = |shift: u32| {
		let a = ((from >> shift) & 0xFF) as f32;
		let b = ((to >> shift) & 0xFF) as f32;
		((a + (b - a) * factor).round() as u32) & 0xFF
	};
	(channel(24) << 24) | (channel(16) << 16) | (channel(8) << 8) | channel(0)
}

pub fn track_color(value: u8) -> u32 {
	let t = (value as f32 - 2.0) / 98.0;
	let red = (255.0 * (0.25 + 0.75 * t)) as u8;
	let green = (255.0 * (0.25 + 0.75 * t)) as u8;
	let blue = (255.0 * (0.3 + 0.7 * t)) as u8;
	crate::theme::rgb(red, green, blue)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn gradient_interpolates_between_stops() {
		let stops = [(0.0, 0xFFFF0000), (1.0, 0xFF0000FF)];
		assert_eq!(sample_gradient(&stops, 0.0), 0xFFFF0000);
		assert_eq!(sample_gradient(&stops, 1.0), 0xFF0000FF);
		let middle = sample_gradient(&stops, 0.5);
		assert!((middle >> 16 & 0xFF) > 100 && (middle & 0xFF) > 100);
	}

	#[test]
	fn rect_contains_its_edges() {
		let rect = Rect::new(10.0, 20.0, 30.0, 40.0);
		assert!(rect.contains(10.0, 20.0));
		assert!(rect.contains(40.0, 60.0));
		assert!(!rect.contains(9.9, 20.0));
		assert_eq!(rect.inset(2.0), Rect::new(12.0, 22.0, 26.0, 36.0));
	}

	#[test]
	fn brighter_values_produce_a_brighter_track() {
		let low = track_color(2);
		let high = track_color(100);
		assert!(high >> 16 & 0xFF > low >> 16 & 0xFF);
		assert!(high & 0xFF > low & 0xFF);
	}
}
