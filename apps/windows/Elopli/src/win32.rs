use std::ffi::c_void;
use std::mem::size_of;
use std::path::Path;
use std::process::Command;

use windows::Win32::Foundation::{
	CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, HINSTANCE, HWND, LPARAM, POINT, RECT,
	WPARAM,
};
use windows::Win32::Graphics::Dwm::{
	DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWA_WINDOW_CORNER_PREFERENCE, DwmSetWindowAttribute,
};
use windows::Win32::Graphics::Gdi::{
	GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MONITORINFOEXW, MonitorFromPoint,
};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::Controls::SetWindowTheme;
use windows::Win32::UI::HiDpi::{
	DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForWindow, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_CONTROL};
use windows::Win32::UI::Shell::{
	NOTIFYICONIDENTIFIER, SetCurrentProcessExplicitAppUserModelID, Shell_NotifyIconGetRect,
};
use windows::Win32::UI::WindowsAndMessaging::{
	FindWindowW, GetClientRect, HICON, IDC_ARROW, IDC_HAND, IMAGE_ICON, LR_DEFAULTSIZE, LR_SHARED,
	LoadCursorW, LoadImageW, PostMessageW, SW_SHOW, SetCursor, SetForegroundWindow, ShowWindow,
};
use windows::core::{BOOL, GUID, HSTRING, PCWSTR};
use winreg::RegKey;
use winreg::enums::HKEY_CURRENT_USER;

pub const TRAY_ID: u32 = 1;
pub const WM_APP_TRAY: u32 = 0x8000 + 1;
pub const WM_APP_UPDATE: u32 = 0x8000 + 2;
pub const WM_APP_SHOW: u32 = 0x8000 + 3;

/// Load the Elopli icon resource from this executable.
///
/// `LoadImageW` with a null instance treats the name as a system resource, so
/// the tray used to fall back to a generic shell icon even though the icon was
/// embedded correctly. Pass the module handle explicitly.
#[allow(clippy::manual_dangling_ptr)]
pub fn app_icon() -> Option<HICON> {
	let module = unsafe { GetModuleHandleW(None) }.ok()?;
	let icon = unsafe {
		LoadImageW(
			Some(HINSTANCE(module.0)),
			PCWSTR(1 as *const u16),
			IMAGE_ICON,
			0,
			0,
			LR_DEFAULTSIZE | LR_SHARED,
		)
	}
	.ok()?;
	Some(HICON(icon.0))
}

pub struct SingleInstance {
	handle: HANDLE,
}

impl Drop for SingleInstance {
	fn drop(&mut self) {
		unsafe {
			let _ = CloseHandle(self.handle);
		}
	}
}

pub fn wide(value: &str) -> Vec<u16> {
	value.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn pcwstr(value: &[u16]) -> PCWSTR {
	PCWSTR(value.as_ptr())
}

pub fn loword(value: usize) -> u16 {
	(value & 0xFFFF) as u16
}

pub fn hiword(value: usize) -> u16 {
	((value >> 16) & 0xFFFF) as u16
}

pub fn scale(dpi: u32, dip: f32) -> f32 {
	dip * dpi as f32 / 96.0
}

pub fn scale_i32(dpi: u32, dip: f32) -> i32 {
	scale(dpi, dip).round() as i32
}

pub fn unscale(dpi: u32, pixels: i32) -> f32 {
	pixels as f32 * 96.0 / dpi as f32
}

pub fn init_process() {
	unsafe {
		let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
		let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
		let _ = SetCurrentProcessExplicitAppUserModelID(&HSTRING::from("com.bevry.elopli"));
	}
}

pub fn single_instance(name: &str) -> Option<SingleInstance> {
	let name = wide(name);
	unsafe {
		match CreateMutexW(None, false, pcwstr(&name)) {
			Ok(handle) if GetLastError() != ERROR_ALREADY_EXISTS => Some(SingleInstance { handle }),
			Ok(handle) => {
				let _ = CloseHandle(handle);
				None
			}
			Err(_) => None,
		}
	}
}

pub fn show_existing_window(class: &str) {
	let class = wide(class);
	unsafe {
		if let Ok(window) = FindWindowW(pcwstr(&class), PCWSTR::null()) {
			let _ = ShowWindow(window, SW_SHOW);
			let _ = SetForegroundWindow(window);
			let _ = PostMessageW(Some(window), WM_APP_SHOW, WPARAM(0), LPARAM(0));
		}
	}
}

pub fn cursor_position() -> POINT {
	use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
	let mut point = POINT::default();
	unsafe {
		let _ = GetCursorPos(&mut point);
	}
	point
}

pub fn work_area(point: POINT) -> RECT {
	let monitor = unsafe { MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST) };
	let mut info = MONITORINFOEXW::default();
	info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
	let mut work = RECT::default();
	if unsafe { GetMonitorInfoW(monitor, &mut info.monitorInfo as *mut MONITORINFO) }.as_bool() {
		work = info.monitorInfo.rcWork;
	}
	work
}

