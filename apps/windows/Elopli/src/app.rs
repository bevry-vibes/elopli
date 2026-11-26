use std::collections::{BTreeSet, HashMap};
use std::time::Instant;

use elopli_core::{PowerOnBehavior, PulseConfig, SweepConfig, parse_duration, warmth_to_mired};
use elopli_session::{
	DiscoveryRow, DiscoveryState, LightDevice, LinkGraph, LinkKind, LinkTarget, SessionSnapshot,
};
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::{
	CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, ClientToScreen, CreateFontW, CreateSolidBrush,
	DEFAULT_CHARSET, DEFAULT_PITCH, DeleteObject, FF_DONTCARE, FW_NORMAL, HBRUSH, HDC, HFONT,
	HGDIOBJ, InvalidateRect, OUT_DEFAULT_PRECIS, SetBkColor, SetTextColor, ValidateRect,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::EM_SETSEL;
use windows::Win32::UI::Input::KeyboardAndMouse::{
	ReleaseCapture, SetCapture, SetFocus, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent, VK_CONTROL,
	VK_ESCAPE, VK_RETURN,
};
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
	AppendMenuW, CS_DROPSHADOW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu,
	DestroyWindow, DispatchMessageW, ES_AUTOHSCROLL, ES_LEFT, GWLP_USERDATA, GetMessageW,
	GetParent, GetWindowLongPtrW, GetWindowTextLengthW, GetWindowTextW, HMENU, IDC_ARROW,
	IsWindowVisible, LoadCursorW, MENU_ITEM_FLAGS, MF_CHECKED, MF_SEPARATOR, MF_STRING, MSG,
	PostQuitMessage, RegisterClassW, RegisterWindowMessageW, SW_HIDE, SW_SHOW, SWP_NOACTIVATE,
	SWP_NOMOVE, SWP_NOZORDER, SendMessageW, SetForegroundWindow, SetTimer, SetWindowLongPtrW,
	SetWindowPos, ShowWindow, TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu, TranslateMessage,
	WA_INACTIVE, WINDOW_STYLE, WM_ACTIVATE, WM_CHAR, WM_CLOSE, WM_COMMAND, WM_CONTEXTMENU,
	WM_CTLCOLOREDIT, WM_DESTROY, WM_DPICHANGED, WM_ENDSESSION, WM_KEYDOWN, WM_KEYUP, WM_KILLFOCUS,
	WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_NCCREATE, WM_PAINT,
	WM_QUERYENDSESSION, WM_RBUTTONUP, WM_SETFONT, WM_SETTINGCHANGE, WM_TIMER, WNDCLASSW, WS_CHILD,
	WS_CLIPCHILDREN, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
};
use windows::core::PCWSTR;

use crate::render::{Rect, Renderer, TextAlign, track_color};
use crate::session_worker::{Command, Worker};
use crate::theme::{Theme, rgba};
use crate::tray::Tray;
use crate::win32::{
	WM_APP_SHOW, WM_APP_TRAY, WM_APP_UPDATE, apply_dark_mode, client_size, ctrl_down, cursor_hand,
	cursor_position, dpi, hiword, loword, reveal_config, scale_i32, tray_rect, unscale, wide,
	work_area,
};

pub const WINDOW_CLASS: &str = "ElopliPanel";
const PANEL_WIDTH: f32 = 460.0;
const MIN_HEIGHT: i32 = 180;
const MAX_HEIGHT: f32 = 640.0;
const TIMER_REFRESH: usize = 1;
const TIMER_UI: usize = 2;
const MENU_SHOW: usize = 2000;
const MENU_REFRESH: usize = 1001;
const MENU_ALL_ON: usize = 1002;
const MENU_ALL_OFF: usize = 1003;
const MENU_DISCOVER: usize = 1004;
const MENU_AUTOSTART: usize = 1005;
const MENU_REVEAL: usize = 1006;
const MENU_QUIT: usize = 1007;
const CARD_RENAME: usize = 3001;
const CARD_POWER_ON: usize = 3002;
const CARD_PULSE: usize = 3003;
const CARD_PULSE_GROUP: usize = 3004;
const CARD_SWEEP: usize = 3005;
const CARD_SWEEP_GROUP: usize = 3006;
const CARD_IDENTIFY: usize = 3007;
/// Not exposed by windows-rs 0.62.
const WM_MOUSELEAVE: u32 = 0x02A3;

pub fn run(autostart: bool) -> windows::core::Result<()> {
	let instance = unsafe { GetModuleHandleW(None)? };
	let class_name = wide(WINDOW_CLASS);
	let cursor = unsafe { LoadCursorW(None, IDC_ARROW)? };
	let class = WNDCLASSW {
		style: CS_DROPSHADOW,
		lpfnWndProc: Some(window_proc),
		hInstance: HINSTANCE(instance.0),
		lpszClassName: PCWSTR(class_name.as_ptr()),
		hCursor: cursor,
		..Default::default()
	};
	unsafe {
		if RegisterClassW(&class) == 0 {
			return Err(windows::core::Error::new(
				windows::core::HRESULT(0x8000_4005u32 as i32),
				"RegisterClassW failed",
			));
		}
	}
	let title = wide("Elopli");
	let window = unsafe {
		CreateWindowExW(
			WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
			PCWSTR(class_name.as_ptr()),
			PCWSTR(title.as_ptr()),
			WS_POPUP | WS_CLIPCHILDREN,
			0,
			0,
			scale_i32(96, PANEL_WIDTH),
			MIN_HEIGHT,
			None,
			None,
			Some(HINSTANCE(instance.0)),
			None,
		)?
	};
	let app = Box::new(App::new(window)?);
	unsafe {
		SetWindowLongPtrW(window, GWLP_USERDATA, Box::into_raw(app) as isize);
	}
	if !autostart && let Some(app) = app_mut(window) {
		app.show();
	}
	let mut message = MSG::default();
	unsafe {
		while GetMessageW(&mut message, None, 0, 0).as_bool() {
			let _ = TranslateMessage(&message);
			DispatchMessageW(&message);
		}
		let raw = GetWindowLongPtrW(window, GWLP_USERDATA);
		if raw != 0 {
			SetWindowLongPtrW(window, GWLP_USERDATA, 0);
			drop(Box::from_raw(raw as *mut App));
		}
	}
	Ok(())
}

