use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const FILE_NAME: &str = "com.bevry.elopli.desktop";

/// The XDG autostart entry for this user.
pub fn entry_path() -> PathBuf {
	gtk::glib::user_config_dir()
		.join("autostart")
		.join(FILE_NAME)
}

pub fn enabled() -> bool {
	enabled_at(&entry_path())
}

pub fn set(enabled: bool) -> Result<(), String> {
	set_at(&entry_path(), &current_executable(), enabled)
}

fn current_executable() -> PathBuf {
	std::env::current_exe().unwrap_or_else(|_| PathBuf::from("elopli-app"))
}

fn enabled_at(path: &Path) -> bool {
	let Ok(contents) = fs::read_to_string(path) else {
		return false;
	};
	!contents.contains("Hidden=true") && !contents.contains("X-GNOME-Autostart-enabled=false")
}

fn set_at(path: &Path, executable: &Path, enabled: bool) -> Result<(), String> {
	if !enabled {
		return match fs::remove_file(path) {
			Ok(()) => Ok(()),
			Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
			Err(error) => Err(format!("cannot remove {}: {error}", path.display())),
		};
	}

	let parent = path
		.parent()
		.ok_or_else(|| format!("{} has no parent directory", path.display()))?;
	fs::create_dir_all(parent)
		.map_err(|error| format!("cannot create {}: {error}", parent.display()))?;

	let timestamp = SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.map(|duration| duration.as_nanos())
		.unwrap_or_default();
	let temporary = parent.join(format!(
		".{FILE_NAME}.tmp-{}-{timestamp}",
		std::process::id()
	));
	let contents = format!(
		"[Desktop Entry]\n\
         Type=Application\n\
         Name=Elopli\n\
         Comment=Control Elgato Key Light devices\n\
         Exec={} --autostart\n\
         Icon=com.bevry.elopli\n\
         Terminal=false\n\
         X-GNOME-Autostart-enabled=true\n",
		quote_exec(executable)
	);

	let result = (|| {
		fs::write(&temporary, contents.as_bytes())
			.map_err(|error| format!("cannot write {}: {error}", temporary.display()))?;
		fs::rename(&temporary, path)
			.map_err(|error| format!("cannot replace {}: {error}", path.display()))
	})();
	if result.is_err() {
		let _ = fs::remove_file(&temporary);
	}
	result
}

/// Quote one argument for a desktop entry `Exec` line.
fn quote_exec(path: &Path) -> String {
	let value = path.to_string_lossy();
	let mut quoted = String::with_capacity(value.len() + 2);
	quoted.push('"');
	for character in value.chars() {
		if matches!(
			character,
			' ' | '\t'
				| '\n'
				| '"'
				| '\''
				| '\\'
				| '>'
				| '<'
				| '~'
				| '|'
				| '&'
				| ';'
				| '$'
				| '*'
				| '?'
				| '#'
				| '('
				| ')'
				| '`'
		) {
			quoted.push('\\');
		}
		quoted.push(character);
	}
	quoted.push('"');
	quoted
}

#[cfg(test)]
mod tests {
	use super::*;

	fn temporary_directory() -> PathBuf {
		let directory = std::env::temp_dir().join(format!(
			"elopli-autostart-{}-{}",
			std::process::id(),
			SystemTime::now()
				.duration_since(UNIX_EPOCH)
				.map(|duration| duration.as_nanos())
				.unwrap_or_default()
		));
		fs::create_dir_all(&directory).unwrap();
		directory
	}

	#[test]
	fn writes_and_removes_the_entry() {
		let directory = temporary_directory();
		let path = directory.join(FILE_NAME);
		let executable = directory.join("Elopli App");

		assert!(!enabled_at(&path));
		set_at(&path, &executable, true).unwrap();
		assert!(enabled_at(&path));
		let contents = fs::read_to_string(&path).unwrap();
		assert!(contents.contains("--autostart"));
		assert!(contents.contains("X-GNOME-Autostart-enabled=true"));
		assert!(contents.contains("Elopli\\ App"));

		set_at(&path, &executable, false).unwrap();
		assert!(!enabled_at(&path));
		assert!(!path.exists());
	}

	#[test]
	fn disabled_entries_are_not_enabled() {
		let directory = temporary_directory();
		let path = directory.join(FILE_NAME);
		fs::write(&path, "X-GNOME-Autostart-enabled=false\n").unwrap();
		assert!(!enabled_at(&path));
		fs::write(&path, "Hidden=true\n").unwrap();
		assert!(!enabled_at(&path));
	}

	#[test]
	fn quoting_escapes_reserved_characters() {
		assert_eq!(
			quote_exec(Path::new("/usr/bin/elopli")),
			"\"/usr/bin/elopli\""
		);
		assert_eq!(
			quote_exec(Path::new("/home/a b/elopli")),
			"\"/home/a\\ b/elopli\""
		);
		assert_eq!(
			quote_exec(Path::new("/home/a\"b/elopli")),
			"\"/home/a\\\"b/elopli\""
		);
	}
}
