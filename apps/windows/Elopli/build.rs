use std::path::Path;
use std::process::Command;

fn main() {
	println!("cargo:rerun-if-changed=assets/elopli.ico");

	if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
		return;
	}

	let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".into());
	let parts = version
		.split('.')
		.map(|part| {
			part.split(['-', '+'])
				.next()
				.and_then(|value| value.parse::<u16>().ok())
				.unwrap_or(0)
		})
		.collect::<Vec<_>>();
	let version4 = format!(
		"{},{},{},0",
		parts.first().copied().unwrap_or(0),
		parts.get(1).copied().unwrap_or(0),
		parts.get(2).copied().unwrap_or(0)
	);

	let icon = Path::new("assets/elopli.ico")
		.canonicalize()
		.expect("the Windows icon must exist")
		.display()
		.to_string();
	let icon = icon
		.strip_prefix(r"\\?\")
		.unwrap_or(&icon)
		.replace('\\', "/");
	let resource = format!(
		r#"1 ICON "{icon}"
1 VERSIONINFO
FILEVERSION {version4}
PRODUCTVERSION {version4}
FILEOS 0x40004
FILETYPE 0x1
BEGIN
	BLOCK "StringFileInfo"
	BEGIN
		BLOCK "040904B0"
		BEGIN
			VALUE "CompanyName", "Benjamin Lupton"
			VALUE "FileDescription", "Elopli"
			VALUE "FileVersion", "{version}"
			VALUE "InternalName", "Elopli"
			VALUE "LegalCopyright", "Copyright (C) Benjamin Lupton. RPL-1.5."
			VALUE "OriginalFilename", "ElopliApp.exe"
			VALUE "ProductName", "Elopli"
			VALUE "ProductVersion", "{version}"
		END
	END
	BLOCK "VarFileInfo"
	BEGIN
		VALUE "Translation", 0x409, 1200
	END
END
"#
	);

	let out = std::env::var("OUT_DIR").expect("OUT_DIR is set");
	let resource_path = Path::new(&out).join("elopli.rc");
	std::fs::write(&resource_path, resource).expect("write the Windows resource script");

	let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
	if target_env == "msvc" {
		// rc.exe is linked directly as a .res file.
		embed_resource::compile(&resource_path, embed_resource::NONE)
			.manifest_optional()
			.expect("compile the Windows resources");
	} else {
		// windres produces an object, not an archive. An archive would not be
		// pulled in by the GNU linker because resource objects export no
		// symbols.
		let object = Path::new(&out).join("elopli-resource.o");
		let output = Command::new("windres")
			.arg("-i")
			.arg(&resource_path)
			.arg("-o")
			.arg(&object)
			.arg("-O")
			.arg("coff")
			.arg("-c")
			.arg("65001")
			.output()
			.expect("run windres");
		if !output.status.success() {
			panic!(
				"windres failed: {}{}",
				String::from_utf8_lossy(&output.stdout),
				String::from_utf8_lossy(&output.stderr)
			);
		}
		println!("cargo:rustc-link-arg-bins={}", object.display());
	}
}