fn app_mut(window: HWND) -> Option<&'static mut App> {
	unsafe {
		let raw = GetWindowLongPtrW(window, GWLP_USERDATA);
		if raw == 0 {
			None
		} else {
			Some(&mut *(raw as *mut App))
		}
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Hit {
	Refresh,
	Scan,
	AllOn,
	AllOff,
	Settings,
	Card(String),
	Power(String),
	Brightness(String),
	Warmth(String),
	Menu(String),
	Wifi(String),
	SheetClose,
	SheetPrimary,
	SheetSecondary,
	SheetScanAgain,
	SheetCountDown,
	SheetCountUp,
	SheetRadio(u8),
	SheetLow,
	SheetPeak,
	SheetBrightness,
	SheetWarmth,
	DiscoveryAction(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Sheet {
	None,
	Discovery,
	Rename {
		name: String,
		value: String,
	},
	PowerOn {
		name: String,
		behavior: u8,
		brightness: u8,
		warmth: u8,
	},
	Pulse {
		names: Vec<String>,
		count: u32,
		duration: String,
		peak: u8,
		low: u8,
		started: bool,
		stopping: bool,
		message: Option<String>,
	},
	Sweep {
		names: Vec<String>,
		count: u32,
		duration: String,
		brightness: u8,
		started: bool,
		stopping: bool,
		message: Option<String>,
	},
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EditKind {
	Rename,
	PulseDuration,
	SweepDuration,
}

struct Edit {
	window: HWND,
	kind: EditKind,
	font: HFONT,
	brush: HBRUSH,
}

impl Drop for Edit {
	fn drop(&mut self) {
		unsafe {
			let _ = DestroyWindow(self.window);
			let _ = DeleteObject(HGDIOBJ(self.font.0));
			let _ = DeleteObject(HGDIOBJ(self.brush.0));
		}
	}
}

enum Drag {
	Slider {
		name: String,
		kind: LinkKind,
	},
	Card {
		name: String,
		start_x: f32,
		start_y: f32,
		offset_x: f32,
		offset_y: f32,
		active: bool,
	},
	SheetLow,
	SheetPeak,
	SheetBrightness,
	SheetWarmth,
}

struct App {
	window: HWND,
	worker: Worker,
	renderer: Renderer,
	theme: Theme,
	snapshot: SessionSnapshot,
	tray: Tray,
	autostart: bool,
	dpi: u32,
	height: i32,
	hover: Option<Hit>,
	pressed: Option<Hit>,
	drag: Option<Drag>,
	drag_target: Option<String>,
	scroll: f32,
	sheet: Sheet,
	edit: Option<Edit>,
	preview: HashMap<(String, LinkKind), u8>,
	preview_until: Option<Instant>,
	wifi_hover: Option<(String, Instant)>,
	wifi_popover: Option<String>,
	ctrl_down: bool,
	menu_open: bool,
	last_click: Option<(String, Instant)>,
	quitting: bool,
	no_hide: bool,
	taskbar_created: u32,
}

struct CardLayout {
	name: String,
	card: Rect,
	power: Rect,
	brightness: Rect,
	warmth: Rect,
	menu: Rect,
	wifi: Rect,
}

struct Layout {
	header: Rect,
	buttons: Vec<(Rect, Hit)>,
	cards: Vec<CardLayout>,
	footer: Rect,
	content_height: f32,
}

struct SheetRects {
	panel: Rect,
	title: Rect,
	description: Rect,
	close: Rect,
	primary: Rect,
	secondary: Rect,
	count: Rect,
	count_down: Rect,
	count_up: Rect,
	edit: Rect,
	slider: Rect,
	radios: [(Rect, u8); 2],
	rows: Vec<(Rect, Rect)>,
	scan_again: Rect,
	message: Rect,
}

impl App {
	fn new(window: HWND) -> windows::core::Result<Self> {
		let dpi = dpi(window);
		let (width, height) = client_size(window);
		let theme = Theme::detect();
		apply_dark_mode(window, theme.dark);
		let worker = Worker::start(window).map_err(|message| {
			windows::core::Error::new(windows::core::HRESULT(0x8000_4005u32 as i32), message)
		})?;
		let snapshot = worker.snapshot();
		let renderer = Renderer::new(
			window,
			width.max(1) as u32,
			height.max(1) as u32,
			dpi as f32,
		)?;
		let tray = Tray::add(window);
		let taskbar_message = wide("TaskbarCreated");
		let taskbar_created = unsafe { RegisterWindowMessageW(PCWSTR(taskbar_message.as_ptr())) };
		unsafe {
			SetTimer(Some(window), TIMER_REFRESH, 5_000, None);
			SetTimer(Some(window), TIMER_UI, 100, None);
		}
		let mut app = Self {
			window,
			worker,
			renderer,
			theme,
			snapshot,
			tray,
			autostart: crate::win32::autostart_enabled(),
			dpi,
			height,
			hover: None,
			pressed: None,
			drag: None,
			drag_target: None,
			scroll: 0.0,
			sheet: Sheet::None,
			edit: None,
			preview: HashMap::new(),
			preview_until: None,
			wifi_hover: None,
			wifi_popover: None,
			ctrl_down: false,
			menu_open: false,
			last_click: None,
			quitting: false,
			no_hide: std::env::var_os("ELOPLI_NO_HIDE").is_some(),
			taskbar_created,
		};
		app.resize_to_content(false);
		Ok(app)
	}

	fn refresh_snapshot(&mut self) {
		self.snapshot = self.worker.snapshot();
		let tooltip = if let Some(error) = &self.snapshot.error {
			format!("Elopli · {error}")
		} else {
			format!(
				"Elopli · {} light{}",
				self.snapshot.devices.len(),
				if self.snapshot.devices.len() == 1 {
					""
				} else {
					"s"
				}
			)
		};
		self.tray.set_tooltip(tooltip);
		self.clear_missing_preview();
	}

	fn show(&mut self) {
		self.refresh_snapshot();
		self.ctrl_down = false;
		self.resize_to_content(false);
		self.position();
		unsafe {
			let _ = ShowWindow(self.window, SW_SHOW);
			let _ = SetForegroundWindow(self.window);
		}
		self.worker.send(Command::Refresh);
		self.invalidate();
	}

	fn hide(&mut self) {
		unsafe {
			let _ = ShowWindow(self.window, SW_HIDE);
		}
		if self.ctrl_down {
			self.worker.send(Command::CommitLinks);
			self.ctrl_down = false;
		}
		self.hover = None;
		self.drag = None;
		self.drag_target = None;
		self.wifi_hover = None;
		self.wifi_popover = None;
		if self.sheet != Sheet::None {
			self.cancel_sheet_animation();
			self.close_sheet();
		}
	}

	fn toggle(&mut self) {
		let visible = unsafe { IsWindowVisible(self.window).as_bool() };
		if visible {
			self.hide();
		} else {
			self.show();
		}
	}

	fn invalidate(&self) {
		unsafe {
			let _ = InvalidateRect(Some(self.window), None, false);
		}
	}

	fn dip_size(&self) -> (f32, f32) {
		let (width, height) = client_size(self.window);
		(unscale(self.dpi, width), unscale(self.dpi, height))
	}

	fn position(&mut self) {
		let width = scale_i32(self.dpi, PANEL_WIDTH);
		let anchor = tray_rect(self.window)
			.map(|rect| POINT {
				x: rect.left,
				y: rect.top,
			})
			.unwrap_or_else(cursor_position);
		let work = work_area(anchor);
		let available_height = (work.bottom - work.top - 16).max(MIN_HEIGHT);
		if self.height > available_height {
			self.height = available_height;
		}
		let height = self.height;
		let mut x = anchor.x - width + 40;
		let mut y = anchor.y - height - 8;
		if y + height > work.bottom - 8 {
			y = work.bottom - height - 8;
		}
		if y < work.top + 8 {
			y = work.top + 8;
		}
		if x + width > work.right - 8 {
			x = work.right - width - 8;
		}
		if x < work.left + 8 {
			x = work.left + 8;
		}
		unsafe {
			let _ = SetWindowPos(self.window, None, x, y, width, height, SWP_NOACTIVATE);
		}
	}

	fn sheet_height(&self) -> f32 {
		match self.sheet {
			Sheet::None => 0.0,
			Sheet::Discovery => 520.0,
			Sheet::Rename { .. } => 300.0,
			Sheet::PowerOn { .. } => 380.0,
			Sheet::Pulse { .. } => 520.0,
			Sheet::Sweep { .. } => 480.0,
		}
	}

	fn resize_to_content(&mut self, reposition: bool) {
		let desired = if self.sheet == Sheet::None {
			let layout = self.layout();
			(layout.content_height + 20.0).ceil()
		} else {
			self.sheet_height()
		};
		let height =
			scale_i32(self.dpi, desired).clamp(MIN_HEIGHT, scale_i32(self.dpi, MAX_HEIGHT));
		if height == self.height {
			return;
		}
		self.height = height;
		if reposition {
			self.position();
		} else {
			unsafe {
				let _ = SetWindowPos(
					self.window,
					None,
					0,
					0,
					scale_i32(self.dpi, PANEL_WIDTH),
					height,
					SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
				);
			}
		}
		self.clamp_scroll();
		self.invalidate();
	}

	fn layout(&self) -> Layout {
		let width = PANEL_WIDTH;
		let padding = 18.0;
		let header = Rect::new(padding, padding, width - padding * 2.0, 36.0);
		let mut buttons = Vec::new();
		let mut right = header.right();
		let settings = Rect::new(right - 28.0, header.y + 4.0, 28.0, 28.0);
		buttons.push((settings, Hit::Settings));
		right -= 34.0;
		let all_off = Rect::new(right - 36.0, header.y + 4.0, 36.0, 28.0);
		buttons.push((all_off, Hit::AllOff));
		right -= 42.0;
		let all_on = Rect::new(right - 36.0, header.y + 4.0, 36.0, 28.0);
		buttons.push((all_on, Hit::AllOn));
		right -= 42.0;
		let scan = Rect::new(right - 28.0, header.y + 4.0, 28.0, 28.0);
		buttons.push((scan, Hit::Scan));
		right -= 34.0;
		let refresh = Rect::new(right - 28.0, header.y + 4.0, 28.0, 28.0);
		buttons.push((refresh, Hit::Refresh));

		let mut cards = Vec::new();
		let mut y = header.bottom() + 14.0;
		for device in &self.snapshot.devices {
			let error = device.error.is_some();
			let height = if error { 166.0 } else { 148.0 };
			let card = Rect::new(padding, y, width - padding * 2.0, height);
			let power = Rect::new(card.right() - 58.0, card.y + 14.0, 44.0, 24.0);
			let wifi = Rect::new(card.right() - 92.0, card.y + 15.0, 22.0, 22.0);
			let brightness = Rect::new(card.x + 38.0, card.y + 53.0, card.w - 100.0, 28.0);
			let warmth = Rect::new(card.x + 38.0, card.y + 85.0, card.w - 100.0, 28.0);
			let menu = Rect::new(card.right() - 30.0, card.y + 118.0, 20.0, 20.0);
			cards.push(CardLayout {
				name: device.name.clone(),
				card,
				power,
				brightness,
				warmth,
				menu,
				wifi,
			});
			y += height + 12.0;
		}
		let footer = Rect::new(padding, y + 2.0, width - padding * 2.0, 30.0);
		let content_height = if cards.is_empty() {
			220.0
		} else {
			footer.bottom() + padding
		};
		Layout {
			header,
			buttons,
			cards,
			footer,
			content_height,
		}
	}

	fn sheet_rects(&self) -> SheetRects {
		let (width, height) = self.dip_size();
		let panel_width = 400.0f32.min(width - 24.0);
		let panel = Rect::new(
			(width - panel_width) / 2.0,
			18.0,
			panel_width,
			(height - 36.0).max(120.0),
		);
		let padding = 20.0;
		let close = Rect::new(panel.x + padding, panel.bottom() - 52.0, 76.0, 32.0);
		let primary = Rect::new(
			panel.right() - padding - 100.0,
			panel.bottom() - 52.0,
			100.0,
			32.0,
		);
		let secondary = Rect::new(primary.x - 104.0, primary.y, 96.0, 32.0);
		let count_down = Rect::new(panel.x + padding, panel.y + 102.0, 28.0, 28.0);
		let count = Rect::new(count_down.right() + 46.0, panel.y + 102.0, 60.0, 28.0);
		let count_up = Rect::new(count.right() + 46.0, panel.y + 102.0, 28.0, 28.0);
		let edit = Rect::new(
			panel.x + padding,
			panel.y + 158.0,
			panel.w - padding * 2.0,
			30.0,
		);
		let slider = Rect::new(
			panel.x + padding,
			panel.y + 218.0,
			panel.w - padding * 2.0,
			40.0,
		);
		let radios = [
			(
				Rect::new(
					panel.x + padding,
					panel.y + 116.0,
					panel.w - padding * 2.0,
					26.0,
				),
				1,
			),
			(
				Rect::new(
					panel.x + padding,
					panel.y + 148.0,
					panel.w - padding * 2.0,
					26.0,
				),
				2,
			),
		];
		let row_count = match &self.sheet {
			Sheet::Discovery => self.snapshot.discovery_rows.len().min(5),
			_ => 0,
		};
		let mut rows = Vec::new();
		for index in 0..row_count {
			let row = Rect::new(
				panel.x + padding,
				panel.y + 92.0 + index as f32 * 52.0,
				panel.w - padding * 2.0,
				46.0,
			);
			let action = Rect::new(row.right() - 86.0, row.y + 8.0, 82.0, 30.0);
			rows.push((row, action));
		}
		let scan_again = Rect::new(panel.x + padding, panel.bottom() - 52.0, 104.0, 32.0);
		let message = Rect::new(
			panel.x + padding,
			panel.bottom() - 94.0,
			panel.w - padding * 2.0,
			34.0,
		);
		SheetRects {
			panel,
			title: Rect::new(
				panel.x + padding,
				panel.y + 16.0,
				panel.w - padding * 2.0,
				28.0,
			),
			description: Rect::new(
				panel.x + padding,
				panel.y + 48.0,
				panel.w - padding * 2.0,
				44.0,
			),
			close,
			primary,
			secondary,
			count,
			count_down,
			count_up,
			edit,
			slider,
			radios,
			rows,
			scan_again,
			message,
		}
	}

	fn hit_test(&self, x: f32, y: f32) -> Option<Hit> {
		if self.sheet != Sheet::None {
			return self.hit_sheet(x, y);
		}
		let layout = self.layout();
		for (rect, hit) in &layout.buttons {
			if rect.contains(x, y) {
				return Some(hit.clone());
			}
		}
		for card in &layout.cards {
			let offset = self.scroll;
			let card_rect = Rect {
				y: card.card.y - offset,
				..card.card
			};
			if !card_rect.contains(x, y) {
				continue;
			}
			let power = Rect {
				y: card.power.y - offset,
				..card.power
			};
			if power.contains(x, y) {
				return Some(Hit::Power(card.name.clone()));
			}
			let wifi = Rect {
				y: card.wifi.y - offset,
				..card.wifi
			};
			if wifi.contains(x, y) {
				return Some(Hit::Wifi(card.name.clone()));
			}
			for (kind, rect) in [("brightness", card.brightness), ("warmth", card.warmth)] {
				let rect = Rect {
					y: rect.y - offset,
					..rect
				};
				if rect.contains(x, y) {
					return Some(if kind == "brightness" {
						Hit::Brightness(card.name.clone())
					} else {
						Hit::Warmth(card.name.clone())
					});
				}
			}
			let menu = Rect {
				y: card.menu.y - offset,
				..card.menu
			};
			if menu.contains(x, y) {
				return Some(Hit::Menu(card.name.clone()));
			}
			return Some(Hit::Card(card.name.clone()));
		}
		None
	}

	fn hit_sheet(&self, x: f32, y: f32) -> Option<Hit> {
		let rects = self.sheet_rects();
		if rects.close.contains(x, y) {
			return Some(Hit::SheetClose);
		}
		if rects.primary.contains(x, y) {
			return Some(Hit::SheetPrimary);
		}
		if rects.secondary.contains(x, y) {
			return Some(Hit::SheetSecondary);
		}
		match &self.sheet {
			Sheet::Discovery => {
				if rects.scan_again.contains(x, y) {
					return Some(Hit::SheetScanAgain);
				}
				for (index, (row, action)) in rects.rows.iter().enumerate() {
					if action.contains(x, y)
						&& let Some(row) = self.snapshot.discovery_rows.get(index)
					{
						return Some(Hit::DiscoveryAction(row.id.clone()));
					}
					let _ = row;
				}
				None
			}
			Sheet::Rename { .. } => None,
			Sheet::PowerOn { .. } => {
				for (rect, value) in rects.radios {
					if rect.contains(x, y) {
						return Some(Hit::SheetRadio(value));
					}
				}
				let warmth = Rect::new(rects.slider.x, rects.slider.y + 38.0, rects.slider.w, 28.0);
				if warmth.contains(x, y) {
					return Some(Hit::SheetWarmth);
				}
				if rects.slider.contains(x, y) {
					return Some(Hit::SheetBrightness);
				}
				None
			}
			Sheet::Pulse { .. } => {
				if rects.count_down.contains(x, y) {
					return Some(Hit::SheetCountDown);
				}
				if rects.count_up.contains(x, y) {
					return Some(Hit::SheetCountUp);
				}
				if rects.slider.contains(x, y) {
					let mid = rects.slider.y + 18.0;
					if y > mid + 10.0 {
						return Some(Hit::SheetLow);
					}
					if y < mid - 10.0 {
						return Some(Hit::SheetPeak);
					}
					return Some(Hit::SheetLow);
				}
				None
			}
			Sheet::Sweep { .. } => {
				if rects.count_down.contains(x, y) {
					return Some(Hit::SheetCountDown);
				}
				if rects.count_up.contains(x, y) {
					return Some(Hit::SheetCountUp);
				}
				if rects.slider.contains(x, y) {
					return Some(Hit::SheetBrightness);
				}
				None
			}
			Sheet::None => None,
		}
	}

	fn on_paint(&mut self) {
		let (width, height) = client_size(self.window);
		let _ = self.renderer.resize(
			self.window,
			width.max(1) as u32,
			height.max(1) as u32,
			self.dpi as f32,
		);
		self.renderer.begin(self.window, self.theme.panel_bg);
		if self.sheet == Sheet::None {
			self.draw_panel();
		} else {
			self.draw_sheet();
		}
		self.renderer.end();
	}

	fn draw_panel(&mut self) {
		let layout = self.layout();
		self.draw_header(&layout);
		if let Some(error) = self.snapshot.error.clone() {
			self.renderer.text_ex(
				&error,
				Rect::new(18.0, layout.header.bottom() + 2.0, PANEL_WIDTH - 36.0, 18.0),
				10.0,
				400,
				TextAlign::Left,
				true,
				false,
				self.theme.danger,
			);
		}
		if self.snapshot.devices.is_empty() {
			self.renderer.text(
				"No lights configured",
				Rect::new(18.0, 100.0, PANEL_WIDTH - 36.0, 24.0),
				15.0,
				600,
				TextAlign::Center,
				self.theme.text,
			);
			self.renderer.text_ex(
				"Add devices to the registry, or scan the local network.",
				Rect::new(18.0, 128.0, PANEL_WIDTH - 36.0, 40.0),
				11.0,
				400,
				TextAlign::Center,
				false,
				true,
				self.theme.text_secondary,
			);
			return;
		}
		let graph = self.graph();
		let visible_top = layout.header.bottom() + 6.0;
		let (_, window_height) = self.dip_size();
		let visible = Rect::new(
			0.0,
			visible_top,
			PANEL_WIDTH,
			(window_height - visible_top).max(1.0),
		);
		self.renderer.push_clip(visible);
		let dragged = match &self.drag {
			Some(Drag::Card {
				name,
				offset_x,
				offset_y,
				active: true,
				..
			}) => Some((name.clone(), *offset_x, *offset_y)),
			_ => None,
		};
		for card in &layout.cards {
			if dragged
				.as_ref()
				.is_some_and(|(name, _, _)| *name == card.name)
			{
				continue;
			}
			let offset_y = -self.scroll;
			let offset_x = 0.0;
			self.draw_card(card, &graph, offset_x, offset_y);
			if self
				.drag_target
				.as_ref()
				.is_some_and(|name| *name == card.name)
			{
				self.renderer.stroke_round(
					Rect {
						x: card.card.x,
						y: card.card.y - self.scroll,
						..card.card
					},
					16.0,
					self.theme.accent,
					2.0,
				);
			}
		}
		if let Some((name, x, y)) = dragged
			&& let Some(card) = layout.cards.iter().find(|card| card.name == name)
		{
			self.draw_card(card, &graph, x, y - self.scroll);
		}
		self.draw_footer(&layout);
		self.renderer.pop_clip();
		if layout.content_height > window_height + 1.0 {
			self.draw_scrollbar(visible, layout.content_height, window_height);
		}
	}

	fn draw_header(&mut self, layout: &Layout) {
		self.renderer.text(
			"Elopli",
			Rect::new(18.0, 18.0, 150.0, 36.0),
			20.0,
			600,
			TextAlign::Left,
			self.theme.text,
		);
		if self.snapshot.refreshing {
			self.renderer.circle(
				layout.header.x + 58.0,
				layout.header.y + 18.0,
				3.0,
				self.theme.accent,
			);
		}
		for (rect, hit) in &layout.buttons {
			let hovered = self.hover.as_ref() == Some(hit);
			if hovered {
				self.renderer.fill_round(*rect, 7.0, self.theme.hover);
			}
			let color = self.theme.text_secondary;
			match hit {
				Hit::Refresh => self.renderer.refresh_icon(
					rect.x + rect.w / 2.0,
					rect.y + rect.h / 2.0,
					14.0,
					color,
				),
				Hit::Scan => self.renderer.wifi(
					rect.x + rect.w / 2.0,
					rect.y + rect.h / 2.0 + 4.0,
					color,
					true,
				),
				Hit::AllOn => {
					self.renderer
						.text("On", *rect, 11.0, 600, TextAlign::Center, color);
				}
				Hit::AllOff => {
					self.renderer
						.text("Off", *rect, 11.0, 600, TextAlign::Center, color);
				}
				Hit::Settings => {
					self.renderer
						.dots(rect.x + rect.w / 2.0, rect.y + rect.h / 2.0, color, true)
				}
				_ => {}
			}
		}
	}

	fn draw_footer(&mut self, layout: &Layout) {
		let y = layout.footer.y - self.scroll;
		self.renderer.text(
			"Ctrl-click a card, toggle, or slider to link lights.",
			Rect::new(18.0, y, PANEL_WIDTH - 36.0, 24.0),
			10.0,
			400,
			TextAlign::Left,
			self.theme.text_secondary,
		);
	}

	fn draw_scrollbar(&mut self, visible: Rect, content: f32, window_height: f32) {
		let track = Rect::new(PANEL_WIDTH - 7.0, visible.y, 3.0, visible.h);
		self.renderer.fill_round(track, 1.5, self.theme.track);
		let fraction = (window_height / content).clamp(0.1, 1.0);
		let max_scroll = (content - window_height).max(1.0);
		let thumb_h = (visible.h * fraction).max(28.0);
		let y = visible.y + (visible.h - thumb_h) * (self.scroll / max_scroll);
		self.renderer.fill_round(
			Rect::new(track.x, y, track.w, thumb_h),
			1.5,
			self.theme.text_secondary,
		);
	}

	fn draw_card(&mut self, card: &CardLayout, graph: &LinkGraph, dx: f32, dy: f32) {
		let Some(device) = self
			.snapshot
			.devices
			.iter()
			.find(|device| device.name == card.name)
			.cloned()
		else {
			return;
		};
		let card_rect = Rect {
			x: card.card.x + dx,
			y: card.card.y + dy,
			..card.card
		};
		let power = Rect {
			x: card.power.x + dx,
			y: card.power.y + dy,
			..card.power
		};
		let wifi = Rect {
			x: card.wifi.x + dx,
			y: card.wifi.y + dy,
			..card.wifi
		};
		let brightness = Rect {
			x: card.brightness.x + dx,
			y: card.brightness.y + dy,
			..card.brightness
		};
		let warmth = Rect {
			x: card.warmth.x + dx,
			y: card.warmth.y + dy,
			..card.warmth
		};
		let menu = Rect {
			x: card.menu.x + dx,
			y: card.menu.y + dy,
			..card.menu
		};
		self.renderer
			.fill_round(card_rect, 16.0, self.theme.card_bg);
		self.renderer
			.stroke_round(card_rect, 16.0, self.theme.card_border, 1.0);
		if let Some(color) = self.link_color(graph, &card.name, LinkKind::Card) {
			self.renderer.stroke_round(card_rect, 16.0, color, 2.0);
		}

		self.renderer.text(
			&device.name,
			Rect::new(card_rect.x + 14.0, card_rect.y + 10.0, 240.0, 20.0),
			15.0,
			600,
			TextAlign::Left,
			self.theme.text,
		);
		self.renderer.text(
			&device.subtitle(),
			Rect::new(card_rect.x + 14.0, card_rect.y + 30.0, 240.0, 16.0),
			10.0,
			400,
			TextAlign::Left,
			self.theme.text_secondary,
		);

		if let Some(color) = self.link_color(graph, &card.name, LinkKind::Power) {
			self.renderer
				.stroke_round(power.inset(-3.0), 12.0, color, 1.5);
		}
		let on = device.is_effectively_on();
		self.renderer.toggle(power, on, &self.theme);

		self.renderer.wifi(
			wifi.x + wifi.w / 2.0,
			wifi.y + wifi.h - 4.0,
			self.wifi_color(&device),
			device
				.info
				.as_ref()
				.and_then(|info| info.wifi.as_ref())
				.is_some()
				&& device.error.is_none(),
		);
		if let Some(color) = self.link_color(graph, &card.name, LinkKind::Brightness) {
			self.renderer.stroke_round(brightness, 8.0, color, 1.5);
		}
		if let Some(color) = self.link_color(graph, &card.name, LinkKind::Warmth) {
			self.renderer.stroke_round(warmth, 8.0, color, 1.5);
		}

		let brightness_value = self
			.preview
			.get(&(card.name.clone(), LinkKind::Brightness))
			.copied()
			.or_else(|| self.snapshot.live_brightness.get(&card.name).copied())
			.unwrap_or_else(|| device.display_brightness());
		let warmth_value = self
			.preview
			.get(&(card.name.clone(), LinkKind::Warmth))
			.copied()
			.or_else(|| self.snapshot.live_warmth.get(&card.name).copied())
			.unwrap_or_else(|| device.display_warmth());
		self.renderer.sun(
			brightness.x + 10.0,
			brightness.y + brightness.h / 2.0,
			16.0,
			self.theme.text_secondary,
		);
		self.draw_slider(
			Rect {
				x: brightness.x + 24.0,
				w: brightness.w - 84.0,
				..brightness
			},
			f32::from(brightness_value),
			2.0,
			100.0,
			false,
			track_color(brightness_value),
			on && device.state.is_some(),
		);
		let brightness_label = if device
			.state
			.is_some_and(|state| state.on && state.brightness < 2)
			&& !self
				.preview
				.contains_key(&(card.name.clone(), LinkKind::Brightness))
		{
			"off".to_string()
		} else {
			format!("{brightness_value}%")
		};
		self.renderer.text(
			&brightness_label,
			Rect::new(card_rect.right() - 58.0, brightness.y, 44.0, brightness.h),
			10.0,
			400,
			TextAlign::Right,
			self.theme.text_secondary,
		);

		self.renderer.flame(
			warmth.x + 10.0,
			warmth.y + warmth.h / 2.0,
			16.0,
			self.theme.text_secondary,
		);
		let warmth_rect = Rect {
			x: warmth.x + 24.0,
			w: warmth.w - 84.0,
			..warmth
		};
		self.draw_warmth_slider(warmth_rect, f32::from(warmth_value));
		self.renderer.text(
			&format!("{warmth_value}%"),
			Rect::new(card_rect.right() - 58.0, warmth.y, 44.0, warmth.h),
			10.0,
			400,
			TextAlign::Right,
			self.theme.text_secondary,
		);

		if let Some(state) = device.state {
			self.renderer.text(
				&format!("{} mired · {} K", state.temperature, state.kelvin()),
				Rect::new(card_rect.x + 14.0, card_rect.y + 118.0, 240.0, 18.0),
				9.5,
				400,
				TextAlign::Left,
				self.theme.text_secondary,
			);
		}
		if self.hover.as_ref() == Some(&Hit::Menu(card.name.clone())) {
			self.renderer
				.fill_round(menu.inset(-4.0), 7.0, self.theme.hover);
		}
		self.renderer.dots(
			menu.x + menu.w / 2.0,
			menu.y + menu.h / 2.0,
			self.theme.text_secondary,
			false,
		);
		if let Some(error) = &device.error {
			self.renderer.text_ex(
				error,
				Rect::new(
					card_rect.x + 14.0,
					card_rect.y + 142.0,
					card_rect.w - 28.0,
					18.0,
				),
				9.5,
				400,
				TextAlign::Left,
				true,
				false,
				self.theme.danger,
			);
		}
		if self.wifi_popover.as_deref() == Some(card.name.as_str()) {
			self.draw_wifi_popover(&device, wifi);
		}
	}

	#[allow(clippy::too_many_arguments)]
	fn draw_slider(
		&mut self,
		rect: Rect,
		value: f32,
		min: f32,
		max: f32,
		reversed: bool,
		color: u32,
		glow: bool,
	) {
		let center = rect.y + rect.h / 2.0;
		let track_x = rect.x + 8.0;
		let track_w = rect.w - 16.0;
		let t = ((value - min) / (max - min)).clamp(0.0, 1.0);
		let knob_x = track_x + track_w * if reversed { 1.0 - t } else { t };
		let thickness = 4.0 + 4.0 * t;
		self.renderer.fill_round(
			Rect::new(track_x, center - thickness / 2.0, track_w, thickness),
			thickness / 2.0,
			self.theme.track,
		);
		if glow {
			for index in 0..3 {
				self.renderer.circle(
					knob_x,
					center,
					9.0 + index as f32 * 3.0,
					rgba(255, 255, 255, 10 - index * 3),
				);
			}
		}
		if reversed {
			self.renderer.fill_round(
				Rect::new(
					knob_x,
					center - thickness / 2.0,
					track_x + track_w - knob_x,
					thickness,
				),
				thickness / 2.0,
				color,
			);
		} else {
			self.renderer.fill_round(
				Rect::new(
					track_x,
					center - thickness / 2.0,
					knob_x - track_x,
					thickness,
				),
				thickness / 2.0,
				color,
			);
		}
		self.renderer.circle(knob_x, center, 7.0, self.theme.knob);
		self.renderer
			.ring(knob_x, center, 7.0, self.theme.card_border, 1.0);
	}

	fn draw_warmth_slider(&mut self, rect: Rect, value: f32) {
		let center = rect.y + rect.h / 2.0;
		let track_x = rect.x + 8.0;
		let track_w = rect.w - 16.0;
		let t = (value / 100.0).clamp(0.0, 1.0);
		let knob_x = track_x + track_w * (1.0 - t);
		self.renderer.gradient_rect(
			Rect::new(track_x, center - 3.0, track_w, 6.0),
			&[
				(0.0, Theme::temperature_color(100.0)),
				(0.6, Theme::temperature_color(40.0)),
				(1.0, Theme::temperature_color(0.0)),
			],
			true,
		);
		self.renderer
			.circle(knob_x, center, 7.0, Theme::temperature_color(value));
		self.renderer
			.ring(knob_x, center, 7.0, self.theme.card_border, 1.0);
	}

	fn wifi_color(&self, device: &LightDevice) -> u32 {
		if device.error.is_some() {
			return self.theme.danger;
		}
		device
			.info
			.as_ref()
			.and_then(|info| info.wifi.as_ref())
			.map_or(self.theme.text_secondary, |wifi| {
				self.theme.wifi_color(wifi.quality())
			})
	}

	fn draw_wifi_popover(&mut self, device: &LightDevice, anchor: Rect) {
		let text = if let Some(error) = &device.error {
			format!(
				"{error} · Address {}. Power cycle the light, wait, then refresh.",
				device.host
			)
		} else if let Some(wifi) = device.info.as_ref().and_then(|info| info.wifi.as_ref()) {
			let frequency = if wifi.frequency_mhz < 3000 {
				"2.4 GHz"
			} else {
				"5 GHz"
			};
			format!(
				"{} · {} · {} dBm ({}) · Address {}",
				wifi.ssid,
				frequency,
				wifi.rssi,
				wifi.quality(),
				device.host
			)
		} else {
			format!("Loading network details · Address {}", device.host)
		};
		let width = 280.0;
		let mut x = anchor.x + anchor.w / 2.0 - width / 2.0;
		let y = anchor.y + anchor.h + 6.0;
		x = x.clamp(8.0, PANEL_WIDTH - width - 8.0);
		let (_, height) = self.renderer.measure(&text, 10.0, 400, width - 20.0);
		let rect = Rect::new(x, y, width, height + 16.0);
		self.renderer.shadow_round(rect, 8.0);
		self.renderer.fill_round(rect, 8.0, self.theme.card_bg);
		self.renderer
			.stroke_round(rect, 8.0, self.theme.card_border, 1.0);
		self.renderer.text_ex(
			&text,
			rect.inset(10.0),
			10.0,
			400,
			TextAlign::Left,
			false,
			true,
			self.theme.text_secondary,
		);
	}

	fn graph(&self) -> LinkGraph {
		LinkGraph {
			groups: self.snapshot.groups.clone(),
			pending: self
				.snapshot
				.pending_links
				.iter()
				.cloned()
				.collect::<BTreeSet<_>>(),
		}
	}

	fn link_color(&self, graph: &LinkGraph, name: &str, kind: LinkKind) -> Option<u32> {
		let target = LinkTarget::new(name, kind);
		if self.snapshot.pending_links.contains(&target) {
			return Some(self.theme.pending);
		}
		if self.ctrl_down && self.hover_target(name, kind) {
			return Some(rgba(255, 214, 10, 210));
		}
		if graph.is_linked(&target) {
			return Some(rgba(255, 0, 255, 150));
		}
		None
	}

	fn hover_target(&self, name: &str, kind: LinkKind) -> bool {
		match (&self.hover, kind) {
			(Some(Hit::Card(hovered)), LinkKind::Card) => hovered == name,
			(Some(Hit::Power(hovered)), LinkKind::Power) => hovered == name,
			(Some(Hit::Brightness(hovered)), LinkKind::Brightness) => hovered == name,
			(Some(Hit::Warmth(hovered)), LinkKind::Warmth) => hovered == name,
			_ => false,
		}
	}

	fn linked_names(&self, name: &str, kind: LinkKind) -> Vec<String> {
		self.graph().linked_device_names(name, kind)
	}

	fn preview_slider(&mut self, name: &str, kind: LinkKind, value: u8) {
		let names = self.linked_names(name, kind);
		for linked in names {
			self.preview.insert((linked, kind), value);
		}
		self.preview_until = Some(Instant::now() + std::time::Duration::from_millis(700));
	}

	fn clear_missing_preview(&mut self) {
		if let Some(until) = self.preview_until
			&& Instant::now() > until
		{
			self.preview.clear();
			self.preview_until = None;
		}
		if self.preview_until.is_none() {
			self.preview.clear();
		}
	}

	fn clamp_scroll(&mut self) {
		let layout = self.layout();
		let (_, height) = self.dip_size();
		let max = (layout.content_height - height).max(0.0);
		self.scroll = self.scroll.clamp(0.0, max);
	}

	fn open_sheet(&mut self, sheet: Sheet) {
		self.cancel_sheet_animation();
		self.destroy_edit();
		self.sheet = sheet;
		self.create_sheet_edit();
		self.resize_to_content(true);
		self.position();
		self.invalidate();
	}

	fn close_sheet(&mut self) {
		self.destroy_edit();
		self.sheet = Sheet::None;
		self.resize_to_content(true);
		self.position();
		self.invalidate();
	}

	fn cancel_sheet_animation(&mut self) {
		let names = match &self.sheet {
			Sheet::Pulse {
				names,
				started: true,
				..
			}
			| Sheet::Sweep {
				names,
				started: true,
				..
			} => names.clone(),
			_ => return,
		};
		if !names.is_empty() {
			self.worker.send(Command::CancelAnimations(names));
		}
	}

	fn create_sheet_edit(&mut self) {
		let (kind, text, rect) = match &self.sheet {
			Sheet::Rename { value, .. } => {
				(EditKind::Rename, value.clone(), self.sheet_rects().edit)
			}
			Sheet::Pulse { duration, .. } => (
				EditKind::PulseDuration,
				duration.clone(),
				self.sheet_rects().edit,
			),
			Sheet::Sweep { duration, .. } => (
				EditKind::SweepDuration,
				duration.clone(),
				self.sheet_rects().edit,
			),
			_ => return,
		};
		let instance = unsafe { GetModuleHandleW(None) }.unwrap_or_default();
		let class = wide("EDIT");
		let text = wide(&text);
		let font = unsafe {
			CreateFontW(
				-scale_i32(self.dpi, 13.0),
				0,
				0,
				0,
				FW_NORMAL.0 as i32,
				0,
				0,
				0,
				DEFAULT_CHARSET,
				OUT_DEFAULT_PRECIS,
				CLIP_DEFAULT_PRECIS,
				CLEARTYPE_QUALITY,
				u32::from(DEFAULT_PITCH.0 | FF_DONTCARE.0),
				PCWSTR(wide("Segoe UI").as_ptr()),
			)
		};
		let brush = unsafe { CreateSolidBrush(colorref(self.theme.edit_bg)) };
		let window = unsafe {
			CreateWindowExW(
				Default::default(),
				PCWSTR(class.as_ptr()),
				PCWSTR(text.as_ptr()),
				WS_CHILD | WS_VISIBLE | WINDOW_STYLE((ES_LEFT | ES_AUTOHSCROLL) as u32),
				scale_i32(self.dpi, rect.x),
				scale_i32(self.dpi, rect.y),
				scale_i32(self.dpi, rect.w),
				scale_i32(self.dpi, rect.h),
				Some(self.window),
				Some(HMENU(std::ptr::dangling_mut::<core::ffi::c_void>())),
				Some(HINSTANCE(instance.0)),
				None,
			)
		};
		let Ok(window) = window else {
			return;
		};
		unsafe {
			let _ = SetWindowSubclass(window, Some(edit_proc), 1, 0);
			let _ = SendMessageW(
				window,
				WM_SETFONT,
				Some(WPARAM(font.0 as usize)),
				Some(LPARAM(1)),
			);
			let _ = SetFocus(Some(window));
			let _ = SendMessageW(window, EM_SETSEL, Some(WPARAM(0)), Some(LPARAM(-1)));
		}
		self.edit = Some(Edit {
			window,
			kind,
			font,
			brush,
		});
	}

	fn destroy_edit(&mut self) {
		self.edit = None;
	}

	fn read_edit(&mut self) {
		let Some(edit) = &self.edit else {
			return;
		};
		let length = unsafe { GetWindowTextLengthW(edit.window) };
		if length <= 0 {
			self.set_edit_text(String::new());
			return;
		}
		let mut buffer = vec![0u16; length as usize + 1];
		let read = unsafe { GetWindowTextW(edit.window, &mut buffer) };
		let text = String::from_utf16_lossy(&buffer[..read.max(0) as usize]);
		self.set_edit_text(text);
	}

	fn set_edit_text(&mut self, text: String) {
		let Some(edit) = &self.edit else {
			return;
		};
		match (&mut self.sheet, edit.kind) {
			(Sheet::Rename { value, .. }, EditKind::Rename) => *value = text,
			(Sheet::Pulse { duration, .. }, EditKind::PulseDuration) => *duration = text,
			(Sheet::Sweep { duration, .. }, EditKind::SweepDuration) => *duration = text,
			_ => {}
		}
	}

	fn draw_sheet(&mut self) {
		let (width, height) = self.dip_size();
		self.renderer
			.fill_rect(Rect::new(0.0, 0.0, width, height), rgba(0, 0, 0, 110));
		let rects = self.sheet_rects();
		self.renderer.shadow_round(rects.panel, 14.0);
		self.renderer
			.fill_round(rects.panel, 14.0, self.theme.card_bg);
		self.renderer
			.stroke_round(rects.panel, 14.0, self.theme.card_border, 1.0);
		let title = match &self.sheet {
			Sheet::None => "",
			Sheet::Discovery => "Discover Lights",
			Sheet::Rename { .. } => "Rename Light",
			Sheet::PowerOn { .. } => "Power On Behaviour",
			Sheet::Pulse { names, .. } if names.len() > 1 => "Pulse Group",
			Sheet::Pulse { .. } => "Pulse",
			Sheet::Sweep { names, .. } if names.len() > 1 => "Sweep Group",
			Sheet::Sweep { .. } => "Sweep",
		};
		self.renderer.text(
			title,
			rects.title,
			18.0,
			600,
			TextAlign::Left,
			self.theme.text,
		);
		let sheet = self.sheet.clone();
		match &sheet {
			Sheet::None => {}
			Sheet::Discovery => self.draw_discovery(&rects),
			Sheet::Rename { name, .. } => {
				self.renderer.text(
					"This changes the name stored on the light.",
					rects.description,
					11.0,
					400,
					TextAlign::Left,
					self.theme.text_secondary,
				);
				self.renderer.text(
					&format!("Registry name: {name}"),
					Rect::new(rects.edit.x, rects.edit.y - 22.0, rects.edit.w, 18.0),
					10.0,
					400,
					TextAlign::Left,
					self.theme.text_secondary,
				);
				self.renderer.text(
					"The name can have 1 to 63 bytes.",
					Rect::new(rects.edit.x, rects.edit.bottom() + 8.0, rects.edit.w, 18.0),
					10.0,
					400,
					TextAlign::Left,
					self.theme.text_secondary,
				);
				self.draw_button(rects.primary, "Save", true, true);
				self.draw_button(rects.close, "Cancel", false, true);
			}
			Sheet::PowerOn {
				name,
				behavior,
				brightness,
				warmth,
			} => {
				self.renderer.text(
					&format!("{name} can restore the last setting or its defaults."),
					rects.description,
					11.0,
					400,
					TextAlign::Left,
					self.theme.text_secondary,
				);
				self.draw_radio(
					rects.radios[0].0,
					*behavior == 1,
					"Restore Last Used Setting",
				);
				self.draw_radio(rects.radios[1].0, *behavior == 2, "Restore Defaults");
				if *behavior == 2 {
					self.renderer.sun(
						rects.slider.x + 10.0,
						rects.slider.y + 12.0,
						16.0,
						self.theme.text_secondary,
					);
					self.draw_slider(
						Rect::new(
							rects.slider.x + 24.0,
							rects.slider.y,
							rects.slider.w - 84.0,
							28.0,
						),
						f32::from(*brightness),
						2.0,
						100.0,
						false,
						track_color(*brightness),
						true,
					);
					self.renderer.text(
						&format!("{brightness}%"),
						Rect::new(rects.slider.right() - 58.0, rects.slider.y, 44.0, 28.0),
						10.0,
						400,
						TextAlign::Right,
						self.theme.text_secondary,
					);
					let warmth_rect = Rect::new(
						rects.slider.x + 24.0,
						rects.slider.y + 38.0,
						rects.slider.w - 84.0,
						28.0,
					);
					self.draw_warmth_slider(warmth_rect, f32::from(*warmth));
					self.renderer.text(
						&format!("{warmth}%"),
						Rect::new(rects.slider.right() - 58.0, warmth_rect.y, 44.0, 28.0),
						10.0,
						400,
						TextAlign::Right,
						self.theme.text_secondary,
					);
				}
				self.draw_button(rects.primary, "Save", true, true);
				self.draw_button(rects.close, "Cancel", false, true);
			}
			Sheet::Pulse {
				names,
				count,
				duration: _,
				peak,
				low,
				started,
				message,
				..
			} => {
				self.renderer.text_ex(
					"Each pulse is one peak-to-minimum fade. The original values are restored at the end.",
					rects.description,
					10.5,
					400,
					TextAlign::Left,
					false,
					true,
					self.theme.text_secondary,
				);
				if names.len() > 1 {
					self.renderer.text(
						&names.join(", "),
						Rect::new(
							rects.description.x,
							rects.description.bottom() + 2.0,
							rects.description.w,
							16.0,
						),
						10.0,
						400,
						TextAlign::Left,
						self.theme.text_secondary,
					);
				}
				self.draw_count(rects.count_down, rects.count, rects.count_up, *count);
				self.renderer.text(
					"Duration",
					Rect::new(rects.edit.x, rects.edit.y - 22.0, rects.edit.w, 18.0),
					10.0,
					400,
					TextAlign::Left,
					self.theme.text_secondary,
				);
				self.renderer.text_ex(
					"Use 1.0s, 500ms, 1.5, 10m, or 1h. The duration covers the whole animation.",
					Rect::new(rects.edit.x, rects.edit.bottom() + 6.0, rects.edit.w, 30.0),
					9.5,
					400,
					TextAlign::Left,
					false,
					true,
					self.theme.text_secondary,
				);
				self.draw_dual_slider(
					Rect::new(rects.slider.x, rects.slider.y + 20.0, rects.slider.w, 34.0),
					*low,
					*peak,
				);
				if let Some(message) = message {
					self.renderer.text_ex(
						message,
						rects.message,
						10.0,
						400,
						TextAlign::Left,
						false,
						true,
						self.theme.text_secondary,
					);
				}
				self.draw_button(rects.close, "Close", false, true);
				if *started {
					self.draw_button(rects.primary, "Stop", true, true);
				} else {
					self.draw_button(rects.primary, "Pulse", true, true);
				}
			}
			Sheet::Sweep {
				names,
				count,
				brightness,
				started,
				message,
				..
			} => {
				self.renderer.text_ex(
					"Each sweep is one warm-to-cool fade. The original values are restored at the end.",
					rects.description,
					10.5,
					400,
					TextAlign::Left,
					false,
					true,
					self.theme.text_secondary,
				);
				if names.len() > 1 {
					self.renderer.text(
						&names.join(", "),
						Rect::new(
							rects.description.x,
							rects.description.bottom() + 2.0,
							rects.description.w,
							16.0,
						),
						10.0,
						400,
						TextAlign::Left,
						self.theme.text_secondary,
					);
				}
				self.draw_count(rects.count_down, rects.count, rects.count_up, *count);
				self.renderer.text(
					"Duration",
					Rect::new(rects.edit.x, rects.edit.y - 22.0, rects.edit.w, 18.0),
					10.0,
					400,
					TextAlign::Left,
					self.theme.text_secondary,
				);
				self.draw_slider(
					Rect::new(rects.slider.x, rects.slider.y, rects.slider.w - 64.0, 28.0),
					f32::from(*brightness),
					0.0,
					100.0,
					false,
					self.theme.accent,
					true,
				);
				let brightness_label = if *brightness < 2 {
					"N/A".to_string()
				} else {
					format!("{brightness}%")
				};
				self.renderer.text(
					&brightness_label,
					Rect::new(rects.slider.right() - 58.0, rects.slider.y, 44.0, 28.0),
					10.0,
					400,
					TextAlign::Right,
					self.theme.text_secondary,
				);
				self.renderer.text_ex(
					"N/A keeps the current brightness. Values from 2% hold that brightness for the sweep.",
					Rect::new(
						rects.slider.x,
						rects.slider.bottom() + 2.0,
						rects.slider.w,
						30.0,
					),
					9.5,
					400,
					TextAlign::Left,
					false,
					true,
					self.theme.text_secondary,
				);
				if let Some(message) = message {
					self.renderer.text_ex(
						message,
						rects.message,
						10.0,
						400,
						TextAlign::Left,
						false,
						true,
						self.theme.text_secondary,
					);
				}
				self.draw_button(rects.close, "Close", false, true);
				if *started {
					self.draw_button(rects.primary, "Stop", true, true);
				} else {
					self.draw_button(rects.primary, "Sweep", true, true);
				}
			}
		}
	}

	fn draw_count(&mut self, down: Rect, count: Rect, up: Rect, value: u32) {
		self.draw_button(down, "−", false, true);
		self.renderer.text(
			&value.to_string(),
			count,
			15.0,
			600,
			TextAlign::Center,
			self.theme.text,
		);
		self.draw_button(up, "+", false, true);
	}

	fn draw_radio(&mut self, rect: Rect, selected: bool, label: &str) {
		let cx = rect.x + 10.0;
		let cy = rect.y + rect.h / 2.0;
		self.renderer
			.ring(cx, cy, 7.0, self.theme.text_secondary, 1.5);
		if selected {
			self.renderer.circle(cx, cy, 4.0, self.theme.accent);
		}
		self.renderer.text(
			label,
			Rect::new(rect.x + 26.0, rect.y, rect.w - 26.0, rect.h),
			11.0,
			400,
			TextAlign::Left,
			self.theme.text,
		);
	}

	fn draw_button(&mut self, rect: Rect, label: &str, primary: bool, enabled: bool) {
		let hovered = self.hovered_rect() == Some(rect);
		let base = if primary {
			self.theme.button_accent
		} else {
			self.theme.button_bg
		};
		let color = if hovered { self.theme.hover } else { base };
		self.renderer.fill_round(rect, 7.0, color);
		let text_color = if primary {
			crate::theme::rgb(255, 255, 255)
		} else {
			self.theme.text
		};
		self.renderer.text(
			label,
			rect,
			11.0,
			600,
			TextAlign::Center,
			if enabled {
				text_color
			} else {
				self.theme.text_secondary
			},
		);
	}

	fn hovered_rect(&self) -> Option<Rect> {
		let rects = self.sheet_rects();
		match self.hover.as_ref()? {
			Hit::SheetPrimary => Some(rects.primary),
			Hit::SheetSecondary => Some(rects.secondary),
			Hit::SheetClose => Some(rects.close),
			Hit::SheetScanAgain => Some(rects.scan_again),
			Hit::SheetCountDown => Some(rects.count_down),
			Hit::SheetCountUp => Some(rects.count_up),
			_ => None,
		}
	}

	fn draw_discovery(&mut self, rects: &SheetRects) {
		if self.snapshot.scanning {
			self.renderer.text(
				"Scanning the local network…",
				rects.description,
				11.0,
				400,
				TextAlign::Left,
				self.theme.text_secondary,
			);
		} else if self.snapshot.discovery_rows.is_empty() {
			self.renderer.text_ex(
				"No lights found. Make sure the lights are on the same network.",
				rects.description,
				11.0,
				400,
				TextAlign::Left,
				false,
				true,
				self.theme.text_secondary,
			);
		}
		for (index, (row, action)) in rects.rows.iter().enumerate() {
			let Some(discovery) = self.snapshot.discovery_rows.get(index).cloned() else {
				break;
			};
			self.renderer.fill_round(*row, 8.0, self.theme.button_bg);
			self.renderer.text(
				&discovery.name,
				Rect::new(row.x + 10.0, row.y + 6.0, row.w - 110.0, 18.0),
				11.0,
				600,
				TextAlign::Left,
				self.theme.text,
			);
			self.renderer.text(
				&discovery.detail,
				Rect::new(row.x + 10.0, row.y + 24.0, row.w - 110.0, 16.0),
				9.5,
				400,
				TextAlign::Left,
				self.theme.text_secondary,
			);
			self.draw_discovery_action(&discovery, *action);
		}
		let scanning = self.snapshot.scanning;
		self.draw_button(rects.scan_again, "Scan Again", false, !scanning);
		self.draw_button(rects.primary, "Done", true, true);
	}

	fn draw_discovery_action(&mut self, row: &DiscoveryRow, action: Rect) {
		match row.state {
			DiscoveryState::Added => {
				if row.needs_update {
					self.draw_button(action, "Update", true, true);
				} else {
					self.renderer.text(
						"Added",
						action,
						10.0,
						400,
						TextAlign::Center,
						self.theme.success,
					);
				}
			}
			DiscoveryState::Missing => {
				self.renderer.text(
					"Missing",
					action,
					10.0,
					400,
					TextAlign::Center,
					self.theme.danger,
				);
			}
			DiscoveryState::New => {
				let enabled = row
					.discovered
					.as_ref()
					.is_some_and(|device| device.address.is_some());
				self.draw_button(
					action,
					if enabled { "Add" } else { "No IPv4" },
					true,
					enabled,
				);
			}
		}
	}

	fn draw_dual_slider(&mut self, rect: Rect, low: u8, peak: u8) {
		let center = rect.y + 8.0;
		let track_x = rect.x + 8.0;
		let track_w = rect.w - 16.0;
		let low_x = track_x + track_w * f32::from(low.min(peak)) / 100.0;
		let peak_x = track_x + track_w * f32::from(peak) / 100.0;
		self.renderer.fill_round(
			Rect::new(track_x, center - 3.0, track_w, 6.0),
			3.0,
			self.theme.track,
		);
		self.renderer.fill_round(
			Rect::new(low_x, center - 3.0, (peak_x - low_x).max(0.0), 6.0),
			3.0,
			self.theme.pending,
		);
		self.renderer.circle(low_x, center, 7.0, self.theme.knob);
		self.renderer
			.ring(low_x, center, 7.0, self.theme.card_border, 1.0);
		self.renderer.circle(peak_x, center, 7.0, self.theme.knob);
		self.renderer
			.ring(peak_x, center, 7.0, self.theme.card_border, 1.0);
		let low_label = if low < 2 {
			"Low: Off".to_string()
		} else {
			format!("Low: {low}%")
		};
		self.renderer.text(
			&low_label,
			Rect::new(rect.x, center + 10.0, rect.w / 2.0, 16.0),
			9.5,
			400,
			TextAlign::Left,
			self.theme.text_secondary,
		);
		self.renderer.text(
			&format!("Peak: {peak}%"),
			Rect::new(rect.x + rect.w / 2.0, center + 10.0, rect.w / 2.0, 16.0),
			9.5,
			400,
			TextAlign::Right,
			self.theme.text_secondary,
		);
	}

	fn sheet_action(&mut self, hit: Hit) {
		match hit {
			Hit::SheetClose => {
				self.cancel_sheet_animation();
				self.close_sheet();
			}
			Hit::SheetPrimary => self.sheet_primary(),
			Hit::SheetSecondary => {
				self.cancel_sheet_animation();
			}
			Hit::SheetScanAgain => {
				self.worker.send(Command::Scan);
			}
			Hit::SheetCountDown => {
				if let Sheet::Pulse { count, .. } | Sheet::Sweep { count, .. } = &mut self.sheet {
					*count = count.saturating_sub(1).max(1);
				}
				self.invalidate();
			}
			Hit::SheetCountUp => {
				if let Sheet::Pulse { count, .. } | Sheet::Sweep { count, .. } = &mut self.sheet {
					*count = (*count + 1).min(20);
				}
				self.invalidate();
			}
			Hit::SheetRadio(value) => {
				if let Sheet::PowerOn { behavior, .. } = &mut self.sheet {
					*behavior = value;
				}
				self.invalidate();
			}
			_ => {}
		}
	}

	fn sheet_primary(&mut self) {
		match self.sheet.clone() {
			Sheet::Rename { name, value } => {
				if !value.trim().is_empty() {
					self.worker.send(Command::Rename {
						name,
						new_name: value,
					});
				}
				self.close_sheet();
			}
			Sheet::PowerOn {
				name,
				behavior,
				brightness,
				warmth,
			} => {
				let defaults = behavior == 2;
				let temperature = if defaults {
					warmth_to_mired(f64::from(warmth)).ok()
				} else {
					None
				};
				self.worker.send(Command::SetPowerOn {
					name,
					behavior: if behavior == 2 {
						PowerOnBehavior::RestoreDefaults
					} else {
						PowerOnBehavior::RestoreLast
					},
					brightness: defaults.then_some(brightness),
					temperature,
				});
				self.close_sheet();
			}
			Sheet::Pulse {
				names,
				count,
				duration,
				peak,
				low,
				started,
				..
			} => {
				if started {
					self.worker.send(Command::CancelAnimations(names));
					if let Sheet::Pulse {
						stopping, message, ..
					} = &mut self.sheet
					{
						*stopping = true;
						*message = Some("Stopping…".to_string());
					}
					self.invalidate();
					return;
				}
				let duration_ms = match parse_duration(&duration) {
					Ok(value) => value,
					Err(_) => {
						self.set_sheet_message("Invalid duration. Use 1.0s, 500ms, 10m, or 1h.");
						return;
					}
				};
				let minimum = count.saturating_mul(2).saturating_add(1) * 100;
				if duration_ms < minimum {
					self.set_sheet_message(&format!(
						"Duration must be at least {minimum} ms for {count} pulses."
					));
					return;
				}
				if duration_ms > 3_600_000 {
					self.set_sheet_message("Pulse duration must be one hour or less.");
					return;
				}
				let config = PulseConfig {
					count,
					duration_ms,
					value: peak,
					low,
				};
				if let Err(error) = config.validate() {
					self.set_sheet_message(&error.to_string());
					return;
				}
				self.worker.send(Command::Pulse { names, config });
				if let Sheet::Pulse {
					started,
					message,
					stopping,
					..
				} = &mut self.sheet
				{
					*started = true;
					*stopping = false;
					*message = Some("Pulsing…".to_string());
				}
				self.invalidate();
			}
			Sheet::Sweep {
				names,
				count,
				duration,
				brightness,
				started,
				..
			} => {
				if started {
					self.worker.send(Command::CancelAnimations(names));
					if let Sheet::Sweep {
						stopping, message, ..
					} = &mut self.sheet
					{
						*stopping = true;
						*message = Some("Stopping…".to_string());
					}
					self.invalidate();
					return;
				}
				let duration_ms = match parse_duration(&duration) {
					Ok(value) => value,
					Err(_) => {
						self.set_sheet_message("Invalid duration. Use 1.0s, 500ms, 10m, or 1h.");
						return;
					}
				};
				let minimum = count.saturating_mul(2).saturating_add(1) * 100;
				if duration_ms < minimum {
					self.set_sheet_message(&format!(
						"Duration must be at least {minimum} ms for {count} sweeps."
					));
					return;
				}
				if duration_ms > 3_600_000 {
					self.set_sheet_message("Sweep duration must be one hour or less.");
					return;
				}
				let config = SweepConfig {
					count,
					duration_ms,
					brightness: (brightness >= 2).then_some(brightness),
				};
				if let Err(error) = config.validate() {
					self.set_sheet_message(&error.to_string());
					return;
				}
				self.worker.send(Command::Sweep { names, config });
				if let Sheet::Sweep {
					started,
					message,
					stopping,
					..
				} = &mut self.sheet
				{
					*started = true;
					*stopping = false;
					*message = Some("Sweeping…".to_string());
				}
				self.invalidate();
			}
			Sheet::Discovery => self.close_sheet(),
			Sheet::None => {}
		}
	}

	fn set_sheet_message(&mut self, message: &str) {
		match &mut self.sheet {
			Sheet::Pulse { message: slot, .. } | Sheet::Sweep { message: slot, .. } => {
				*slot = Some(message.to_string());
			}
			_ => {}
		}
		self.invalidate();
	}

	fn discovery_action(&mut self, id: &str) {
		let Some(row) = self
			.snapshot
			.discovery_rows
			.iter()
			.find(|row| row.id == id)
			.cloned()
		else {
			return;
		};
		let Some(device) = row.discovered.clone() else {
			return;
		};
		match row.state {
			DiscoveryState::New => {
				self.worker.send(Command::AddDiscovered(device));
			}
			DiscoveryState::Added if row.needs_update => {
				self.worker.send(Command::UpdateDiscovered(device));
			}
			DiscoveryState::Added | DiscoveryState::Missing => {}
		}
	}

	fn open_card_menu(&mut self, name: &str, mut point: POINT) {
		unsafe {
			let _ = ClientToScreen(self.window, &mut point);
		}
		let graph = self.graph();
		let grouped = graph.is_grouped(name);
		let Some(menu) = (unsafe { CreatePopupMenu() }).ok() else {
			return;
		};
		let items = [
			(CARD_RENAME, "Rename…"),
			(CARD_POWER_ON, "Power On Behaviour…"),
			(CARD_PULSE, "Pulse…"),
			(CARD_PULSE_GROUP, "Pulse Group…"),
			(CARD_SWEEP, "Sweep…"),
			(CARD_SWEEP_GROUP, "Sweep Group…"),
			(CARD_IDENTIFY, "Identify"),
		];
		unsafe {
			for (index, (id, label)) in items.iter().enumerate() {
				if (index == 3 || index == 5) && !grouped {
					continue;
				}
				let label = wide(label);
				let _ = AppendMenuW(menu, MF_STRING, *id, PCWSTR(label.as_ptr()));
			}
			let _ = SetForegroundWindow(self.window);
			self.menu_open = true;
			let command = TrackPopupMenu(
				menu,
				TPM_RETURNCMD | TPM_RIGHTBUTTON,
				point.x,
				point.y,
				None,
				self.window,
				None,
			);
			self.menu_open = false;
			let _ = DestroyMenu(menu);
			self.handle_card_menu(name, command.0 as usize, grouped);
		}
	}

	fn handle_card_menu(&mut self, name: &str, command: usize, grouped: bool) {
		let device = self
			.snapshot
			.devices
			.iter()
			.find(|device| device.name == name)
			.cloned();
		match command {
			CARD_RENAME => {
				let value = device
					.as_ref()
					.and_then(|device| device.info.as_ref())
					.map(|info| info.display_name.trim().to_string())
					.filter(|value| !value.is_empty())
					.unwrap_or_else(|| name.to_string());
				self.open_sheet(Sheet::Rename {
					name: name.to_string(),
					value,
				});
			}
			CARD_POWER_ON => {
				let settings = device.as_ref().and_then(|device| device.power_on);
				let behavior = settings.map_or(1, |settings| settings.behavior as u8);
				let brightness = settings.map_or(20, |settings| settings.brightness.max(2));
				let warmth = settings.map_or(35, |settings| settings.warmth());
				self.open_sheet(Sheet::PowerOn {
					name: name.to_string(),
					behavior,
					brightness,
					warmth,
				});
			}
			CARD_PULSE | CARD_PULSE_GROUP => {
				let names = if grouped {
					self.graph().grouped_device_names(name)
				} else {
					vec![name.to_string()]
				};
				self.open_sheet(Sheet::Pulse {
					names,
					count: 3,
					duration: "1.0s".to_string(),
					peak: 100,
					low: 0,
					started: false,
					stopping: false,
					message: None,
				});
			}
			CARD_SWEEP | CARD_SWEEP_GROUP => {
				let names = if grouped {
					self.graph().grouped_device_names(name)
				} else {
					vec![name.to_string()]
				};
				self.open_sheet(Sheet::Sweep {
					names,
					count: 1,
					duration: "1.0s".to_string(),
					brightness: 0,
					started: false,
					stopping: false,
					message: None,
				});
			}
			CARD_IDENTIFY => {
				self.worker.send(Command::Identify(name.to_string()));
			}
			_ => {}
		}
	}

	fn open_settings_menu(&mut self, mut point: POINT) {
		unsafe {
			let _ = ClientToScreen(self.window, &mut point);
		}
		let Some(menu) = (unsafe { CreatePopupMenu() }).ok() else {
			return;
		};
		unsafe {
			let autostart = wide("Launch at Login");
			let flags =
				MENU_ITEM_FLAGS(MF_STRING.0 | if self.autostart { MF_CHECKED.0 } else { 0 });
			let _ = AppendMenuW(menu, flags, MENU_AUTOSTART, PCWSTR(autostart.as_ptr()));
			let reveal = wide("Reveal Config");
			let _ = AppendMenuW(menu, MF_STRING, MENU_REVEAL, PCWSTR(reveal.as_ptr()));
			let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
			let quit = wide("Quit Elopli");
			let _ = AppendMenuW(menu, MF_STRING, MENU_QUIT, PCWSTR(quit.as_ptr()));
			let _ = SetForegroundWindow(self.window);
			self.menu_open = true;
			let command = TrackPopupMenu(
				menu,
				TPM_RETURNCMD | TPM_RIGHTBUTTON,
				point.x,
				point.y,
				None,
				self.window,
				None,
			);
			self.menu_open = false;
			let _ = DestroyMenu(menu);
			match command.0 as usize {
				MENU_AUTOSTART => {
					let next = !self.autostart;
					if crate::win32::set_autostart(next).is_ok() {
						self.autostart = next;
					}
				}
				MENU_REVEAL => reveal_config(&self.snapshot.config_path),
				MENU_QUIT => self.quit(),
				_ => {}
			}
		}
	}

	fn show_tray_menu(&mut self, point: POINT) {
		let Some(menu) = (unsafe { CreatePopupMenu() }).ok() else {
			return;
		};
		unsafe {
			let visible = IsWindowVisible(self.window).as_bool();
			let label = wide(if visible {
				"Hide Elopli"
			} else {
				"Show Elopli"
			});
			let _ = AppendMenuW(menu, MF_STRING, MENU_SHOW, PCWSTR(label.as_ptr()));
			let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
			let refresh = wide("Refresh");
			let _ = AppendMenuW(menu, MF_STRING, MENU_REFRESH, PCWSTR(refresh.as_ptr()));
			let all_on = wide("All On");
			let _ = AppendMenuW(menu, MF_STRING, MENU_ALL_ON, PCWSTR(all_on.as_ptr()));
			let all_off = wide("All Off");
			let _ = AppendMenuW(menu, MF_STRING, MENU_ALL_OFF, PCWSTR(all_off.as_ptr()));
			let discover = wide("Discover Lights…");
			let _ = AppendMenuW(menu, MF_STRING, MENU_DISCOVER, PCWSTR(discover.as_ptr()));
			let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
			let autostart = wide("Launch at Login");
			let flags =
				MENU_ITEM_FLAGS(MF_STRING.0 | if self.autostart { MF_CHECKED.0 } else { 0 });
			let _ = AppendMenuW(menu, flags, MENU_AUTOSTART, PCWSTR(autostart.as_ptr()));
			let reveal = wide("Reveal Config");
			let _ = AppendMenuW(menu, MF_STRING, MENU_REVEAL, PCWSTR(reveal.as_ptr()));
			let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
			let quit = wide("Quit Elopli");
			let _ = AppendMenuW(menu, MF_STRING, MENU_QUIT, PCWSTR(quit.as_ptr()));
			let _ = SetForegroundWindow(self.window);
			self.menu_open = true;
			let command = TrackPopupMenu(
				menu,
				TPM_RETURNCMD | TPM_RIGHTBUTTON,
				point.x,
				point.y,
				None,
				self.window,
				None,
			);
			self.menu_open = false;
			let _ = DestroyMenu(menu);
			match command.0 as usize {
				MENU_SHOW => self.toggle(),
				MENU_REFRESH => self.worker.send(Command::Refresh),
				MENU_ALL_ON => self.worker.send(Command::SetAllOn(true)),
				MENU_ALL_OFF => self.worker.send(Command::SetAllOn(false)),
				MENU_DISCOVER => {
					self.cancel_sheet_animation();
					self.sheet = Sheet::Discovery;
					self.worker.send(Command::Scan);
					self.resize_to_content(true);
					self.position();
					self.invalidate();
				}
				MENU_AUTOSTART => {
					let next = !self.autostart;
					if crate::win32::set_autostart(next).is_ok() {
						self.autostart = next;
					}
				}
				MENU_REVEAL => reveal_config(&self.snapshot.config_path),
				MENU_QUIT => self.quit(),
				_ => {}
			}
		}
	}

	fn quit(&mut self) {
		self.quitting = true;
		unsafe {
			let _ = DestroyWindow(self.window);
		}
	}

	fn on_tick(&mut self) {
		let mut changed = false;
		self.clear_missing_preview();
		if let Some((name, since)) = &self.wifi_hover {
			if since.elapsed() >= std::time::Duration::from_millis(500)
				&& self.wifi_popover.as_deref() != Some(name.as_str())
			{
				self.wifi_popover = Some(name.clone());
				changed = true;
			}
		} else if self.wifi_popover.take().is_some() {
			changed = true;
		}
		let pulsing = self.snapshot.pulsing.clone();
		let done = match &self.sheet {
			Sheet::Pulse {
				names,
				started: true,
				..
			}
			| Sheet::Sweep {
				names,
				started: true,
				..
			} => !names.iter().any(|name| pulsing.contains(name)),
			_ => false,
		};
		if done {
			let (names, stopping) = match &self.sheet {
				Sheet::Pulse {
					names, stopping, ..
				}
				| Sheet::Sweep {
					names, stopping, ..
				} => (names.clone(), *stopping),
				_ => (Vec::new(), false),
			};
			let failures = names
				.iter()
				.filter(|name| {
					self.snapshot
						.devices
						.iter()
						.any(|device| &device.name == *name && device.error.is_some())
				})
				.count();
			let message = if stopping {
				"Stopped.".to_string()
			} else if failures > 0 {
				format!("Failed on {failures} of {} lights.", names.len())
			} else {
				"Completed.".to_string()
			};
			match &mut self.sheet {
				Sheet::Pulse {
					started,
					stopping,
					message: slot,
					..
				}
				| Sheet::Sweep {
					started,
					stopping,
					message: slot,
					..
				} => {
					*started = false;
					*stopping = false;
					*slot = Some(message);
				}
				_ => {}
			}
			changed = true;
		}
		if changed {
			self.invalidate();
		}
	}

	fn on_mouse_move(&mut self, x: f32, y: f32) {
		let mut track = TRACKMOUSEEVENT {
			cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
			dwFlags: TME_LEAVE,
			hwndTrack: self.window,
			dwHoverTime: 0,
		};
		unsafe {
			let _ = TrackMouseEvent(&mut track);
		}
		let ctrl = ctrl_down() || self.ctrl_down;
		if ctrl != self.ctrl_down {
			self.ctrl_down = ctrl;
			self.invalidate();
		}
		let hover = self.hit_test(x, y);
		if hover != self.hover {
			self.hover = hover.clone();
			self.invalidate();
		}
		if self.sheet == Sheet::None
			&& let Some(Hit::Wifi(name)) = &hover
		{
			if self.wifi_hover.as_ref().map(|(current, _)| current) != Some(name) {
				self.wifi_hover = Some((name.clone(), Instant::now()));
				self.wifi_popover = None;
			}
		} else if !matches!(hover, Some(Hit::Wifi(_))) {
			self.wifi_hover = None;
		}
		cursor_hand(matches!(
			hover,
			Some(
				Hit::Refresh
					| Hit::Scan
					| Hit::AllOn
					| Hit::AllOff
					| Hit::Settings
					| Hit::Power(_)
					| Hit::Brightness(_)
					| Hit::Warmth(_)
					| Hit::Menu(_)
					| Hit::Wifi(_)
					| Hit::SheetPrimary
					| Hit::SheetSecondary
					| Hit::SheetClose
					| Hit::SheetScanAgain
					| Hit::SheetCountDown
					| Hit::SheetCountUp
					| Hit::SheetLow
					| Hit::SheetPeak
					| Hit::SheetBrightness
					| Hit::SheetWarmth
					| Hit::DiscoveryAction(_)
			)
		));
		let drag = self.drag.take();
		match drag {
			Some(Drag::Slider { name, kind }) => {
				let value = self.slider_value_at(&name, kind, x);
				self.preview_slider(&name, kind, value);
				self.worker.send(match kind {
					LinkKind::Brightness => Command::QueueBrightness {
						name: name.clone(),
						value,
					},
					LinkKind::Warmth => Command::QueueWarmth {
						name: name.clone(),
						value,
					},
					_ => Command::Refresh,
				});
				self.drag = Some(Drag::Slider { name, kind });
				self.invalidate();
			}
			Some(Drag::Card {
				name,
				start_x,
				start_y,
				mut active,
				..
			}) => {
				let offset_x = x - start_x;
				let offset_y = y - start_y;
				if !active && offset_x.abs() + offset_y.abs() > 4.0 {
					active = true;
				}
				if active {
					self.drag_target = self.card_at(x, y).filter(|target| target != &name);
					self.invalidate();
				}
				self.drag = Some(Drag::Card {
					name,
					start_x,
					start_y,
					offset_x,
					offset_y,
					active,
				});
			}
			Some(Drag::SheetLow) => {
				let slider = self.sheet_rects().slider;
				if let Sheet::Pulse { low, peak, .. } = &mut self.sheet {
					*low = value_0_100(x, slider).min(*peak);
				}
				self.drag = Some(Drag::SheetLow);
				self.invalidate();
			}
			Some(Drag::SheetPeak) => {
				let slider = self.sheet_rects().slider;
				if let Sheet::Pulse { low, peak, .. } = &mut self.sheet {
					*peak = value_0_100(x, slider).max(*low).max(2);
				}
				self.drag = Some(Drag::SheetPeak);
				self.invalidate();
			}
			Some(Drag::SheetBrightness) => {
				let slider = self.sheet_rects().slider;
				if let Sheet::Sweep { brightness, .. } | Sheet::PowerOn { brightness, .. } =
					&mut self.sheet
				{
					let value = value_0_100(x, slider);
					*brightness = if value < 2 { 0 } else { value };
				}
				self.drag = Some(Drag::SheetBrightness);
				self.invalidate();
			}
			Some(Drag::SheetWarmth) => {
				let slider = Rect::new(
					self.sheet_rects().slider.x,
					self.sheet_rects().slider.y + 38.0,
					self.sheet_rects().slider.w,
					28.0,
				);
				if let Sheet::PowerOn { warmth, .. } = &mut self.sheet {
					*warmth = value_0_100(x, slider);
				}
				self.drag = Some(Drag::SheetWarmth);
				self.invalidate();
			}
			None => {}
		}
	}

	fn slider_value_at(&self, name: &str, kind: LinkKind, x: f32) -> u8 {
		let layout = self.layout();
		let Some(card) = layout.cards.iter().find(|card| card.name == name) else {
			return 50;
		};
		let rect = match kind {
			LinkKind::Brightness => card.brightness,
			LinkKind::Warmth => card.warmth,
			_ => return 50,
		};
		let slider = Rect {
			x: rect.x + 24.0,
			w: rect.w - 84.0,
			..rect
		};
		let t = ((x - slider.x - 8.0) / (slider.w - 16.0)).clamp(0.0, 1.0);
		match kind {
			LinkKind::Brightness => (2.0 + t * 98.0).round() as u8,
			LinkKind::Warmth => ((1.0 - t) * 100.0).round() as u8,
			_ => 50,
		}
	}

	fn card_at(&self, x: f32, y: f32) -> Option<String> {
		let layout = self.layout();
		layout
			.cards
			.iter()
			.find(|card| {
				Rect {
					y: card.card.y - self.scroll,
					..card.card
				}
				.contains(x, y)
			})
			.map(|card| card.name.clone())
	}

	fn on_lbutton_down(&mut self, x: f32, y: f32) {
		unsafe {
			SetCapture(self.window);
		}
		let ctrl = ctrl_down() || self.ctrl_down;
		self.ctrl_down = ctrl;
		let hit = self.hit_test(x, y);
		if ctrl
			&& let Some(hit) = &hit
			&& let Some((name, kind)) = link_target_of(hit)
		{
			self.worker.send(Command::ToggleLink { name, kind });
			self.invalidate();
			return;
		}
		match hit {
			Some(Hit::Brightness(name)) => {
				let value = self.slider_value_at(&name, LinkKind::Brightness, x);
				self.preview_slider(&name, LinkKind::Brightness, value);
				self.worker.send(Command::QueueBrightness {
					name: name.clone(),
					value,
				});
				self.drag = Some(Drag::Slider {
					name,
					kind: LinkKind::Brightness,
				});
			}
			Some(Hit::Warmth(name)) => {
				let value = self.slider_value_at(&name, LinkKind::Warmth, x);
				self.preview_slider(&name, LinkKind::Warmth, value);
				self.worker.send(Command::QueueWarmth {
					name: name.clone(),
					value,
				});
				self.drag = Some(Drag::Slider {
					name,
					kind: LinkKind::Warmth,
				});
			}
			Some(Hit::Card(name)) => {
				self.drag = Some(Drag::Card {
					name,
					start_x: x,
					start_y: y,
					offset_x: 0.0,
					offset_y: 0.0,
					active: false,
				});
			}
			Some(Hit::SheetLow) => self.drag = Some(Drag::SheetLow),
			Some(Hit::SheetPeak) => self.drag = Some(Drag::SheetPeak),
			Some(Hit::SheetBrightness) => self.drag = Some(Drag::SheetBrightness),
			Some(Hit::SheetWarmth) => self.drag = Some(Drag::SheetWarmth),
			Some(Hit::Wifi(name)) => {
				self.wifi_popover = if self.wifi_popover.as_deref() == Some(name.as_str()) {
					None
				} else {
					Some(name)
				};
			}
			Some(hit) => {
				self.pressed = Some(hit);
			}
			None => {}
		}
		self.invalidate();
	}

	fn on_lbutton_up(&mut self, x: f32, y: f32) {
		unsafe {
			let _ = ReleaseCapture();
		}
		if let Some(drag) = self.drag.take() {
			match drag {
				Drag::Slider { name, kind } => {
					let value = match kind {
						LinkKind::Brightness => self
							.preview
							.get(&(name.clone(), kind))
							.copied()
							.unwrap_or(50),
						LinkKind::Warmth => self
							.preview
							.get(&(name.clone(), kind))
							.copied()
							.unwrap_or(50),
						_ => 50,
					};
					self.worker.send(match kind {
						LinkKind::Brightness => Command::QueueBrightness { name, value },
						LinkKind::Warmth => Command::QueueWarmth { name, value },
						_ => Command::Refresh,
					});
				}
				Drag::Card {
					name, active: true, ..
				} => {
					if let Some(target) = self.drag_target.clone() {
						self.worker.send(Command::Move { name, onto: target });
					}
				}
				_ => {}
			}
		}
		self.drag_target = None;
		self.preview_until = Some(Instant::now() + std::time::Duration::from_millis(700));
		if let Some(hit) = self.pressed.take()
			&& self.hit_test(x, y) == Some(hit.clone())
		{
			self.activate(hit);
		}
		self.invalidate();
	}

	fn activate(&mut self, hit: Hit) {
		match hit {
			Hit::Refresh => self.worker.send(Command::Refresh),
			Hit::Scan => {
				self.sheet = Sheet::Discovery;
				self.worker.send(Command::Scan);
				self.resize_to_content(true);
				self.position();
				self.invalidate();
			}
			Hit::AllOn => self.worker.send(Command::SetAllOn(true)),
			Hit::AllOff => self.worker.send(Command::SetAllOn(false)),
			Hit::Settings => {
				let layout = self.layout();
				if let Some((rect, _)) =
					layout.buttons.iter().find(|(_, hit)| *hit == Hit::Settings)
				{
					self.open_settings_menu(POINT {
						x: scale_i32(self.dpi, rect.x),
						y: scale_i32(self.dpi, rect.bottom()),
					});
				}
			}
			Hit::Card(name) => {
				let now = Instant::now();
				if self
					.last_click
					.as_ref()
					.is_some_and(|(last, at)| last == &name && at.elapsed().as_millis() < 400)
				{
					self.last_click = None;
					self.worker.send(Command::Toggle(name));
				} else {
					self.last_click = Some((name, now));
				}
			}
			Hit::Power(name) => {
				let on = self
					.snapshot
					.devices
					.iter()
					.find(|device| device.name == name)
					.is_some_and(LightDevice::is_effectively_on);
				self.worker.send(Command::SetOn { name, on: !on });
			}
			Hit::Menu(name) => {
				let layout = self.layout();
				if let Some(card) = layout.cards.iter().find(|card| card.name == name) {
					self.open_card_menu(
						&name,
						POINT {
							x: scale_i32(self.dpi, card.menu.x),
							y: scale_i32(self.dpi, card.menu.bottom() - self.scroll),
						},
					);
				}
			}
			Hit::SheetClose
			| Hit::SheetPrimary
			| Hit::SheetSecondary
			| Hit::SheetScanAgain
			| Hit::SheetCountDown
			| Hit::SheetCountUp
			| Hit::SheetRadio(_)
			| Hit::SheetLow
			| Hit::SheetPeak
			| Hit::SheetBrightness
			| Hit::SheetWarmth => self.sheet_action(hit),
			Hit::DiscoveryAction(id) => self.discovery_action(&id),
			Hit::Brightness(_) | Hit::Warmth(_) | Hit::Wifi(_) => {}
		}
	}

	fn on_mouse_wheel(&mut self, delta: i32) {
		let layout = self.layout();
		let (_, height) = self.dip_size();
		if layout.content_height <= height {
			return;
		}
		self.scroll =
			(self.scroll - delta as f32 / 120.0 * 48.0).clamp(0.0, layout.content_height - height);
		self.invalidate();
	}
}

fn colorref(color: u32) -> COLORREF {
	let red = ((color >> 16) & 0xFF) as u8;
	let green = ((color >> 8) & 0xFF) as u8;
	let blue = (color & 0xFF) as u8;
	COLORREF(u32::from(red) | (u32::from(green) << 8) | (u32::from(blue) << 16))
}

fn value_0_100(x: f32, rect: Rect) -> u8 {
	let t = ((x - rect.x - 8.0) / (rect.w - 16.0)).clamp(0.0, 1.0);
	(t * 100.0).round() as u8
}

fn link_target_of(hit: &Hit) -> Option<(String, LinkKind)> {
	match hit {
		Hit::Card(name) => Some((name.clone(), LinkKind::Card)),
		Hit::Power(name) => Some((name.clone(), LinkKind::Power)),
		Hit::Brightness(name) => Some((name.clone(), LinkKind::Brightness)),
		Hit::Warmth(name) => Some((name.clone(), LinkKind::Warmth)),
		_ => None,
	}
}

unsafe extern "system" fn edit_proc(
	window: HWND,
	message: u32,
	wparam: WPARAM,
	lparam: LPARAM,
	_id: usize,
	_data: usize,
) -> LRESULT {
	match message {
		WM_KEYDOWN if wparam.0 == VK_RETURN.0 as usize || wparam.0 == VK_ESCAPE.0 as usize => {
			let parent = unsafe { GetParent(window) }.unwrap_or_default();
			unsafe {
				let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
					Some(parent),
					WM_KEYDOWN,
					wparam,
					LPARAM(0),
				);
			}
			LRESULT(0)
		}
		WM_CHAR if wparam.0 == 13 || wparam.0 == 27 => LRESULT(0),
		_ => unsafe { DefSubclassProc(window, message, wparam, lparam) },
	}
}

unsafe extern "system" fn window_proc(
	window: HWND,
	message: u32,
	wparam: WPARAM,
	lparam: LPARAM,
) -> LRESULT {
	match message {
		WM_NCCREATE => LRESULT(1),
		WM_PAINT => {
			if let Some(app) = app_mut(window) {
				app.on_paint();
			}
			unsafe {
				let _ = ValidateRect(Some(window), None);
			}
			LRESULT(0)
		}
		WM_APP_UPDATE => {
			if let Some(app) = app_mut(window) {
				app.refresh_snapshot();
				app.resize_to_content(true);
				app.invalidate();
			}
			LRESULT(0)
		}
		WM_APP_SHOW => {
			if let Some(app) = app_mut(window) {
				app.show();
			}
			LRESULT(0)
		}
		WM_APP_TRAY => {
			if let Some(app) = app_mut(window) {
				match loword(lparam.0 as usize) as u32 {
					WM_LBUTTONUP => app.toggle(),
					WM_RBUTTONUP | WM_CONTEXTMENU => app.show_tray_menu(cursor_position()),
					_ => {}
				}
			}
			LRESULT(0)
		}
		WM_TIMER => {
			if let Some(app) = app_mut(window) {
				if wparam.0 == TIMER_REFRESH {
					if unsafe { IsWindowVisible(window).as_bool() } {
						app.worker.send(Command::Refresh);
					}
				} else if wparam.0 == TIMER_UI {
					app.on_tick();
				}
			}
			LRESULT(0)
		}
		WM_CLOSE => {
			if let Some(app) = app_mut(window) {
				app.hide();
			}
			LRESULT(0)
		}
		WM_QUERYENDSESSION => LRESULT(1),
		WM_ENDSESSION => {
			if let Some(app) = app_mut(window)
				&& wparam.0 != 0
			{
				app.quitting = true;
				unsafe {
					PostQuitMessage(0);
				}
			}
			LRESULT(0)
		}
		WM_DESTROY => {
			if let Some(app) = app_mut(window)
				&& !app.quitting
			{
				app.hide();
				return LRESULT(0);
			}
			unsafe {
				PostQuitMessage(0);
			}
			LRESULT(0)
		}
		WM_ACTIVATE => {
			if let Some(app) = app_mut(window)
				&& loword(wparam.0) as u32 == WA_INACTIVE
				&& !app.dragging()
				&& !app.menu_open
				&& !app.no_hide
				&& app.sheet == Sheet::None
			{
				app.hide();
			}
			LRESULT(0)
		}
		WM_MOUSEMOVE => {
			if let Some(app) = app_mut(window) {
				let dpi = app.dpi as f32;
				let x = unscale(app.dpi, (lparam.0 & 0xFFFF) as i16 as i32);
				let y = unscale(app.dpi, ((lparam.0 >> 16) & 0xFFFF) as i16 as i32);
				let _ = dpi;
				app.on_mouse_move(x, y);
			}
			LRESULT(0)
		}
		WM_MOUSELEAVE => {
			if let Some(app) = app_mut(window) {
				app.hover = None;
				app.wifi_hover = None;
				app.wifi_popover = None;
				app.invalidate();
			}
			LRESULT(0)
		}
		WM_LBUTTONDOWN => {
			if let Some(app) = app_mut(window) {
				let x = unscale(app.dpi, (lparam.0 & 0xFFFF) as i16 as i32);
				let y = unscale(app.dpi, ((lparam.0 >> 16) & 0xFFFF) as i16 as i32);
				app.on_lbutton_down(x, y);
			}
			LRESULT(0)
		}
		WM_LBUTTONUP => {
			if let Some(app) = app_mut(window) {
				let x = unscale(app.dpi, (lparam.0 & 0xFFFF) as i16 as i32);
				let y = unscale(app.dpi, ((lparam.0 >> 16) & 0xFFFF) as i16 as i32);
				app.on_lbutton_up(x, y);
			}
			LRESULT(0)
		}
		WM_MOUSEWHEEL => {
			if let Some(app) = app_mut(window) {
				let delta = ((wparam.0 >> 16) & 0xFFFF) as i16 as i32;
				app.on_mouse_wheel(delta);
			}
			LRESULT(0)
		}
		WM_KEYDOWN => {
			if let Some(app) = app_mut(window) {
				match wparam.0 as u32 {
					value if value == VK_ESCAPE.0 as u32 => {
						if app.sheet != Sheet::None {
							app.cancel_sheet_animation();
							app.close_sheet();
						} else {
							app.hide();
						}
					}
					value if value == VK_RETURN.0 as u32 => {
						if app.sheet != Sheet::None {
							app.sheet_primary();
						}
					}
					value if value == VK_CONTROL.0 as u32 => {
						app.ctrl_down = true;
						app.invalidate();
					}
					_ => {}
				}
			}
			LRESULT(0)
		}
		WM_KEYUP => {
			if let Some(app) = app_mut(window)
				&& wparam.0 as u32 == VK_CONTROL.0 as u32
			{
				app.ctrl_down = false;
				app.worker.send(Command::CommitLinks);
				app.invalidate();
			}
			LRESULT(0)
		}
		WM_KILLFOCUS => {
			if let Some(app) = app_mut(window) {
				if app.ctrl_down {
					app.worker.send(Command::CommitLinks);
					app.ctrl_down = false;
				}
				app.invalidate();
			}
			LRESULT(0)
		}
		WM_COMMAND => {
			if let Some(app) = app_mut(window)
				&& hiword(wparam.0) == 0x0300
				&& app
					.edit
					.as_ref()
					.is_some_and(|edit| edit.window.0 == lparam.0 as *mut _)
			{
				app.read_edit();
			}
			LRESULT(0)
		}
		WM_CTLCOLOREDIT => {
			if let Some(app) = app_mut(window)
				&& let Some(edit) = &app.edit
			{
				let hdc = HDC(wparam.0 as *mut _);
				unsafe {
					let _ = SetTextColor(hdc, colorref(app.theme.text));
					let _ = SetBkColor(hdc, colorref(app.theme.edit_bg));
				}
				return LRESULT(edit.brush.0 as isize);
			}
			unsafe { DefWindowProcW(window, message, wparam, lparam) }
		}
		WM_DPICHANGED => {
			if let Some(app) = app_mut(window) {
				app.dpi = dpi(window);
				app.theme = Theme::detect();
				apply_dark_mode(window, app.theme.dark);
				app.resize_to_content(true);
				if app.edit.is_some() {
					app.destroy_edit();
					app.create_sheet_edit();
				}
				app.invalidate();
			}
			LRESULT(0)
		}
		WM_SETTINGCHANGE => {
			if let Some(app) = app_mut(window) {
				app.theme = Theme::detect();
				apply_dark_mode(window, app.theme.dark);
				if app.edit.is_some() {
					app.destroy_edit();
					app.create_sheet_edit();
				}
				app.invalidate();
			}
			LRESULT(0)
		}
		_ => {
			if let Some(app) = app_mut(window)
				&& app.taskbar_created != 0
				&& message == app.taskbar_created
			{
				app.tray.readd();
				return LRESULT(0);
			}
			unsafe { DefWindowProcW(window, message, wparam, lparam) }
		}
	}
}

impl App {
	fn dragging(&self) -> bool {
		self.drag.is_some()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn slider_geometry_maps_to_a_percentage() {
		let rect = Rect::new(0.0, 0.0, 100.0, 20.0);
		assert_eq!(value_0_100(8.0, rect), 0);
		assert_eq!(value_0_100(92.0, rect), 100);
		assert_eq!(value_0_100(50.0, rect), 50);
	}

	#[test]
	fn colour_reference_uses_windows_channel_order() {
		assert_eq!(colorref(0x0011_2233).0, 0x0033_2211);
	}

	#[test]
	fn link_hits_map_to_their_field() {
		assert_eq!(
			link_target_of(&Hit::Brightness("Left".into())),
			Some(("Left".to_string(), LinkKind::Brightness))
		);
		assert_eq!(
			link_target_of(&Hit::Card("Left".into())),
			Some(("Left".to_string(), LinkKind::Card))
		);
		assert_eq!(link_target_of(&Hit::Refresh), None);
	}
}