pub fn tray_rect(window: HWND) -> Option<RECT> {
	let identifier = NOTIFYICONIDENTIFIER {
		cbSize: size_of::<NOTIFYICONIDENTIFIER>() as u32,
		hWnd: window,
		uID: TRAY_ID,
		guidItem: GUID::zeroed(),
	};
	unsafe { Shell_NotifyIconGetRect(&identifier).ok() }
}

pub fn client_size(window: HWND) -> (i32, i32) {
	let mut rect = RECT::default();
	unsafe {
		let _ = GetClientRect(window, &mut rect);
	}
	(rect.right - rect.left, rect.bottom - rect.top)
}

pub fn dpi(window: HWND) -> u32 {
	let value = unsafe { GetDpiForWindow(window) };
	value.max(96)
}

pub fn ctrl_down() -> bool {
	unsafe { GetKeyState(VK_CONTROL.0 as i32) < 0 }
}

pub fn cursor_hand(hand: bool) {
	unsafe {
		let cursor = LoadCursorW(None, if hand { IDC_HAND } else { IDC_ARROW });
		if let Ok(cursor) = cursor {
			let _ = SetCursor(Some(cursor));
		}
	}
}

pub fn apply_dark_mode(window: HWND, dark: bool) {
	let value = BOOL::from(dark);
	unsafe {
		let _ = DwmSetWindowAttribute(
			window,
			DWMWA_USE_IMMERSIVE_DARK_MODE,
			&value as *const _ as *const c_void,
			size_of::<BOOL>() as u32,
		);
		let corner = 2u32;
		let _ = DwmSetWindowAttribute(
			window,
			DWMWA_WINDOW_CORNER_PREFERENCE,
			&corner as *const _ as *const c_void,
			size_of::<u32>() as u32,
		);
	}
	let theme = wide(if dark {
		"DarkMode_Explorer"
	} else {
		"Explorer"
	});
	unsafe {
		let _ = SetWindowTheme(window, pcwstr(&theme), PCWSTR::null());
	}
}

pub fn autostart_enabled() -> bool {
	let key = RegKey::predef(HKEY_CURRENT_USER).open_subkey(RUN_KEY);
	key.is_ok_and(|key| key.get_value::<String, _>(RUN_VALUE).is_ok())
}

pub fn set_autostart(enabled: bool) -> std::io::Result<()> {
	let (key, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(RUN_KEY)?;
	if enabled {
		let executable = std::env::current_exe()?;
		let value = format!("\"{}\" --autostart", executable.display());
		key.set_value(RUN_VALUE, &value)
	} else if key.get_value::<String, _>(RUN_VALUE).is_ok() {
		key.delete_value(RUN_VALUE)
	} else {
		Ok(())
	}
}

pub fn message_box(title: &str, text: &str) {
	let title = wide(title);
	let text = wide(text);
	unsafe {
		let _ = windows::Win32::UI::WindowsAndMessaging::MessageBoxW(
			None,
			pcwstr(&text),
			pcwstr(&title),
			windows::Win32::UI::WindowsAndMessaging::MB_OK
				| windows::Win32::UI::WindowsAndMessaging::MB_ICONERROR,
		);
	}
}

pub fn reveal_config(path: &Path) {
	let target = if path.exists() {
		format!("/select,{}", path.display())
	} else {
		path.parent()
			.map_or_else(|| ".".to_string(), |parent| parent.display().to_string())
	};
	let _ = Command::new("explorer").arg(target).spawn();
}

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const RUN_VALUE: &str = "Elopli";
