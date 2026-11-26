use std::mem::size_of;

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Shell::{
	NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::{
	HICON, IDI_APPLICATION, IMAGE_ICON, LR_DEFAULTSIZE, LR_SHARED, LoadIconW, LoadImageW,
};
use windows::core::PCWSTR;

use crate::win32::{TRAY_ID, WM_APP_TRAY, wide};

pub struct Tray {
	window: HWND,
	added: bool,
	tooltip: String,
}

impl Tray {
	pub fn add(window: HWND) -> Self {
		let mut tray = Self {
			window,
			added: false,
			tooltip: "Elopli".to_string(),
		};
		tray.install();
		tray
	}

	#[allow(clippy::manual_dangling_ptr)]
	fn data(&self) -> NOTIFYICONDATAW {
		let mut data = NOTIFYICONDATAW {
			cbSize: size_of::<NOTIFYICONDATAW>() as u32,
			hWnd: self.window,
			uID: TRAY_ID,
			uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
			uCallbackMessage: WM_APP_TRAY,
			..Default::default()
		};
		let icon = unsafe {
			LoadImageW(
				None,
				PCWSTR(1 as *const u16),
				IMAGE_ICON,
				0,
				0,
				LR_DEFAULTSIZE | LR_SHARED,
			)
		};
		if let Ok(icon) = icon {
			data.hIcon = HICON(icon.0);
		} else if let Ok(icon) = unsafe { LoadIconW(None, IDI_APPLICATION) } {
			data.hIcon = icon;
		}
		let tooltip = wide(&self.tooltip);
		let count = tooltip.len().min(data.szTip.len());
		data.szTip[..count].copy_from_slice(&tooltip[..count]);
		data
	}

	fn install(&mut self) {
		let data = self.data();
		unsafe {
			self.added = Shell_NotifyIconW(NIM_ADD, &data).as_bool();
		}
	}

	pub fn readd(&mut self) {
		if self.added {
			let data = self.data();
			unsafe {
				let _ = Shell_NotifyIconW(NIM_DELETE, &data);
			}
		}
		self.install();
	}

	pub fn remove(&mut self) {
		if self.added {
			let data = self.data();
			unsafe {
				let _ = Shell_NotifyIconW(NIM_DELETE, &data);
			}
			self.added = false;
		}
	}

	pub fn set_tooltip(&mut self, tooltip: impl Into<String>) {
		let tooltip = tooltip.into();
		if tooltip == self.tooltip {
			return;
		}
		self.tooltip = tooltip;
		let data = self.data();
		unsafe {
			let _ = Shell_NotifyIconW(windows::Win32::UI::Shell::NIM_MODIFY, &data);
		}
	}
}

impl Drop for Tray {
	fn drop(&mut self) {
		self.remove();
	}
}
